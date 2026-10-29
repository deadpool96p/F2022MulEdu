#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <winsock2.h>
#include <ws2tcpip.h>
#include <MinHook.h>
#include <blake3.h>

#include <iostream>
#include <fstream>
#include <string>
#include <vector>
#include <atomic>
#include <thread>
#include <chrono>
#include <cstring>
#include <filesystem>

#include "overlay.h"

#pragma comment(lib, "ws2_32.lib")

namespace fs = std::filesystem;

// -----------------------------------------------------------------------------
// Protocol Constants (Matching RELAY_SERVER_SPEC.md)
// -----------------------------------------------------------------------------
static const uint8_t PROTOCOL_MAGIC[4] = {'S', 'F', '2', '2'};
static const uint8_t PROTOCOL_VERSION = 0x01;
static const size_t HEADER_LEN = 46;

enum class MessageType : uint8_t {
    HandshakeInit    = 0x01,
    HandshakeAck     = 0x02,
    GameData         = 0x03,
    HeartbeatPing    = 0x04,
    HeartbeatPong    = 0x05,
    PeerStatusNotify = 0x06,
    Disconnect       = 0x07
};

// -----------------------------------------------------------------------------
// Configuration & State
// -----------------------------------------------------------------------------
struct Config {
    std::string relay_host = "127.0.0.1";
    uint16_t relay_port = 27015;
    std::string session_password = "fifa_coop_room";
    std::string role = "host";
    bool enable_overlay = true;
};

static Config g_config;
static uint8_t g_room_hash[32] = {0};
static sockaddr_in g_relay_addr{};

static std::atomic<SOCKET> g_game_socket{INVALID_SOCKET};
static std::atomic<uint8_t> g_peer_id{0}; // 0 = Host, 1 = Guest
static std::atomic<uint32_t> g_seq_num{1};
static std::atomic<uint64_t> g_packets_sent{0};
static std::atomic<uint64_t> g_packets_recv{0};
static std::atomic<uint32_t> g_rtt_ms{0};
static std::atomic<bool> g_is_connected{false};
static std::atomic<bool> g_is_peer_connected{false};
static std::atomic<bool> g_running{true};

static std::thread g_heartbeat_thread;

// -----------------------------------------------------------------------------
// Helper: Load Configuration & Compute Room Hash
// -----------------------------------------------------------------------------
static std::string Trim(const std::string& str) {
    size_t first = str.find_first_not_of(" \t\r\n");
    if (first == std::string::npos) return "";
    size_t last = str.find_last_not_of(" \t\r\n");
    return str.substr(first, (last - first + 1));
}

static void LoadConfig() {
    std::ifstream file("seamless.ini");
    if (file.is_open()) {
        std::string line;
        while (std::getline(file, line)) {
            line = Trim(line);
            if (line.empty() || line[0] == ';' || line[0] == '#') continue;
            size_t eq = line.find('=');
            if (eq == std::string::npos) continue;

            std::string key = Trim(line.substr(0, eq));
            std::string val = Trim(line.substr(eq + 1));
            if (val.size() >= 2 && val.front() == '"' && val.back() == '"') {
                val = val.substr(1, val.size() - 2);
            }

            if (key == "relay_host" || key == "server_ip") g_config.relay_host = val;
            else if (key == "relay_port" || key == "server_port") g_config.relay_port = static_cast<uint16_t>(std::stoi(val));
            else if (key == "session_password" || key == "password") g_config.session_password = val;
            else if (key == "role") g_config.role = val;
            else if (key == "enable_overlay") g_config.enable_overlay = (val == "true" || val == "1");
        }
    }

    // Set peer role default
    if (g_config.role == "guest") {
        g_peer_id.store(1);
    } else {
        g_peer_id.store(0);
    }

    // Compute BLAKE3 Room Hash: BLAKE3("FIFA22_SEAMLESS_SALT:" + password)
    blake3_hasher hasher;
    blake3_hasher_init(&hasher);
    const char salt[] = "FIFA22_SEAMLESS_SALT:";
    blake3_hasher_update(&hasher, salt, sizeof(salt) - 1);
    blake3_hasher_update(&hasher, g_config.session_password.c_str(), g_config.session_password.length());
    blake3_hasher_finalize(&hasher, g_room_hash, 32);

    // Resolve Relay Address
    std::memset(&g_relay_addr, 0, sizeof(g_relay_addr));
    g_relay_addr.sin_family = AF_INET;
    g_relay_addr.sin_port = htons(g_config.relay_port);
    inet_pton(AF_INET, g_config.relay_host.c_str(), &g_relay_addr.sin_addr);

    // Initialize HUD state
    SeamlessHudState hud{};
    hud.room_name = g_config.session_password;
    hud.status_text = "Waiting for Game Socket...";
    hud.role_text = (g_peer_id.load() == 0) ? "Host (Peer 0)" : "Guest (Peer 1)";
    hud.peer_id = g_peer_id.load();
    UpdateHudState(hud);
}

// -----------------------------------------------------------------------------
// Envelope Packaging Helpers
// -----------------------------------------------------------------------------
static void BuildEnvelope(
    MessageType msg_type,
    uint8_t peer_id,
    uint32_t seq,
    const void* payload,
    uint16_t payload_len,
    std::vector<uint8_t>& out_buf
) {
    out_buf.resize(HEADER_LEN + payload_len);
    std::memcpy(&out_buf[0], PROTOCOL_MAGIC, 4);
    out_buf[4] = PROTOCOL_VERSION;
    out_buf[5] = static_cast<uint8_t>(msg_type);
    out_buf[6] = peer_id;
    out_buf[7] = 0x00; // Flags

    std::memcpy(&out_buf[8], g_room_hash, 32);

    // Sequence Number (Big-Endian)
    out_buf[40] = static_cast<uint8_t>((seq >> 24) & 0xFF);
    out_buf[41] = static_cast<uint8_t>((seq >> 16) & 0xFF);
    out_buf[42] = static_cast<uint8_t>((seq >> 8) & 0xFF);
    out_buf[43] = static_cast<uint8_t>(seq & 0xFF);

    // Payload Length (Big-Endian)
    out_buf[44] = static_cast<uint8_t>((payload_len >> 8) & 0xFF);
    out_buf[45] = static_cast<uint8_t>(payload_len & 0xFF);

    if (payload && payload_len > 0) {
        std::memcpy(&out_buf[HEADER_LEN], payload, payload_len);
    }
}

static void SendHandshakeInit(SOCKET s) {
    if (s == INVALID_SOCKET) return;

    std::vector<uint8_t> pkt;
    uint8_t role_pref = g_peer_id.load();
    BuildEnvelope(MessageType::HandshakeInit, role_pref, g_seq_num.fetch_add(1), nullptr, 0, pkt);

    sendto(s, reinterpret_cast<const char*>(pkt.data()), static_cast<int>(pkt.size()), 0,
           reinterpret_cast<const sockaddr*>(&g_relay_addr), sizeof(g_relay_addr));
}

// -----------------------------------------------------------------------------
// Heartbeat Worker Thread
// -----------------------------------------------------------------------------
static void HeartbeatWorker() {
    while (g_running.load()) {
        std::this_thread::sleep_for(std::chrono::milliseconds(1000));

        SOCKET s = g_game_socket.load();
        if (s != INVALID_SOCKET) {
            uint64_t now_ms = static_cast<uint64_t>(
                std::chrono::duration_cast<std::chrono::milliseconds>(
                    std::chrono::steady_clock::now().time_since_epoch()
                ).count()
            );

            std::vector<uint8_t> pkt;
            BuildEnvelope(MessageType::HeartbeatPing, g_peer_id.load(), g_seq_num.fetch_add(1),
                          &now_ms, sizeof(now_ms), pkt);

            sendto(s, reinterpret_cast<const char*>(pkt.data()), static_cast<int>(pkt.size()), 0,
                   reinterpret_cast<const sockaddr*>(&g_relay_addr), sizeof(g_relay_addr));
        }

        // Periodically refresh HUD
        SeamlessHudState hud = GetHudState();
        hud.rtt_ms = g_rtt_ms.load();
        hud.packets_sent = g_packets_sent.load();
        hud.packets_recv = g_packets_recv.load();
        hud.is_connected = g_is_connected.load();
        hud.is_peer_connected = g_is_peer_connected.load();
        hud.peer_id = g_peer_id.load();
        hud.role_text = (hud.peer_id == 0) ? "Host (Peer 0)" : "Guest (Peer 1)";
        UpdateHudState(hud);
    }
}

// -----------------------------------------------------------------------------
// Winsock Detours
// -----------------------------------------------------------------------------
using PFN_bind = int(WSAAPI*)(SOCKET s, const struct sockaddr *name, int namelen);
using PFN_sendto = int(WSAAPI*)(SOCKET s, const char *buf, int len, int flags, const struct sockaddr *to, int tolen);
using PFN_recvfrom = int(WSAAPI*)(SOCKET s, char *buf, int len, int flags, struct sockaddr *from, int *fromlen);

static PFN_bind g_orig_bind = nullptr;
static PFN_sendto g_orig_sendto = nullptr;
static PFN_recvfrom g_orig_recvfrom = nullptr;

static __declspec(noinline) int WSAAPI Hooked_bind(SOCKET s, const struct sockaddr *name, int namelen) {
    if (name && name->sa_family == AF_INET) {
        const auto* sin = reinterpret_cast<const sockaddr_in*>(name);
        uint16_t port = ntohs(sin->sin_port);

        // Detect DirtySDK CommUDP match socket (port 3659 or range 3658-3665)
        if (port == 3659 || (port >= 3658 && port <= 3665)) {
            g_game_socket.store(s);

            int bufsize = 65536;
            setsockopt(s, SOL_SOCKET, SO_RCVBUF, reinterpret_cast<const char*>(&bufsize), sizeof(bufsize));
            setsockopt(s, SOL_SOCKET, SO_SNDBUF, reinterpret_cast<const char*>(&bufsize), sizeof(bufsize));

            SeamlessHudState hud = GetHudState();
            hud.status_text = "Contacting Relay Server...";
            UpdateHudState(hud);

            // Announce presence to relay server
            SendHandshakeInit(s);
        }
    }
    return g_orig_bind(s, name, namelen);
}

static __declspec(noinline) int WSAAPI Hooked_sendto(
    SOCKET s,
    const char *buf,
    int len,
    int flags,
    const struct sockaddr *to,
    int tolen
) {
    if (!to || len <= 0) {
        return g_orig_sendto(s, buf, len, flags, to, tolen);
    }

    uint16_t dst_port = 0;
    if (to->sa_family == AF_INET) {
        dst_port = ntohs(reinterpret_cast<const sockaddr_in*>(to)->sin_port);
    }

    // Drop EA telemetry / tracking silently
    if (dst_port == 443 || dst_port == 80 || dst_port == 8080) {
        WSASetLastError(WSAECONNREFUSED);
        return SOCKET_ERROR;
    }

    // Check if this is game match UDP traffic
    if (s == g_game_socket.load() || dst_port == 3659 || (dst_port >= 3658 && dst_port <= 3665)) {
        std::vector<uint8_t> envelope;
        BuildEnvelope(MessageType::GameData, g_peer_id.load(), g_seq_num.fetch_add(1),
                      buf, static_cast<uint16_t>(len), envelope);

        int res = g_orig_sendto(
            s,
            reinterpret_cast<const char*>(envelope.data()),
            static_cast<int>(envelope.size()),
            flags,
            reinterpret_cast<const sockaddr*>(&g_relay_addr),
            sizeof(g_relay_addr)
        );

        if (res > 0) {
            g_packets_sent.fetch_add(1);
            return len; // Return original length to DirtySDK
        }
        return res;
    }

    return g_orig_sendto(s, buf, len, flags, to, tolen);
}

static __declspec(noinline) int WSAAPI Hooked_recvfrom(
    SOCKET s,
    char *buf,
    int len,
    int flags,
    struct sockaddr *from,
    int *fromlen
) {
    while (true) {
        int res = g_orig_recvfrom(s, buf, len, flags, from, fromlen);
        if (res <= 0) {
            return res;
        }

        // Verify if packet is from our relay server
        if (from && from->sa_family == AF_INET) {
            const auto* sin = reinterpret_cast<const sockaddr_in*>(from);
            if (sin->sin_addr.s_addr == g_relay_addr.sin_addr.s_addr &&
                sin->sin_port == g_relay_addr.sin_port) {

                if (res >= static_cast<int>(HEADER_LEN) &&
                    std::memcmp(buf, PROTOCOL_MAGIC, 4) == 0 &&
                    buf[4] == PROTOCOL_VERSION) {

                    auto msg_type = static_cast<MessageType>(buf[5]);
                    uint8_t peer_id = buf[6];
                    uint16_t payload_len = (static_cast<uint8_t>(buf[44]) << 8) | static_cast<uint8_t>(buf[45]);

                    if (msg_type == MessageType::HandshakeAck) {
                        g_peer_id.store(peer_id);
                        g_is_connected.store(true);

                        uint8_t status_byte = (payload_len > 0) ? static_cast<uint8_t>(buf[HEADER_LEN]) : 0x01;
                        g_is_peer_connected.store(status_byte == 0x02);

                        SeamlessHudState hud = GetHudState();
                        hud.is_connected = true;
                        hud.peer_id = peer_id;
                        hud.role_text = (peer_id == 0) ? "Host (Peer 0)" : "Guest (Peer 1)";
                        hud.status_text = (status_byte == 0x02) ? "Ready - Peer Connected" : "Waiting for Peer...";
                        hud.is_peer_connected = (status_byte == 0x02);
                        UpdateHudState(hud);

                        // Discard control packet from game loop, continue polling
                        continue;
                    }

                    if (msg_type == MessageType::PeerStatusNotify) {
                        uint8_t status_byte = (payload_len > 0) ? static_cast<uint8_t>(buf[HEADER_LEN]) : 0x00;
                        g_is_peer_connected.store(status_byte == 0x02);

                        SeamlessHudState hud = GetHudState();
                        hud.is_peer_connected = (status_byte == 0x02);
                        hud.status_text = (status_byte == 0x02) ? "Ready - Peer Connected" : "Peer Disconnected";
                        UpdateHudState(hud);
                        continue;
                    }

                    if (msg_type == MessageType::HeartbeatPong) {
                        if (payload_len >= sizeof(uint64_t)) {
                            uint64_t sent_ms = 0;
                            std::memcpy(&sent_ms, &buf[HEADER_LEN], sizeof(uint64_t));
                            uint64_t now_ms = static_cast<uint64_t>(
                                std::chrono::duration_cast<std::chrono::milliseconds>(
                                    std::chrono::steady_clock::now().time_since_epoch()
                                ).count()
                            );
                            if (now_ms >= sent_ms) {
                                g_rtt_ms.store(static_cast<uint32_t>(now_ms - sent_ms));
                            }
                        }
                        continue;
                    }

                    if (msg_type == MessageType::GameData) {
                        // Decapsulate: strip the 46-byte envelope and return raw DirtySDK frame
                        if (payload_len > 0 && res >= static_cast<int>(HEADER_LEN + payload_len)) {
                            std::memmove(buf, buf + HEADER_LEN, payload_len);

                            // Synthesize virtual opponent socket address (127.0.0.1:3659)
                            if (from && fromlen && *fromlen >= static_cast<int>(sizeof(sockaddr_in))) {
                                auto* out_sin = reinterpret_cast<sockaddr_in*>(from);
                                out_sin->sin_family = AF_INET;
                                out_sin->sin_port = htons(3659);
                                inet_pton(AF_INET, "127.0.0.1", &out_sin->sin_addr);
                                *fromlen = sizeof(sockaddr_in);
                            }

                            g_packets_recv.fetch_add(1);
                            return static_cast<int>(payload_len);
                        }
                    }
                }
            }
        }

        return res;
    }
}

// -----------------------------------------------------------------------------
// DLL Lifetime & Hook Installation
// -----------------------------------------------------------------------------
static void InitializeSeamlessShim() {
    LoadConfig();

    if (MH_Initialize() == MH_OK) {
        HMODULE hWs2 = GetModuleHandleW(L"ws2_32.dll");
        if (hWs2) {
            void* pBind = reinterpret_cast<void*>(GetProcAddress(hWs2, "bind"));
            void* pSendto = reinterpret_cast<void*>(GetProcAddress(hWs2, "sendto"));
            void* pRecvfrom = reinterpret_cast<void*>(GetProcAddress(hWs2, "recvfrom"));

            if (pBind) MH_CreateHook(pBind, reinterpret_cast<LPVOID>(&Hooked_bind), reinterpret_cast<LPVOID*>(&g_orig_bind));
            if (pSendto) MH_CreateHook(pSendto, reinterpret_cast<LPVOID>(&Hooked_sendto), reinterpret_cast<LPVOID*>(&g_orig_sendto));
            if (pRecvfrom) MH_CreateHook(pRecvfrom, reinterpret_cast<LPVOID>(&Hooked_recvfrom), reinterpret_cast<LPVOID*>(&g_orig_recvfrom));

            MH_EnableHook(MH_ALL_HOOKS);
        }

        if (g_config.enable_overlay) {
            InstallDxgiOverlayHooks();
        }

        g_heartbeat_thread = std::thread(HeartbeatWorker);
    }
}

static void ShutdownSeamlessShim() {
    g_running.store(false);
    if (g_heartbeat_thread.joinable()) {
        g_heartbeat_thread.join();
    }
    RemoveDxgiOverlayHooks();
    MH_Uninitialize();
}

BOOL WINAPI DllMain(HINSTANCE hinstDLL, DWORD fdwReason, LPVOID lpvReserved) {
    if (fdwReason == DLL_PROCESS_ATTACH) {
        DisableThreadLibraryCalls(hinstDLL);
        InitializeSeamlessShim();
    } else if (fdwReason == DLL_PROCESS_DETACH) {
        ShutdownSeamlessShim();
    }
    return TRUE;
}
