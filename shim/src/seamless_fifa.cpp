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
    bool enable_overlay = false; // Temporarily disabled for isolation test
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

static std::ofstream g_shim_log;

static void LogShim(const std::string& msg) {
    OutputDebugStringA(("[SF22] " + msg + "\n").c_str());
    if (g_shim_log.is_open()) {
        auto now = std::chrono::system_clock::now();
        auto time_t_now = std::chrono::system_clock::to_time_t(now);
        std::tm tm_buf{};
        localtime_s(&tm_buf, &time_t_now);

        char time_str[64];
        std::snprintf(time_str, sizeof(time_str), "[%02d:%02d:%02d] ",
                      tm_buf.tm_hour, tm_buf.tm_min, tm_buf.tm_sec);
        g_shim_log << time_str << msg << "\n";
        g_shim_log.flush();
    }
}

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

    LogShim("Config loaded: relay=" + g_config.relay_host + ":" + std::to_string(g_config.relay_port) +
            ", room=" + g_config.session_password + ", role=" + g_config.role);
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
    LogShim("Sent HANDSHAKE_INIT to relay server (" + g_config.relay_host + ":" + std::to_string(g_config.relay_port) + ")");
}

// -----------------------------------------------------------------------------
// Heartbeat Worker Thread
// -----------------------------------------------------------------------------
static DWORD WINAPI HeartbeatWorker(LPVOID) {
    LogShim("Heartbeat worker thread started.");
    while (g_running.load()) {
        Sleep(1000);

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
    LogShim("Heartbeat worker thread exiting.");
    return 0;
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
            LogShim("[WinSock] Detected DirtySDK CommUDP bind on port " + std::to_string(port) + " (Socket: " + std::to_string(s) + ")");

            int bufsize = 65536;
            setsockopt(s, SOL_SOCKET, SO_RCVBUF, reinterpret_cast<const char*>(&bufsize), sizeof(bufsize));
            setsockopt(s, SOL_SOCKET, SO_SNDBUF, reinterpret_cast<const char*>(&bufsize), sizeof(bufsize));

            SeamlessHudState hud = GetHudState();
            hud.status_text = "Contacting Relay Server...";
            UpdateHudState(hud);

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

    // Intercept and wrap match gameplay packets
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
            return len; // Return original payload length to DirtySDK
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
                        LogShim("[Relay] Received HandshakeAck: Assigned Peer ID " + std::to_string(peer_id) +
                                (status_byte == 0x02 ? " (Both Peers Ready)" : " (Waiting for Peer)"));

                        SeamlessHudState hud = GetHudState();
                        hud.is_connected = true;
                        hud.peer_id = peer_id;
                        hud.role_text = (peer_id == 0) ? "Host (Peer 0)" : "Guest (Peer 1)";
                        hud.status_text = (status_byte == 0x02) ? "Ready - Peer Connected" : "Waiting for Peer...";
                        hud.is_peer_connected = (status_byte == 0x02);
                        UpdateHudState(hud);

                        continue;
                    }

                    if (msg_type == MessageType::PeerStatusNotify) {
                        uint8_t status_byte = (payload_len > 0) ? static_cast<uint8_t>(buf[HEADER_LEN]) : 0x00;
                        g_is_peer_connected.store(status_byte == 0x02);
                        LogShim("[Relay] PeerStatusNotify: " + std::string(status_byte == 0x02 ? "Connected" : "Disconnected"));

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
                        if (payload_len > 0 && res >= static_cast<int>(HEADER_LEN + payload_len)) {
                            std::memmove(buf, buf + HEADER_LEN, payload_len);

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

static LONG WINAPI CustomCrashHandler(EXCEPTION_POINTERS* pExceptionInfo) {
    if (pExceptionInfo && pExceptionInfo->ExceptionRecord) {
        char buf[512];
        std::snprintf(buf, sizeof(buf),
                      "[FATAL CRASH] ExceptionCode=0x%08lX Address=0x%p Flags=0x%lX",
                      pExceptionInfo->ExceptionRecord->ExceptionCode,
                      pExceptionInfo->ExceptionRecord->ExceptionAddress,
                      pExceptionInfo->ExceptionRecord->ExceptionFlags);
        OutputDebugStringA(buf);
        std::ofstream f("crash_dump.txt", std::ios::out | std::ios::trunc);
        f << buf << "\n";
        f.flush();
    }
    return EXCEPTION_CONTINUE_SEARCH;
}

// -----------------------------------------------------------------------------
// Offline & Server Status Bypass Memory Patches & Continuous Enforcer
// -----------------------------------------------------------------------------
static bool SafeMemoryPatch(uintptr_t address, const uint8_t* patch_bytes, size_t patch_len) {
    DWORD oldProtect = 0;
    if (VirtualProtect(reinterpret_cast<LPVOID>(address), patch_len, PAGE_EXECUTE_READWRITE, &oldProtect)) {
        std::memcpy(reinterpret_cast<void*>(address), patch_bytes, patch_len);
        VirtualProtect(reinterpret_cast<LPVOID>(address), patch_len, oldProtect, &oldProtect);
        FlushInstructionCache(GetCurrentProcess(), reinterpret_cast<LPCVOID>(address), patch_len);
        return true;
    }
    return false;
}

static std::string BytesToHex(const uint8_t* ptr, size_t len) {
    char buf[64];
    std::string s;
    for (size_t i = 0; i < len; ++i) {
        std::snprintf(buf, sizeof(buf), "%02X ", ptr[i]);
        s += buf;
    }
    if (!s.empty()) s.pop_back();
    return s;
}

static void DumpPatchedMemory(const char* context) {
    HMODULE hExe = GetModuleHandleW(NULL);
    if (!hExe) return;
    const uint8_t* base = reinterpret_cast<const uint8_t*>(hExe);

    const uint8_t* b4D89F4C = base + 0x4D89F4C;
    const uint8_t* b27E2C72 = base + 0x27E2C72;
    const uint8_t* b27E2C79 = base + 0x27E2C79;
    const uint8_t* b27E2CC3 = base + 0x27E2CC3;
    const uint8_t* b27E2B90 = base + 0x27E2B90;

    std::string dump = "[MemoryDump: " + std::string(context) + "] ";
    dump += "0x144D89F4C=[" + BytesToHex(b4D89F4C, 3) + "] (expected 90 90 90) ";
    dump += "0x1427E2C72=[" + BytesToHex(b27E2C72, 5) + "] (expected B8 00 00 00 00) ";
    dump += "0x1427E2C79=[" + BytesToHex(b27E2C79, 5) + "] ";
    dump += "0x1427E2CC3=[" + BytesToHex(b27E2CC3, 2) + "] ";
    dump += "0x1427E2B90=[" + BytesToHex(b27E2B90, 6) + "]";

    HMODULE hCards = GetModuleHandleW(L"CardsDLL_Win64_retail.dll");
    if (hCards) {
        const uint8_t* cbase = reinterpret_cast<const uint8_t*>(hCards);
        dump += " | CardsDLL at 0x" + std::to_string((uintptr_t)hCards);
        dump += " 0x4213B=[" + BytesToHex(cbase + 0x4213B, 2) + "] (expected 90 90)";
        dump += " 0x42154=[" + BytesToHex(cbase + 0x42154, 3) + "] (expected EB E7 90)";
        dump += " 0x421C6=[" + BytesToHex(cbase + 0x421C6, 3) + "] (expected 90 90 90)";
    } else {
        dump += " | CardsDLL: NOT LOADED";
    }

    LogShim(dump);
}

static bool ApplyFifaExePatches() {
    HMODULE hExe = GetModuleHandleW(NULL);
    if (!hExe) return false;
    const uint8_t* base = reinterpret_cast<const uint8_t*>(hExe);

    uint8_t mov_zero[5] = {0xB8, 0x00, 0x00, 0x00, 0x00};
    uint8_t xor_eax[2] = {0x31, 0xC0};
    uint8_t nop6[6] = {0x90, 0x90, 0x90, 0x90, 0x90, 0x90};
    uint8_t nop3[3] = {0x90, 0x90, 0x90};

    bool ok = true;
    ok &= SafeMemoryPatch(reinterpret_cast<uintptr_t>(base + 0x27E2C72), mov_zero, 5);
    ok &= SafeMemoryPatch(reinterpret_cast<uintptr_t>(base + 0x27E2C79), mov_zero, 5);
    ok &= SafeMemoryPatch(reinterpret_cast<uintptr_t>(base + 0x27E2CC3), xor_eax, 2);
    ok &= SafeMemoryPatch(reinterpret_cast<uintptr_t>(base + 0x27E2B90), nop6, 6);
    ok &= SafeMemoryPatch(reinterpret_cast<uintptr_t>(base + 0x4D89F4C), nop3, 3);
    return ok;
}

static bool ApplyCardsDllPatches() {
    HMODULE hCards = GetModuleHandleW(L"CardsDLL_Win64_retail.dll");
    if (!hCards) return false;
    const uint8_t* base = reinterpret_cast<const uint8_t*>(hCards);

    uint8_t nop2[2] = {0x90, 0x90};
    uint8_t jmp_online[3] = {0xEB, 0xE7, 0x90}; // jmp 0x4213D (status 2 = ONLINE); nop
    uint8_t nop3[3] = {0x90, 0x90, 0x90};

    bool ok = true;
    ok &= SafeMemoryPatch(reinterpret_cast<uintptr_t>(base + 0x4213B), nop2, 2);
    ok &= SafeMemoryPatch(reinterpret_cast<uintptr_t>(base + 0x42154), jmp_online, 3);
    ok &= SafeMemoryPatch(reinterpret_cast<uintptr_t>(base + 0x421C6), nop3, 3);
    return ok;
}

static DWORD WINAPI PatchWorkerThread(LPVOID) {
    LogShim("[PatchWorker] Background patcher thread started. Applying initial FIFA22.exe patches...");
    ApplyFifaExePatches();
    DumpPatchedMemory("InitialPostPatch");

    bool cards_patched = false;
    uint32_t iteration = 0;

    while (g_running.load(std::memory_order_relaxed)) {
        Sleep(100);
        iteration++;

        HMODULE hExe = GetModuleHandleW(NULL);
        if (hExe) {
            const uint8_t* base = reinterpret_cast<const uint8_t*>(hExe);
            const uint8_t* b4D89F4C = base + 0x4D89F4C;
            const uint8_t* b27E2C72 = base + 0x27E2C72;

            // Check if patched bytes reverted
            if (b4D89F4C[0] != 0x90 || b4D89F4C[1] != 0x90 || b4D89F4C[2] != 0x90 ||
                b27E2C72[0] != 0xB8 || b27E2C72[1] != 0x00) {
                LogShim("[PatchWorker ALERT] Detected byte revert in FIFA22.exe! Re-applying patches...");
                ApplyFifaExePatches();
                DumpPatchedMemory("RevertedReapplied");
            }
        }

        HMODULE hCards = GetModuleHandleW(L"CardsDLL_Win64_retail.dll");
        if (hCards) {
            const uint8_t* cbase = reinterpret_cast<const uint8_t*>(hCards);
            const uint8_t* c4213B = cbase + 0x4213B;
            const uint8_t* c42154 = cbase + 0x42154;

            if (!cards_patched) {
                LogShim("[PatchWorker] CardsDLL_Win64_retail.dll detected! Applying patches...");
                ApplyCardsDllPatches();
                cards_patched = true;
                DumpPatchedMemory("CardsLoadedPostPatch");
            } else if (c4213B[0] != 0x90 || c42154[0] != 0xEB) {
                LogShim("[PatchWorker ALERT] Detected byte revert in CardsDLL! Re-applying patches...");
                ApplyCardsDllPatches();
                DumpPatchedMemory("CardsRevertedReapplied");
            }
        }

        // Dump memory periodically every 10 seconds (100 * 100ms) to track state
        if (iteration % 100 == 0) {
            DumpPatchedMemory("HeartbeatDump");
        }
    }

    return 0;
}

// -----------------------------------------------------------------------------
// Safe Asynchronous Initialization Thread (Avoids Loader Lock Deadlock)
// -----------------------------------------------------------------------------
static DWORD WINAPI SeamlessInitThread(LPVOID lpParam) {
    SetUnhandledExceptionFilter(CustomCrashHandler);

    g_shim_log.open("seamless_fifa.log", std::ios::out | std::ios::trunc);
    LogShim("============================================================");
    LogShim("   Seamless FIFA 22 Initialization Thread Active            ");
    LogShim("============================================================");

    LogShim("[InitThread] Waiting 500ms for CRT and socket libraries...");
    Sleep(500);
    LogShim("[InitThread] Proceeding with configuration and hooks...");

    LoadConfig();

    LogShim("Initializing MinHook...");
    MH_STATUS mh_init = MH_Initialize();
    LogShim("MH_Initialize returned status: " + std::to_string(mh_init));

    if (mh_init == MH_OK) {
        HMODULE hWs2 = GetModuleHandleW(L"ws2_32.dll");
        if (!hWs2) {
            hWs2 = LoadLibraryW(L"ws2_32.dll");
        }

        if (hWs2) {
            void* pBind = reinterpret_cast<void*>(GetProcAddress(hWs2, "bind"));
            void* pSendto = reinterpret_cast<void*>(GetProcAddress(hWs2, "sendto"));
            void* pRecvfrom = reinterpret_cast<void*>(GetProcAddress(hWs2, "recvfrom"));

            LogShim("Target addresses: bind=0x" + std::to_string((uintptr_t)pBind) +
                    ", sendto=0x" + std::to_string((uintptr_t)pSendto) +
                    ", recvfrom=0x" + std::to_string((uintptr_t)pRecvfrom));

            if (pBind) {
                MH_STATUS s = MH_CreateHook(pBind, reinterpret_cast<LPVOID>(&Hooked_bind), reinterpret_cast<LPVOID*>(&g_orig_bind));
                LogShim("Hook bind status: " + std::to_string(s));
            }
            if (pSendto) {
                MH_STATUS s = MH_CreateHook(pSendto, reinterpret_cast<LPVOID>(&Hooked_sendto), reinterpret_cast<LPVOID*>(&g_orig_sendto));
                LogShim("Hook sendto status: " + std::to_string(s));
            }
            if (pRecvfrom) {
                MH_STATUS s = MH_CreateHook(pRecvfrom, reinterpret_cast<LPVOID>(&Hooked_recvfrom), reinterpret_cast<LPVOID*>(&g_orig_recvfrom));
                LogShim("Hook recvfrom status: " + std::to_string(s));
            }

            MH_STATUS s_all = MH_EnableHook(MH_ALL_HOOKS);
            LogShim("MH_EnableHook(MH_ALL_HOOKS) status: " + std::to_string(s_all));
        } else {
            LogShim("ERROR: Failed to obtain ws2_32.dll handle!");
        }

        // Overlay temporarily disabled to verify engine launch stability first
        if (g_config.enable_overlay) {
            LogShim("Installing DXGI Overlay Hooks...");
            InstallDxgiOverlayHooks();
        }

        // Spawn background heartbeat thread
        HANDLE hHeartbeat = CreateThread(nullptr, 0, HeartbeatWorker, nullptr, 0, nullptr);
        if (hHeartbeat) CloseHandle(hHeartbeat);

        // Spawn background memory patcher thread for server status & popup bypass
        HANDLE hPatcher = CreateThread(nullptr, 0, PatchWorkerThread, nullptr, 0, nullptr);
        if (hPatcher) CloseHandle(hPatcher);
    }

    LogShim("Initialization thread finished successfully.");
    return 0;
}

// -----------------------------------------------------------------------------
// DllMain (Strict Minimal Loader Lock Compliance)
// -----------------------------------------------------------------------------
BOOL WINAPI DllMain(HINSTANCE hinstDLL, DWORD fdwReason, LPVOID lpvReserved) {
    if (fdwReason == DLL_PROCESS_ATTACH) {
        SetUnhandledExceptionFilter(CustomCrashHandler);
        DisableThreadLibraryCalls(hinstDLL);
        OutputDebugStringW(L"[SF22] DllMain DLL_PROCESS_ATTACH invoked.\n");

        // Spawn independent worker thread to initialize hooks outside loader lock
        HANDLE hInit = CreateThread(nullptr, 0, SeamlessInitThread, hinstDLL, 0, nullptr);
        if (hInit) {
            CloseHandle(hInit);
        }
    } else if (fdwReason == DLL_PROCESS_DETACH) {
        g_running.store(false);
        OutputDebugStringW(L"[SF22] DllMain DLL_PROCESS_DETACH invoked.\n");
    }
    return TRUE;
}
