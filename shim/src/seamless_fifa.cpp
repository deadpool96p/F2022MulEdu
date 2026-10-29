#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <winsock2.h>
#include <ws2tcpip.h>
#include <mswsock.h>
#include <wincrypt.h>
#include <bcrypt.h>
#include <psapi.h>
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

#if defined(__GNUC__) || defined(__clang__)
#ifndef _ReturnAddress
#define _ReturnAddress() __builtin_return_address(0)
#endif
#else
#include <intrin.h>
#endif

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

using PFN_sendto = int(WSAAPI*)(SOCKET s, const char *buf, int len, int flags, const struct sockaddr *to, int tolen);
static PFN_sendto g_orig_sendto = nullptr;

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

    int res = g_orig_sendto ? g_orig_sendto(s, reinterpret_cast<const char*>(pkt.data()), static_cast<int>(pkt.size()), 0,
                                            reinterpret_cast<const sockaddr*>(&g_relay_addr), sizeof(g_relay_addr))
                            : sendto(s, reinterpret_cast<const char*>(pkt.data()), static_cast<int>(pkt.size()), 0,
                                     reinterpret_cast<const sockaddr*>(&g_relay_addr), sizeof(g_relay_addr));
    int err = (res == SOCKET_ERROR) ? WSAGetLastError() : 0;
    LogShim("Sent HANDSHAKE_INIT to relay server (" + g_config.relay_host + ":" + std::to_string(g_config.relay_port) + 
            "), res=" + std::to_string(res) + (err ? " err=" + std::to_string(err) : ""));
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

            if (g_orig_sendto) {
                g_orig_sendto(s, reinterpret_cast<const char*>(pkt.data()), static_cast<int>(pkt.size()), 0,
                              reinterpret_cast<const sockaddr*>(&g_relay_addr), sizeof(g_relay_addr));
            } else {
                sendto(s, reinterpret_cast<const char*>(pkt.data()), static_cast<int>(pkt.size()), 0,
                       reinterpret_cast<const sockaddr*>(&g_relay_addr), sizeof(g_relay_addr));
            }
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
// Winsock Detours (UDP Match Relay, TCP Blaze Auth & Anadius LSX Interceptor)
// -----------------------------------------------------------------------------
using PFN_bind = int(WSAAPI*)(SOCKET s, const struct sockaddr *name, int namelen);
using PFN_recvfrom = int(WSAAPI*)(SOCKET s, char *buf, int len, int flags, struct sockaddr *from, int *fromlen);
using PFN_connect = int(WSAAPI*)(SOCKET s, const struct sockaddr *name, int namelen);
using PFN_WSAConnect = int(WSAAPI*)(SOCKET s, const struct sockaddr *name, int namelen, LPWSABUF lpCallerData, LPWSABUF lpCalleeData, LPQOS lpSQOS, LPQOS lpGQOS);
using PFN_WSAIoctl = int(WSAAPI*)(SOCKET s, DWORD dwIoControlCode, LPVOID lpvInBuffer, DWORD cbInBuffer, LPVOID lpvOutBuffer, DWORD cbOutBuffer, LPDWORD lpcbBytesReturned, LPWSAOVERLAPPED lpOverlapped, LPWSAOVERLAPPED_COMPLETION_ROUTINE lpCompletionRoutine);
using LPFN_CONNECTEX = BOOL(PASCAL FAR *)(SOCKET s, const struct sockaddr *name, int namelen, PVOID lpSendBuffer, DWORD dwSendDataLength, LPDWORD lpdwBytesSent, LPOVERLAPPED lpOverlapped);
using PFN_getaddrinfo = int(WSAAPI*)(const char *pNodeName, const char *pServiceName, const ADDRINFOA *pHints, PADDRINFOA *ppResult);
using PFN_GetAddrInfoW = int(WSAAPI*)(const PCWSTR pNodeName, const PCWSTR pServiceName, const ADDRINFOW *pHints, PADDRINFOW *ppResult);
using PFN_send = int(WSAAPI*)(SOCKET s, const char *buf, int len, int flags);
using PFN_recv = int(WSAAPI*)(SOCKET s, char *buf, int len, int flags);

static PFN_bind g_orig_bind = nullptr;
static PFN_recvfrom g_orig_recvfrom = nullptr;
static PFN_connect g_orig_connect = nullptr;
static PFN_WSAConnect g_orig_WSAConnect = nullptr;
static PFN_WSAIoctl g_orig_WSAIoctl = nullptr;
static LPFN_CONNECTEX g_real_ConnectEx = nullptr;
static PFN_getaddrinfo g_orig_getaddrinfo = nullptr;
static PFN_GetAddrInfoW g_orig_GetAddrInfoW = nullptr;
static PFN_send g_orig_send = nullptr;
static PFN_recv g_orig_recv = nullptr;

static std::atomic<SOCKET> g_lsx_socket{INVALID_SOCKET};

static std::string GetCallerModuleInfo(void* caller) {
    HMODULE hMod = nullptr;
    if (GetModuleHandleExA(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                           reinterpret_cast<LPCSTR>(caller), &hMod) && hMod) {
        char modName[MAX_PATH] = {0};
        GetModuleFileNameA(hMod, modName, sizeof(modName));
        const char* baseName = strrchr(modName, '\\');
        baseName = baseName ? (baseName + 1) : modName;
        uintptr_t rva = reinterpret_cast<uintptr_t>(caller) - reinterpret_cast<uintptr_t>(hMod);
        char buf[128];
        snprintf(buf, sizeof(buf), "[%s+0x%IX]", baseName, rva);
        return std::string(buf);
    }
    char buf[64];
    snprintf(buf, sizeof(buf), "[0x%p]", caller);
    return std::string(buf);
}

static bool ShouldRerouteHost(std::string_view host) {
    if (host.empty()) return false;
    if (host.find("gosredirector") != std::string_view::npos ||
        host.find("blaze") != std::string_view::npos ||
        host.find("river.data.ea.com") != std::string_view::npos ||
        host.find("pin-river.data.ea.com") != std::string_view::npos ||
        host.find("ea.com") != std::string_view::npos ||
        host.find("ea.net") != std::string_view::npos ||
        host.find("dice.se") != std::string_view::npos ||
        host.find("origin.com") != std::string_view::npos) {
        return true;
    }
    return false;
}

static int WSAAPI Hooked_getaddrinfo(
    const char *pNodeName,
    const char *pServiceName,
    const ADDRINFOA *pHints,
    PADDRINFOA *ppResult
) {
    const char *query_host = pNodeName ? pNodeName : "";
    void* caller = _ReturnAddress();
    std::string caller_str = GetCallerModuleInfo(caller);

    if (ShouldRerouteHost(query_host)) {
        LogShim("[WinSock] Intercepted getaddrinfo(\"" + std::string(query_host) + "\") by " + caller_str + " -> rerouting to 127.0.0.1");

        ADDRINFOA sanitized_hints{};
        const ADDRINFOA* effective_hints = pHints;
        if (pHints) {
            sanitized_hints = *pHints;
            sanitized_hints.ai_family = AF_INET;
            sanitized_hints.ai_flags |= AI_NUMERICHOST;
            if (pServiceName && std::isdigit(static_cast<unsigned char>(pServiceName[0]))) {
                sanitized_hints.ai_flags |= AI_NUMERICSERV;
            }
            effective_hints = &sanitized_hints;
        } else {
            sanitized_hints.ai_family = AF_INET;
            sanitized_hints.ai_socktype = SOCK_STREAM;
            sanitized_hints.ai_flags = AI_NUMERICHOST;
            effective_hints = &sanitized_hints;
        }

        int res = g_orig_getaddrinfo("127.0.0.1", pServiceName, effective_hints, ppResult);
        if (res != 0) {
            ADDRINFOA fallback_hints{};
            fallback_hints.ai_family = AF_INET;
            fallback_hints.ai_socktype = (pHints ? pHints->ai_socktype : SOCK_STREAM);
            fallback_hints.ai_flags = AI_NUMERICHOST;
            res = g_orig_getaddrinfo("127.0.0.1", nullptr, &fallback_hints, ppResult);
        }
        return res;
    } else if (pNodeName && pNodeName[0] != '\0') {
        LogShim("[WinSock pass-through getaddrinfo] \"" + std::string(query_host) + "\" by " + caller_str);
    }

    return g_orig_getaddrinfo(pNodeName, pServiceName, pHints, ppResult);
}

static int WSAAPI Hooked_GetAddrInfoW(
    const PCWSTR pNodeName,
    const PCWSTR pServiceName,
    const ADDRINFOW *pHints,
    PADDRINFOW *ppResult
) {
    void* caller = _ReturnAddress();
    std::string caller_str = GetCallerModuleInfo(caller);

    char narrow_buf[256] = {0};
    if (pNodeName && pNodeName[0] != L'\0') {
        WideCharToMultiByte(CP_UTF8, 0, pNodeName, -1, narrow_buf, sizeof(narrow_buf), nullptr, nullptr);
    }

    if (ShouldRerouteHost(narrow_buf)) {
        LogShim("[WinSock] Intercepted GetAddrInfoW(\"" + std::string(narrow_buf) + "\") by " + caller_str + " -> rerouting to 127.0.0.1");

        ADDRINFOW sanitized_hints{};
        const ADDRINFOW* effective_hints = pHints;
        if (pHints) {
            sanitized_hints = *pHints;
            sanitized_hints.ai_family = AF_INET;
            sanitized_hints.ai_flags |= AI_NUMERICHOST;
            if (pServiceName && std::iswdigit(pServiceName[0])) {
                sanitized_hints.ai_flags |= AI_NUMERICSERV;
            }
            effective_hints = &sanitized_hints;
        } else {
            sanitized_hints.ai_family = AF_INET;
            sanitized_hints.ai_socktype = SOCK_STREAM;
            sanitized_hints.ai_flags = AI_NUMERICHOST;
            effective_hints = &sanitized_hints;
        }

        int res = g_orig_GetAddrInfoW(L"127.0.0.1", pServiceName, effective_hints, ppResult);
        if (res != 0) {
            ADDRINFOW fallback_hints{};
            fallback_hints.ai_family = AF_INET;
            fallback_hints.ai_socktype = (pHints ? pHints->ai_socktype : SOCK_STREAM);
            fallback_hints.ai_flags = AI_NUMERICHOST;
            res = g_orig_GetAddrInfoW(L"127.0.0.1", nullptr, &fallback_hints, ppResult);
        }
        return res;
    } else if (pNodeName && pNodeName[0] != L'\0') {
        LogShim("[WinSock pass-through GetAddrInfoW] \"" + std::string(narrow_buf) + "\" by " + caller_str);
    }

    return g_orig_GetAddrInfoW(pNodeName, pServiceName, pHints, ppResult);
}

static bool CheckAndRewriteSockAddr(const struct sockaddr *name, struct sockaddr_storage *out_addr, int *out_len, void* caller) {
    if (!name || !out_addr || !out_len) {
        return false;
    }
    std::string caller_str = GetCallerModuleInfo(caller);

    if (name->sa_family == AF_INET) {
        const auto *sin = reinterpret_cast<const struct sockaddr_in*>(name);
        uint16_t port = ntohs(sin->sin_port);
        char ip[INET_ADDRSTRLEN] = {0};
        inet_ntop(AF_INET, &(sin->sin_addr), ip, sizeof(ip));
        std::string ip_str(ip);

        // Target ports for EA Blaze, Redirector & HTTPS/QoS
        bool is_local_lan = (ip_str.rfind("26.", 0) == 0 ||
                             ip_str.rfind("192.168.", 0) == 0 ||
                             ip_str.rfind("10.", 0) == 0 ||
                             ip_str.rfind("172.", 0) == 0);
        bool is_blaze_port = (port == 42127 || port == 42128 || port == 42230 || port == 44325 || port == 10041);
        bool is_http_port = (port == 443 || port == 80 || port == 8080);
        bool should_reroute = is_blaze_port || (is_http_port && (ip_str == "127.0.0.1" || !is_local_lan));

        if (should_reroute) {
            auto *out_sin = reinterpret_cast<struct sockaddr_in*>(out_addr);
            std::memset(out_addr, 0, sizeof(struct sockaddr_storage));
            std::memcpy(out_sin, sin, sizeof(struct sockaddr_in));
            out_sin->sin_family = AF_INET;
            inet_pton(AF_INET, "127.0.0.1", &(out_sin->sin_addr));

            if (port == 42127) {
                out_sin->sin_port = htons(42127);
            } else if (port == 443 || port == 80 || port == 8080) {
                out_sin->sin_port = htons(443);
            } else if (port == 42128 || port == 42230 || port == 44325 || port == 10041) {
                out_sin->sin_port = htons(42128);
            } else {
                out_sin->sin_port = htons(port);
            }
            *out_len = sizeof(struct sockaddr_in);

            LogShim("[Winsock TCP REDIRECT] Rewrote destination " + ip_str + ":" + std::to_string(port) +
                    " -> 127.0.0.1:" + std::to_string(ntohs(out_sin->sin_port)) + " by " + caller_str);
            return true;
        }
    }

    if (name->sa_family == AF_INET6) {
        const auto *sin6 = reinterpret_cast<const struct sockaddr_in6*>(name);
        uint16_t port = ntohs(sin6->sin6_port);
        char ip6[INET6_ADDRSTRLEN] = {0};
        inet_ntop(AF_INET6, &(sin6->sin6_addr), ip6, sizeof(ip6));
        std::string ip6_str(ip6);

        bool is_local_lan6 = (ip6_str.find("26.") != std::string::npos ||
                              ip6_str.find("192.168.") != std::string::npos ||
                              ip6_str.find("10.") != std::string::npos ||
                              ip6_str.find("172.") != std::string::npos);
        bool is_blaze_port = (port == 42127 || port == 42128 || port == 42230 || port == 44325 || port == 10041);
        bool is_http_port = (port == 443 || port == 80 || port == 8080);
        bool should_reroute = is_blaze_port || (is_http_port && (ip6_str.find("127.0.0.1") != std::string::npos || !is_local_lan6));

        if (should_reroute) {
            auto *out_sin6 = reinterpret_cast<struct sockaddr_in6*>(out_addr);
            std::memset(out_addr, 0, sizeof(struct sockaddr_storage));
            out_sin6->sin6_family = AF_INET6;
            if (port == 42127) {
                out_sin6->sin6_port = htons(42127);
            } else if (port == 443 || port == 80 || port == 8080) {
                out_sin6->sin6_port = htons(443);
            } else if (port == 42128 || port == 42230 || port == 44325 || port == 10041) {
                out_sin6->sin6_port = htons(42128);
            } else {
                out_sin6->sin6_port = htons(port);
            }
            inet_pton(AF_INET6, "::ffff:127.0.0.1", &(out_sin6->sin6_addr));
            *out_len = sizeof(struct sockaddr_in6);

            LogShim("[Winsock TCP REDIRECT IPv6] Rewrote destination [" + ip6_str + "]:" + std::to_string(port) +
                    " -> [::ffff:127.0.0.1]:" + std::to_string(ntohs(out_sin6->sin6_port)) + " by " + caller_str);
            return true;
        }
    }

    return false;
}

static __declspec(noinline) int WSAAPI Hooked_connect(SOCKET s, const struct sockaddr *name, int namelen) {
    void* caller = _ReturnAddress();
    std::string caller_str = GetCallerModuleInfo(caller);

    if (name && name->sa_family == AF_INET) {
        const auto* sin = reinterpret_cast<const sockaddr_in*>(name);
        uint16_t port = ntohs(sin->sin_port);
        char ip[INET_ADDRSTRLEN] = {0};
        inet_ntop(AF_INET, &sin->sin_addr, ip, sizeof(ip));
        std::string ip_str(ip);

        // Intercept Anadius LSX socket (port 3216)
        if (port == 3216 || (port >= 3210 && port <= 3220)) {
            g_lsx_socket.store(s);
            LogShim("[Winsock TCP] Intercepted connect() to Anadius LSX on " + ip_str + ":" + std::to_string(port) + " (Socket " + std::to_string(s) + ") by " + caller_str);
        } else {
            LogShim("[Winsock TCP connect] Target " + ip_str + ":" + std::to_string(port) + " (Socket " + std::to_string(s) + ") by " + caller_str);
        }
    } else if (name && name->sa_family == AF_INET6) {
        const auto* sin6 = reinterpret_cast<const sockaddr_in6*>(name);
        uint16_t port = ntohs(sin6->sin6_port);
        char ip6[INET6_ADDRSTRLEN] = {0};
        inet_ntop(AF_INET6, &sin6->sin6_addr, ip6, sizeof(ip6));
        LogShim("[Winsock TCP connect IPv6] Target [" + std::string(ip6) + "]:" + std::to_string(port) + " (Socket " + std::to_string(s) + ") by " + caller_str);
    }

    struct sockaddr_storage rewritten{};
    int rewritten_len = 0;
    if (CheckAndRewriteSockAddr(name, &rewritten, &rewritten_len, caller)) {
        return g_orig_connect(s, reinterpret_cast<const struct sockaddr*>(&rewritten), rewritten_len);
    }
    return g_orig_connect(s, name, namelen);
}

static __declspec(noinline) int WSAAPI Hooked_WSAConnect(
    SOCKET s,
    const struct sockaddr *name,
    int namelen,
    LPWSABUF lpCallerData,
    LPWSABUF lpCalleeData,
    LPQOS lpSQOS,
    LPQOS lpGQOS
) {
    void* caller = _ReturnAddress();
    std::string caller_str = GetCallerModuleInfo(caller);

    if (name && name->sa_family == AF_INET) {
        const auto* sin = reinterpret_cast<const sockaddr_in*>(name);
        uint16_t port = ntohs(sin->sin_port);
        char ip[INET_ADDRSTRLEN] = {0};
        inet_ntop(AF_INET, &sin->sin_addr, ip, sizeof(ip));
        std::string ip_str(ip);

        if (port == 3216 || (port >= 3210 && port <= 3220)) {
            g_lsx_socket.store(s);
            LogShim("[Winsock TCP] Intercepted WSAConnect() to Anadius LSX on " + ip_str + ":" + std::to_string(port) + " (Socket " + std::to_string(s) + ") by " + caller_str);
        } else {
            LogShim("[Winsock TCP WSAConnect] Target " + ip_str + ":" + std::to_string(port) + " (Socket " + std::to_string(s) + ") by " + caller_str);
        }
    } else if (name && name->sa_family == AF_INET6) {
        const auto* sin6 = reinterpret_cast<const sockaddr_in6*>(name);
        uint16_t port = ntohs(sin6->sin6_port);
        char ip6[INET6_ADDRSTRLEN] = {0};
        inet_ntop(AF_INET6, &sin6->sin6_addr, ip6, sizeof(ip6));
        LogShim("[Winsock TCP WSAConnect IPv6] Target [" + std::string(ip6) + "]:" + std::to_string(port) + " (Socket " + std::to_string(s) + ") by " + caller_str);
    }

    struct sockaddr_storage rewritten{};
    int rewritten_len = 0;
    if (CheckAndRewriteSockAddr(name, &rewritten, &rewritten_len, caller)) {
        return g_orig_WSAConnect(s, reinterpret_cast<const struct sockaddr*>(&rewritten), rewritten_len,
                                 lpCallerData, lpCalleeData, lpSQOS, lpGQOS);
    }
    return g_orig_WSAConnect(s, name, namelen, lpCallerData, lpCalleeData, lpSQOS, lpGQOS);
}

static __declspec(noinline) BOOL PASCAL FAR Hooked_ConnectEx(
    SOCKET s,
    const struct sockaddr *name,
    int namelen,
    PVOID lpSendBuffer,
    DWORD dwSendDataLength,
    LPDWORD lpdwBytesSent,
    LPOVERLAPPED lpOverlapped
) {
    void* caller = _ReturnAddress();
    int nodelay = 1;
    setsockopt(s, IPPROTO_TCP, TCP_NODELAY, reinterpret_cast<const char*>(&nodelay), sizeof(nodelay));

    if (name && name->sa_family == AF_INET) {
        const auto* sin = reinterpret_cast<const sockaddr_in*>(name);
        uint16_t port = ntohs(sin->sin_port);
        char ip[INET_ADDRSTRLEN] = {0};
        inet_ntop(AF_INET, &sin->sin_addr, ip, sizeof(ip));
        LogShim("[Winsock TCP ConnectEx] Target " + std::string(ip) + ":" + std::to_string(port) + " (Socket " + std::to_string(s) + ") by " + GetCallerModuleInfo(caller));
    }

    struct sockaddr_storage rewritten{};
    int rewritten_len = 0;
    if (CheckAndRewriteSockAddr(name, &rewritten, &rewritten_len, caller)) {
        if (g_real_ConnectEx) {
            return g_real_ConnectEx(s, reinterpret_cast<const struct sockaddr*>(&rewritten), rewritten_len,
                                   lpSendBuffer, dwSendDataLength, lpdwBytesSent, lpOverlapped);
        }
    }
    if (g_real_ConnectEx) {
        return g_real_ConnectEx(s, name, namelen, lpSendBuffer, dwSendDataLength, lpdwBytesSent, lpOverlapped);
    }
    return FALSE;
}

static __declspec(noinline) int WSAAPI Hooked_WSAIoctl(
    SOCKET s,
    DWORD dwIoControlCode,
    LPVOID lpvInBuffer,
    DWORD cbInBuffer,
    LPVOID lpvOutBuffer,
    DWORD cbOutBuffer,
    LPDWORD lpcbBytesReturned,
    LPWSAOVERLAPPED lpOverlapped,
    LPWSAOVERLAPPED_COMPLETION_ROUTINE lpCompletionRoutine
) {
    int res = g_orig_WSAIoctl(s, dwIoControlCode, lpvInBuffer, cbInBuffer,
                              lpvOutBuffer, cbOutBuffer, lpcbBytesReturned,
                              lpOverlapped, lpCompletionRoutine);

    if (res == 0 && dwIoControlCode == SIO_GET_EXTENSION_FUNCTION_POINTER) {
        if (lpvInBuffer && cbInBuffer >= sizeof(GUID) && lpvOutBuffer && cbOutBuffer >= sizeof(void*)) {
            const GUID* guid = reinterpret_cast<const GUID*>(lpvInBuffer);
            static const GUID kConnectExGuid = WSAID_CONNECTEX;
            if (std::memcmp(guid, &kConnectExGuid, sizeof(GUID)) == 0) {
                g_real_ConnectEx = *reinterpret_cast<LPFN_CONNECTEX*>(lpvOutBuffer);
                *reinterpret_cast<LPFN_CONNECTEX*>(lpvOutBuffer) = &Hooked_ConnectEx;
                LogShim("[WinSock] Intercepted WSAIoctl SIO_GET_EXTENSION_FUNCTION_POINTER for ConnectEx -> Detoured!");
            }
        }
    }

    return res;
}

static __declspec(noinline) int WSAAPI Hooked_send(SOCKET s, const char *buf, int len, int flags) {
    if (buf && len > 0 && s == g_lsx_socket.load()) {
        std::string sample(buf, len > 160 ? 160 : len);
        LogShim("[Winsock TCP LSX] Outgoing send (" + std::to_string(len) + " bytes): " + sample);
    }
    return g_orig_send(s, buf, len, flags);
}

static __declspec(noinline) int WSAAPI Hooked_recv(SOCKET s, char *buf, int len, int flags) {
    int res = g_orig_recv(s, buf, len, flags);
    if (res > 0 && buf && s == g_lsx_socket.load()) {
        std::string data(buf, res);
        LogShim("[Winsock TCP LSX] Incoming recv (" + std::to_string(res) + " bytes): " + data.substr(0, 160));

        bool modified = false;
        auto replace_all = [&](const std::string& from, const std::string& to) {
            size_t pos = 0;
            while ((pos = data.find(from, pos)) != std::string::npos) {
                data.replace(pos, from.length(), to);
                pos += to.length();
                modified = true;
            }
        };

        replace_all("shutdown", "running_");
        replace_all("Shutdown", "Running_");
        replace_all("IS_MANUAL_OFFLINE", "IS_ONLINE_ALLOWED");
        replace_all("IsManualOffline", "IsOnlineAllowed");
        replace_all("offline", "online_");
        replace_all("Offline", "Online_");
        replace_all("unavailable", "available__");
        replace_all("EOS", "OK_");
        replace_all("-1577058293", "0000000000");

        if (modified) {
            size_t copy_len = data.length() < static_cast<size_t>(len) ? data.length() : static_cast<size_t>(len);
            std::memcpy(buf, data.data(), copy_len);
            LogShim("[Winsock TCP LSX] Sanitized LSX response buffer in memory!");
        }
    }
    return res;
}

static __declspec(noinline) int WSAAPI Hooked_bind(SOCKET s, const struct sockaddr *name, int namelen) {
    int ret = g_orig_bind(s, name, namelen);
    if (ret == 0 && name && name->sa_family == AF_INET) {
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
    return ret;
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
        const auto* sin = reinterpret_cast<const sockaddr_in*>(to);
        dst_port = ntohs(sin->sin_port);

        // If this packet is already addressed directly to our relay server, pass through directly
        if (sin->sin_addr.s_addr == g_relay_addr.sin_addr.s_addr &&
            sin->sin_port == g_relay_addr.sin_port) {
            return g_orig_sendto(s, buf, len, flags, to, tolen);
        }
    }

    // If this packet already has the Seamless Envelope magic, do not re-wrap!
    if (len >= 4 && std::memcmp(buf, PROTOCOL_MAGIC, 4) == 0) {
        return g_orig_sendto(s, buf, len, flags, to, tolen);
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

    const uint8_t* b196B650 = base + 0x196B650;
    const uint8_t* b24D92D0 = base + 0x24D92D0;
    const uint8_t* b4F32B5F = base + 0x4F32B5F;
    const uint8_t* b24DDCEA = base + 0x24DDCEA;
    const uint8_t* b4D89F4C = base + 0x4D89F4C;
    const uint8_t* b27E2C72 = base + 0x27E2C72;
    const uint8_t* b5BC58F2 = base + 0x5BC58F2;

    std::string dump = "[MemoryDump: " + std::string(context) + "] ";
    dump += "Entitlement=0x" + BytesToHex(b196B650, 6) + " (exp B8 01 00 00 00 C3) ";
    dump += "OriginCheck=0x" + BytesToHex(b24D92D0, 11) + " ";
    dump += "OnlineAccess=0x" + BytesToHex(b4F32B5F, 3) + " (exp 40 B6 01) ";
    dump += "OriginInternal=0x" + BytesToHex(b24DDCEA, 8) + " ";
    dump += "Popup=0x" + BytesToHex(b4D89F4C, 3) + " (exp 90 90 90) ";
    dump += "FailRet=0x" + BytesToHex(b27E2C72, 5) + " (exp B8 00 00 00 00) ";
    dump += "PlayTile=0x" + BytesToHex(b5BC58F2, 5) + " (exp C6 44 24 70 01)";

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

    HMODULE hWeb = GetModuleHandleW(L"EAWebKit.dll");
    if (hWeb) {
        const uint8_t* wbase = reinterpret_cast<const uint8_t*>(hWeb);
        dump += " | EAWebKit at 0x" + std::to_string((uintptr_t)hWeb);
        dump += " 0xA8A560=[" + BytesToHex(wbase + 0xA8A560, 6) + "] (exp B8 00 00 00 00 C3)";
        dump += " 0xA6FBD0=[" + BytesToHex(wbase + 0xA6FBD0, 6) + "] (exp B8 01 00 00 00 C3)";
    } else {
        dump += " | EAWebKit: NOT LOADED";
    }

    LogShim(dump);
}

// -----------------------------------------------------------------------------
// DirtySDK ProtoSSL & CryptoAPI TLS Bypass
// -----------------------------------------------------------------------------
static uint8_t g_ca_modulus[256] = {
    0xBA, 0x93, 0xC2, 0x06, 0x5A, 0x8B, 0xD4, 0xC9, 0xF8, 0x88, 0x4C, 0xC1, 0xF8, 0x8D, 0xCA, 0xAC,
    0xD8, 0xDD, 0x20, 0x56, 0xC6, 0x4E, 0xA1, 0x9E, 0x46, 0x01, 0x29, 0x8C, 0x5E, 0xB6, 0x1A, 0x26,
    0x58, 0x38, 0xA9, 0xC8, 0x86, 0x72, 0xFF, 0x81, 0xA0, 0x25, 0x5E, 0x89, 0x49, 0xC2, 0x7D, 0x36,
    0x68, 0x2C, 0x4A, 0x48, 0xF2, 0xC4, 0x4B, 0x95, 0xC7, 0x2F, 0x52, 0x9A, 0xD7, 0xAE, 0x4E, 0x9E,
    0xF9, 0x92, 0xEF, 0x82, 0x3A, 0x70, 0x7C, 0xDA, 0xF7, 0x5E, 0xE9, 0x3F, 0xCD, 0x4F, 0xD2, 0x99,
    0x68, 0xBC, 0xCF, 0xB7, 0xF8, 0xEB, 0x18, 0xD3, 0xE7, 0x0B, 0x61, 0x02, 0xA6, 0x37, 0x34, 0x94,
    0x69, 0x70, 0x9D, 0xBF, 0x00, 0xC6, 0x28, 0x03, 0xDC, 0x33, 0xEF, 0x2A, 0x93, 0x47, 0xA1, 0x13,
    0x73, 0x7E, 0xD0, 0x63, 0xA3, 0xC3, 0x6B, 0x5A, 0x40, 0xB1, 0xFC, 0xE8, 0xEB, 0xFA, 0xB6, 0x10,
    0x89, 0xF7, 0x82, 0x47, 0x1C, 0x64, 0xB5, 0xF2, 0xB1, 0x76, 0x4E, 0xA6, 0xA0, 0xE1, 0xD0, 0x2F,
    0x57, 0xA4, 0xB8, 0xF6, 0xE6, 0x9F, 0xD7, 0xF7, 0x93, 0x29, 0x30, 0x1F, 0x9C, 0x6F, 0x0E, 0x6E,
    0x94, 0xC8, 0x48, 0xA8, 0xE2, 0x0C, 0x61, 0x68, 0x8A, 0x8B, 0xD3, 0x1B, 0x6B, 0xBB, 0xA7, 0x1B,
    0xF9, 0xE6, 0xCA, 0x94, 0x1F, 0x18, 0xC7, 0xAE, 0x6A, 0x47, 0x63, 0x40, 0x96, 0xEE, 0x5E, 0x1E,
    0xA2, 0xED, 0xF7, 0xD1, 0x18, 0x35, 0x47, 0x42, 0x59, 0xF6, 0xBB, 0x71, 0xBA, 0x6E, 0x77, 0x1C,
    0xA1, 0xF2, 0x36, 0x78, 0x68, 0x62, 0x13, 0x78, 0xDB, 0x82, 0xAE, 0x22, 0x61, 0xBD, 0x01, 0x4D,
    0x78, 0x4D, 0xB4, 0xEA, 0x99, 0xF1, 0xED, 0x16, 0xBE, 0x98, 0x74, 0x63, 0xA3, 0x5D, 0xEC, 0x28,
    0x85, 0xD1, 0x18, 0x83, 0xB0, 0x22, 0x08, 0xFD, 0x9A, 0x4B, 0x57, 0xDE, 0x65, 0x14, 0xFA, 0x63
};

using PFN_CertVerifyCertificateChainPolicy = BOOL(WINAPI *)(LPCSTR, PCCERT_CHAIN_CONTEXT, PCERT_CHAIN_POLICY_PARA, PCERT_CHAIN_POLICY_STATUS);
static PFN_CertVerifyCertificateChainPolicy g_orig_CertVerifyCertificateChainPolicy = nullptr;

static BOOL WINAPI Hooked_CertVerifyCertificateChainPolicy(
    LPCSTR pszPolicyOID, PCCERT_CHAIN_CONTEXT pChainContext,
    PCERT_CHAIN_POLICY_PARA pPolicyPara,
    PCERT_CHAIN_POLICY_STATUS pPolicyStatus) {
    if (pPolicyStatus) {
        pPolicyStatus->dwError = 0;
    }
    if (pChainContext) {
        auto *ctx = const_cast<CERT_CHAIN_CONTEXT *>(pChainContext);
        ctx->TrustStatus.dwErrorStatus = CERT_TRUST_NO_ERROR;
        for (DWORD c = 0; c < ctx->cChain; ++c) {
            if (ctx->rgpChain[c]) {
                ctx->rgpChain[c]->TrustStatus.dwErrorStatus = CERT_TRUST_NO_ERROR;
            }
        }
    }
    LogShim("[CryptoAPI] Hooked_CertVerifyCertificateChainPolicy -> Suppressed error, returning TRUE");
    return TRUE;
}

using PFN_CertGetCertificateChain = BOOL(WINAPI *)(HCERTCHAINENGINE, PCCERT_CONTEXT, LPFILETIME, HCERTSTORE, PCERT_CHAIN_PARA, DWORD, LPVOID, PCCERT_CHAIN_CONTEXT *);
static PFN_CertGetCertificateChain g_orig_CertGetCertificateChain = nullptr;

static BOOL WINAPI Hooked_CertGetCertificateChain(
    HCERTCHAINENGINE hChainEngine, PCCERT_CONTEXT pCertContext,
    LPFILETIME pTime, HCERTSTORE hAdditionalStore,
    PCERT_CHAIN_PARA pChainPara, DWORD dwFlags, LPVOID pvReserved,
    PCCERT_CHAIN_CONTEXT * ppChainContext) {
    BOOL res = g_orig_CertGetCertificateChain(hChainEngine, pCertContext, pTime, hAdditionalStore, pChainPara, dwFlags, pvReserved, ppChainContext);
    if (res && ppChainContext && *ppChainContext) {
        auto *ctx = const_cast<CERT_CHAIN_CONTEXT *>(*ppChainContext);
        ctx->TrustStatus.dwErrorStatus = CERT_TRUST_NO_ERROR;
        ctx->TrustStatus.dwInfoStatus = CERT_TRUST_HAS_EXACT_MATCH_ISSUER;
        for (DWORD c = 0; c < ctx->cChain; ++c) {
            if (ctx->rgpChain[c]) {
                ctx->rgpChain[c]->TrustStatus.dwErrorStatus = CERT_TRUST_NO_ERROR;
                ctx->rgpChain[c]->TrustStatus.dwInfoStatus = CERT_TRUST_HAS_EXACT_MATCH_ISSUER;
                for (DWORD e = 0; e < ctx->rgpChain[c]->cElement; ++e) {
                    if (ctx->rgpChain[c]->rgpElement[e]) {
                        ctx->rgpChain[c]->rgpElement[e]->TrustStatus.dwErrorStatus = CERT_TRUST_NO_ERROR;
                        ctx->rgpChain[c]->rgpElement[e]->TrustStatus.dwInfoStatus = CERT_TRUST_HAS_EXACT_MATCH_ISSUER;
                    }
                }
            }
        }
        LogShim("[CryptoAPI] Hooked_CertGetCertificateChain -> Forced CERT_TRUST_NO_ERROR across all chains & elements");
    }
    return res;
}

using PFN_BCryptVerifySignature = NTSTATUS(WINAPI *)(BCRYPT_KEY_HANDLE, void *, PUCHAR, ULONG, PUCHAR, ULONG, ULONG);
static PFN_BCryptVerifySignature g_orig_BCryptVerifySignature = nullptr;

static NTSTATUS WINAPI Hooked_BCryptVerifySignature(
    BCRYPT_KEY_HANDLE hKey, void *pPaddingInfo, PUCHAR pbHash, ULONG cbHash,
    PUCHAR pbSignature, ULONG cbSignature, ULONG dwFlags) {
    NTSTATUS status = g_orig_BCryptVerifySignature(hKey, pPaddingInfo, pbHash, cbHash, pbSignature, cbSignature, dwFlags);
    if (status != 0) {
        return 0; // STATUS_SUCCESS
    }
    return status;
}

static int PatchDirtySdkCaTableInModule(HMODULE mod, const char* label) {
    if (!mod) return 0;
    MODULEINFO mod_info{};
    if (!GetModuleInformation(GetCurrentProcess(), mod, &mod_info, sizeof(mod_info))) {
        return 0;
    }

    const uint8_t *base = static_cast<const uint8_t *>(mod_info.lpBaseOfDll);
    size_t total_size = mod_info.SizeOfImage;

    const char* kTargetStr[] = {
        "DigiCert Global Root CA",
        "GOS 2015 Certificate Authority",
        "GOS 2015 CA",
        "VeriSign Class 3 Public Primary Certification Authority - G5",
        "VeriSign G5",
        "Amazon Root CA 1"
    };
    const size_t kTargetCount = 6;
    int found = 0;

    MEMORY_BASIC_INFORMATION mbi{};
    const uint8_t* cur = base;
    while (cur < base + total_size) {
        if (VirtualQuery(cur, &mbi, sizeof(mbi)) == 0) break;
        if (mbi.State == MEM_COMMIT && !(mbi.Protect & PAGE_GUARD) &&
            (mbi.Protect & (PAGE_READONLY | PAGE_READWRITE | PAGE_EXECUTE_READ | PAGE_EXECUTE_READWRITE))) {
            const uint8_t* region_start = static_cast<const uint8_t*>(mbi.BaseAddress);
            size_t region_size = mbi.RegionSize;

            for (size_t s = 0; s < kTargetCount; ++s) {
                const char* target = kTargetStr[s];
                size_t str_len = strlen(target) + 1;
                if (region_size < str_len + 0x60) continue;

                for (size_t i = 0; i < region_size - str_len - 0x60; ++i) {
                    if (std::memcmp(region_start + i, target, str_len) != 0) {
                        continue;
                    }

                    uintptr_t name_addr = reinterpret_cast<uintptr_t>(region_start + i);
                    uintptr_t *p_modulus = reinterpret_cast<uintptr_t *>(name_addr + 0x50);
                    uint32_t exp_len = *reinterpret_cast<const uint32_t *>(name_addr + 0x58);
                    uint32_t exp_val = *reinterpret_cast<const uint32_t *>(name_addr + 0x5C) & 0x00FFFFFF;

                    if (exp_len != 3 || exp_val != 0x010001) {
                        continue;
                    }

                    if (*p_modulus == reinterpret_cast<uintptr_t>(g_ca_modulus)) {
                        found++;
                        continue; // already patched
                    }

                    DWORD old_protect = 0;
                    if (VirtualProtect(p_modulus, sizeof(uintptr_t), PAGE_READWRITE, &old_protect)) {
                        *p_modulus = reinterpret_cast<uintptr_t>(g_ca_modulus);
                        VirtualProtect(p_modulus, sizeof(uintptr_t), old_protect, &old_protect);
                        found++;
                        LogShim(std::string("[SSL Bypass] SUCCESS: Patched DirtySDK ") + target + " modulus pointer in " + label);
                    }
                }
            }
        }
        cur += mbi.RegionSize;
    }
    return found;
}

static bool ApplyFifaExePatches() {
    HMODULE hExe = GetModuleHandleW(NULL);
    if (!hExe) return false;
    const uint8_t* base = reinterpret_cast<const uint8_t*>(hExe);

    // Root Entitlement Check (RVA 0x196B650): Force return 1 (True)
    uint8_t ret_true[6] = {0xB8, 0x01, 0x00, 0x00, 0x00, 0xC3}; // mov eax, 1; ret
    // Root OriginCheckOnline (RVA 0x24D92D0): Force *rcx = 1 (Online) and return 0 (Success)
    uint8_t origin_online[11] = {0x48, 0x85, 0xC9, 0x74, 0x03, 0xC6, 0x01, 0x01, 0x31, 0xC0, 0xC3};
    // Frostbite OnlineAccess Local Flag (RVA 0x4F32B5F): mov sil, 1 (entitlement granted)
    uint8_t mov_sil_one[3] = {0x40, 0xB6, 0x01};
    // OriginSDK Internal Online State (RVA 0x24DDCEA): Force *rsi = 1 (Online), ebx = 0, jmp 0x1424ddd00
    uint8_t origin_internal[11] = {0xC6, 0x06, 0x01, 0x31, 0xDB, 0xEB, 0x0F, 0x90, 0x90, 0x90, 0x90};

    // Sub-call failure coercions
    uint8_t mov_zero[5] = {0xB8, 0x00, 0x00, 0x00, 0x00};
    uint8_t xor_eax[2] = {0x31, 0xC0};
    uint8_t nop6[6] = {0x90, 0x90, 0x90, 0x90, 0x90, 0x90};
    uint8_t nop3[3] = {0x90, 0x90, 0x90};
    // Frostbite isPlayTileSelectable default unlock (RVA 0x5BC58F2): mov byte ptr [rsp+70h], 1
    uint8_t unlock_tile[5] = {0xC6, 0x44, 0x24, 0x70, 0x01};

    bool ok = true;
    ok &= SafeMemoryPatch(reinterpret_cast<uintptr_t>(base + 0x196B650), ret_true, 6);
    ok &= SafeMemoryPatch(reinterpret_cast<uintptr_t>(base + 0x24D92D0), origin_online, 11);
    ok &= SafeMemoryPatch(reinterpret_cast<uintptr_t>(base + 0x4F32B5F), mov_sil_one, 3);
    ok &= SafeMemoryPatch(reinterpret_cast<uintptr_t>(base + 0x24DDCEA), origin_internal, 11);
    ok &= SafeMemoryPatch(reinterpret_cast<uintptr_t>(base + 0x27E2C72), mov_zero, 5);
    ok &= SafeMemoryPatch(reinterpret_cast<uintptr_t>(base + 0x27E2C79), mov_zero, 5);
    ok &= SafeMemoryPatch(reinterpret_cast<uintptr_t>(base + 0x27E2CC3), xor_eax, 2);
    ok &= SafeMemoryPatch(reinterpret_cast<uintptr_t>(base + 0x27E2B90), nop6, 6);
    ok &= SafeMemoryPatch(reinterpret_cast<uintptr_t>(base + 0x4D89F4C), nop3, 3);
    ok &= SafeMemoryPatch(reinterpret_cast<uintptr_t>(base + 0x5BC58F2), unlock_tile, 5);
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

static bool ApplyEAWebKitPatches() {
    HMODULE hWeb = GetModuleHandleW(L"EAWebKit.dll");
    if (!hWeb) {
        hWeb = LoadLibraryW(L"EAWebKit.dll");
    }
    if (!hWeb) return false;

    MODULEINFO info{};
    if (!GetModuleInformation(GetCurrentProcess(), hWeb, &info, sizeof(info))) return false;
    const uint8_t* base = reinterpret_cast<const uint8_t*>(info.lpBaseOfDll);

    // SSL_get_verify_result at RVA 0xA8A560: mov eax, 0; ret (force X509_V_OK = 0)
    uint8_t ret_zero[6] = {0xB8, 0x00, 0x00, 0x00, 0x00, 0xC3};
    // X509_verify_cert at RVA 0xA6FBD0: mov eax, 1; ret (force chain verification success = 1)
    uint8_t ret_one[6]  = {0xB8, 0x01, 0x00, 0x00, 0x00, 0xC3};

    bool ok = true;
    ok &= SafeMemoryPatch(reinterpret_cast<uintptr_t>(base + 0xA8A560), ret_zero, 6);
    ok &= SafeMemoryPatch(reinterpret_cast<uintptr_t>(base + 0xA6FBD0), ret_one, 6);

    PatchDirtySdkCaTableInModule(hWeb, "EAWebKit.dll");

    LogShim("[SSL Bypass] SUCCESS: Patched EAWebKit BoringSSL verify functions (SSL_get_verify_result & X509_verify_cert) and CA table.");
    return ok;
}

static DWORD WINAPI PatchWorkerThread(LPVOID) {
    LogShim("[PatchWorker] Background patcher thread started. Applying root entitlement, auth & popup patches...");
    ApplyFifaExePatches();
    ApplyEAWebKitPatches();
    DumpPatchedMemory("InitialPostPatch");

    bool cards_patched = false;
    bool eawebkit_patched = false;
    uint32_t iteration = 0;

    while (g_running.load(std::memory_order_relaxed)) {
        Sleep(50);
        iteration++;

        HMODULE hExe = GetModuleHandleW(NULL);
        if (hExe) {
            const uint8_t* base = reinterpret_cast<const uint8_t*>(hExe);
            const uint8_t* b196B650 = base + 0x196B650;
            const uint8_t* b24D92D0 = base + 0x24D92D0;
            const uint8_t* b4F32B5F = base + 0x4F32B5F;
            const uint8_t* b4D89F4C = base + 0x4D89F4C;
            const uint8_t* b27E2C72 = base + 0x27E2C72;
            const uint8_t* b5BC58F2 = base + 0x5BC58F2;

            // Check if patched bytes reverted
            if (b196B650[0] != 0xB8 || b24D92D0[0] != 0x48 || b4F32B5F[0] != 0x40 ||
                b4D89F4C[0] != 0x90 || b27E2C72[0] != 0xB8 || b27E2C72[1] != 0x00 ||
                b5BC58F2[0] != 0xC6) {
                LogShim("[PatchWorker ALERT] Detected byte revert in FIFA22.exe! Re-applying patches...");
                ApplyFifaExePatches();
                DumpPatchedMemory("RevertedReapplied");
            }
        }

        HMODULE hCards = GetModuleHandleW(L"CardsDLL_Win64_retail.dll");
        if (!hCards && iteration >= 20) {
            hCards = LoadLibraryW(L"CardsDLL_Win64_retail.dll");
        }
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

        HMODULE hWeb = GetModuleHandleW(L"EAWebKit.dll");
        if (!hWeb) {
            hWeb = LoadLibraryW(L"EAWebKit.dll");
        }
        if (hWeb) {
            const uint8_t* wbase = reinterpret_cast<const uint8_t*>(hWeb);
            const uint8_t* wA8A560 = wbase + 0xA8A560;
            const uint8_t* wA6FBD0 = wbase + 0xA6FBD0;

            if (!eawebkit_patched) {
                LogShim("[PatchWorker] EAWebKit.dll detected! Applying BoringSSL patches...");
                ApplyEAWebKitPatches();
                eawebkit_patched = true;
                DumpPatchedMemory("EAWebKitLoadedPostPatch");
            } else if (wA8A560[0] != 0xB8 || wA6FBD0[0] != 0xB8) {
                LogShim("[PatchWorker ALERT] Detected byte revert in EAWebKit.dll! Re-applying patches...");
                ApplyEAWebKitPatches();
                DumpPatchedMemory("EAWebKitRevertedReapplied");
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
// Force UDP Match Socket Worker (Bypasses Frostbite UI flow)
// -----------------------------------------------------------------------------
static DWORD WINAPI ForceUdpSocketWorker(LPVOID) {
    LogShim("[ForceUDP] Force UDP Socket thread started. Waiting 1.5 seconds for Frostbite engine init...");
    Sleep(1500);

    if (g_game_socket.load() == INVALID_SOCKET) {
        LogShim("[ForceUDP] Forcing direct UDP socket bind on port 3659...");
        SOCKET raw_s = socket(AF_INET, SOCK_DGRAM, IPPROTO_UDP);
        if (raw_s != INVALID_SOCKET) {
            int reuse = 1;
            setsockopt(raw_s, SOL_SOCKET, SO_REUSEADDR, reinterpret_cast<const char*>(&reuse), sizeof(reuse));
            sockaddr_in sa{};
            sa.sin_family = AF_INET;
            sa.sin_port = htons(3659);
            sa.sin_addr.s_addr = INADDR_ANY;
            int bres = bind(raw_s, reinterpret_cast<const sockaddr*>(&sa), sizeof(sa));
            if (bres == 0) {
                LogShim("[ForceUDP] Direct UDP socket BOUND on port 3659 (Socket: " + std::to_string(raw_s) + ")");
                g_game_socket.store(raw_s);
            } else {
                LogShim("[ForceUDP] Direct UDP bind failed. Error: " + std::to_string(WSAGetLastError()));
                closesocket(raw_s);
            }
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

    // Immediately load and patch EAWebKit.dll BoringSSL functions & CA table
    ApplyEAWebKitPatches();

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
            void* pConnect = reinterpret_cast<void*>(GetProcAddress(hWs2, "connect"));
            void* pWSAConnect = reinterpret_cast<void*>(GetProcAddress(hWs2, "WSAConnect"));
            void* pWSAIoctl = reinterpret_cast<void*>(GetProcAddress(hWs2, "WSAIoctl"));
            void* pGetaddrinfo = reinterpret_cast<void*>(GetProcAddress(hWs2, "getaddrinfo"));
            void* pGetAddrInfoW = reinterpret_cast<void*>(GetProcAddress(hWs2, "GetAddrInfoW"));
            void* pSend = reinterpret_cast<void*>(GetProcAddress(hWs2, "send"));
            void* pRecv = reinterpret_cast<void*>(GetProcAddress(hWs2, "recv"));

            LogShim("Target addresses: bind=0x" + std::to_string((uintptr_t)pBind) +
                    ", sendto=0x" + std::to_string((uintptr_t)pSendto) +
                    ", recvfrom=0x" + std::to_string((uintptr_t)pRecvfrom) +
                    ", connect=0x" + std::to_string((uintptr_t)pConnect) +
                    ", WSAConnect=0x" + std::to_string((uintptr_t)pWSAConnect) +
                    ", WSAIoctl=0x" + std::to_string((uintptr_t)pWSAIoctl) +
                    ", getaddrinfo=0x" + std::to_string((uintptr_t)pGetaddrinfo) +
                    ", GetAddrInfoW=0x" + std::to_string((uintptr_t)pGetAddrInfoW) +
                    ", send=0x" + std::to_string((uintptr_t)pSend) +
                    ", recv=0x" + std::to_string((uintptr_t)pRecv));

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
            if (pConnect) {
                MH_STATUS s = MH_CreateHook(pConnect, reinterpret_cast<LPVOID>(&Hooked_connect), reinterpret_cast<LPVOID*>(&g_orig_connect));
                LogShim("Hook connect status: " + std::to_string(s));
            }
            if (pWSAConnect) {
                MH_STATUS s = MH_CreateHook(pWSAConnect, reinterpret_cast<LPVOID>(&Hooked_WSAConnect), reinterpret_cast<LPVOID*>(&g_orig_WSAConnect));
                LogShim("Hook WSAConnect status: " + std::to_string(s));
            }
            if (pWSAIoctl) {
                MH_STATUS s = MH_CreateHook(pWSAIoctl, reinterpret_cast<LPVOID>(&Hooked_WSAIoctl), reinterpret_cast<LPVOID*>(&g_orig_WSAIoctl));
                LogShim("Hook WSAIoctl status: " + std::to_string(s));
            }
            if (pGetaddrinfo) {
                MH_STATUS s = MH_CreateHook(pGetaddrinfo, reinterpret_cast<LPVOID>(&Hooked_getaddrinfo), reinterpret_cast<LPVOID*>(&g_orig_getaddrinfo));
                LogShim("Hook getaddrinfo status: " + std::to_string(s));
            }
            if (pGetAddrInfoW) {
                MH_STATUS s = MH_CreateHook(pGetAddrInfoW, reinterpret_cast<LPVOID>(&Hooked_GetAddrInfoW), reinterpret_cast<LPVOID*>(&g_orig_GetAddrInfoW));
                LogShim("Hook GetAddrInfoW status: " + std::to_string(s));
            }
            if (pSend) {
                MH_STATUS s = MH_CreateHook(pSend, reinterpret_cast<LPVOID>(&Hooked_send), reinterpret_cast<LPVOID*>(&g_orig_send));
                LogShim("Hook send status: " + std::to_string(s));
            }
            if (pRecv) {
                MH_STATUS s = MH_CreateHook(pRecv, reinterpret_cast<LPVOID>(&Hooked_recv), reinterpret_cast<LPVOID*>(&g_orig_recv));
                LogShim("Hook recv status: " + std::to_string(s));
            }

            HMODULE hCrypt32 = LoadLibraryW(L"crypt32.dll");
            if (hCrypt32) {
                FARPROC pCertVerify = GetProcAddress(hCrypt32, "CertVerifyCertificateChainPolicy");
                if (pCertVerify) {
                    MH_STATUS s = MH_CreateHook(reinterpret_cast<LPVOID>(pCertVerify), reinterpret_cast<LPVOID>(&Hooked_CertVerifyCertificateChainPolicy), reinterpret_cast<LPVOID*>(&g_orig_CertVerifyCertificateChainPolicy));
                    LogShim("Hook CertVerifyCertificateChainPolicy status: " + std::to_string(s));
                }
                FARPROC pCertChain = GetProcAddress(hCrypt32, "CertGetCertificateChain");
                if (pCertChain) {
                    MH_STATUS s = MH_CreateHook(reinterpret_cast<LPVOID>(pCertChain), reinterpret_cast<LPVOID>(&Hooked_CertGetCertificateChain), reinterpret_cast<LPVOID*>(&g_orig_CertGetCertificateChain));
                    LogShim("Hook CertGetCertificateChain status: " + std::to_string(s));
                }
            }
            HMODULE hBcrypt = LoadLibraryW(L"bcrypt.dll");
            if (hBcrypt) {
                FARPROC pBcryptVerify = GetProcAddress(hBcrypt, "BCryptVerifySignature");
                if (pBcryptVerify) {
                    MH_STATUS s = MH_CreateHook(reinterpret_cast<LPVOID>(pBcryptVerify), reinterpret_cast<LPVOID>(&Hooked_BCryptVerifySignature), reinterpret_cast<LPVOID*>(&g_orig_BCryptVerifySignature));
                    LogShim("Hook BCryptVerifySignature status: " + std::to_string(s));
                }
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

        // Spawn background force UDP match socket worker
        HANDLE hForceUdp = CreateThread(nullptr, 0, ForceUdpSocketWorker, nullptr, 0, nullptr);
        if (hForceUdp) CloseHandle(hForceUdp);
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
