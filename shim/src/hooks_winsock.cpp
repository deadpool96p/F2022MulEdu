#include "f22_shim.h"
#include "steam_bridge.hpp"
#include "overlay/input_hook.h"
#include <MinHook.h>
#include <sstream>
#include <cstring>
#include <string>
#include <mutex>

namespace f22 {

// TCP Function pointers
using PFN_getaddrinfo = int(WSAAPI*)(const char *pNodeName, const char *pServiceName, const ADDRINFOA *pHints, PADDRINFOA *ppResult);
using PFN_GetAddrInfoW = int(WSAAPI*)(const PCWSTR pNodeName, const PCWSTR pServiceName, const ADDRINFOW *pHints, PADDRINFOW *ppResult);
using PFN_connect = int(WSAAPI*)(SOCKET s, const struct sockaddr *name, int namelen);
using PFN_WSAConnect = int(WSAAPI*)(SOCKET s, const struct sockaddr *name, int namelen, LPWSABUF lpCallerData, LPWSABUF lpCalleeData, LPQOS lpSQOS, LPQOS lpGQOS);

// UDP Function pointers
using PFN_bind = int(WSAAPI*)(SOCKET s, const struct sockaddr *name, int namelen);
using PFN_sendto = int(WSAAPI*)(SOCKET s, const char *buf, int len, int flags, const struct sockaddr *to, int tolen);
using PFN_WSASendTo = int(WSAAPI*)(SOCKET s, LPWSABUF lpBuffers, DWORD dwBufferCount, LPDWORD lpNumberOfBytesSent, DWORD dwFlags, const struct sockaddr *lpTo, int iTolen, LPWSAOVERLAPPED lpOverlapped, LPWSAOVERLAPPED_COMPLETION_ROUTINE lpCompletionRoutine);
using PFN_recvfrom = int(WSAAPI*)(SOCKET s, char *buf, int len, int flags, struct sockaddr *from, int *fromlen);
using PFN_WSARecvFrom = int(WSAAPI*)(SOCKET s, LPWSABUF lpBuffers, DWORD dwBufferCount, LPDWORD lpNumberOfBytesRecvd, LPDWORD lpFlags, struct sockaddr *lpFrom, LPINT lpFromlen, LPWSAOVERLAPPED lpOverlapped, LPWSAOVERLAPPED_COMPLETION_ROUTINE lpCompletionRoutine);

static PFN_getaddrinfo g_orig_getaddrinfo = nullptr;
static PFN_GetAddrInfoW g_orig_GetAddrInfoW = nullptr;
static PFN_connect g_orig_connect = nullptr;
static PFN_WSAConnect g_orig_WSAConnect = nullptr;

static PFN_bind g_orig_bind = nullptr;
static PFN_sendto g_orig_sendto = nullptr;
static PFN_WSASendTo g_orig_WSASendTo = nullptr;
static PFN_recvfrom g_orig_recvfrom = nullptr;
static PFN_WSARecvFrom g_orig_WSARecvFrom = nullptr;

// Live Netcode Telemetry & Socket State
static std::mutex g_metrics_mutex;
static NetcodeMetrics g_metrics;
static SOCKET g_game_socket = INVALID_SOCKET;
static uint64_t g_last_send_time = 0;
static uint64_t g_last_recv_time = 0;
static uint64_t g_target_steam_id = 0;

NetcodeMetrics GetCurrentNetcodeMetrics() {
    std::lock_guard<std::mutex> lock(g_metrics_mutex);
    return g_metrics;
}

static bool ShouldRerouteHost(std::string_view host) {
    if (host.empty()) return false;

    if (host.find("gosredirector.ea.com") != std::string_view::npos ||
        host.find("river.data.ea.com") != std::string_view::npos ||
        host.find("pin-river.data.ea.com") != std::string_view::npos ||
        host.find("blaze") != std::string_view::npos) {
        return true;
    }

    if (GetConfig().reroute_all_ea) {
        if (host.ends_with(".ea.com") || host.ends_with(".ea.net") || host.ends_with(".dice.se")) {
            return true;
        }
    }

    return false;
}

// Hooked getaddrinfo (ANSI)
static int WSAAPI Hooked_getaddrinfo(
    const char *pNodeName,
    const char *pServiceName,
    const ADDRINFOA *pHints,
    PADDRINFOA *ppResult
) {
    const char *query_host = pNodeName ? pNodeName : "";
    bool reroute = ShouldRerouteHost(query_host);

    if (reroute) {
        const std::string& target_host = GetConfig().emulator_host;
        std::ostringstream ss;
        ss << "[WinSock] Intercepted getaddrinfo(\"" << query_host << "\") -> rerouting to " << target_host;
        LogInfo(ss.str());

        return g_orig_getaddrinfo(target_host.c_str(), pServiceName, pHints, ppResult);
    }

    return g_orig_getaddrinfo(pNodeName, pServiceName, pHints, ppResult);
}

// Hooked GetAddrInfoW (Unicode)
static int WSAAPI Hooked_GetAddrInfoW(
    const PCWSTR pNodeName,
    const PCWSTR pServiceName,
    const ADDRINFOW *pHints,
    PADDRINFOW *ppResult
) {
    if (pNodeName) {
        char narrow_buf[256];
        WideCharToMultiByte(CP_UTF8, 0, pNodeName, -1, narrow_buf, sizeof(narrow_buf), nullptr, nullptr);
        if (ShouldRerouteHost(narrow_buf)) {
            const std::string& target_host = GetConfig().emulator_host;
            wchar_t wide_target[64];
            MultiByteToWideChar(CP_UTF8, 0, target_host.c_str(), -1, wide_target, 64);

            std::ostringstream ss;
            ss << "[WinSock] Intercepted GetAddrInfoW(\"" << narrow_buf << "\") -> rerouting to " << target_host;
            LogInfo(ss.str());

            return g_orig_GetAddrInfoW(wide_target, pServiceName, pHints, ppResult);
        }
    }

    return g_orig_GetAddrInfoW(pNodeName, pServiceName, pHints, ppResult);
}

// Helper to inspect and rewrite sockaddr_in destination
static bool CheckAndRewriteSockAddr(const struct sockaddr *name, struct sockaddr_in *out_sin) {
    if (!name || name->sa_family != AF_INET) {
        return false;
    }

    const auto *sin = reinterpret_cast<const struct sockaddr_in*>(name);
    uint16_t port = ntohs(sin->sin_port);
    char ip_str[INET_ADDRSTRLEN];
    inet_ntop(AF_INET, &(sin->sin_addr), ip_str, INET_ADDRSTRLEN);

    const auto& cfg = GetConfig();
    bool should_reroute = (port == cfg.redirector_port || port == cfg.blaze_port);

    if (cfg.log_sockets) {
        std::ostringstream ss;
        ss << "[WinSock] connect() target: " << ip_str << ":" << port
           << (should_reroute ? " [REROUTING]" : " [PASSTHROUGH]");
        LogInfo(ss.str());
    }

    if (should_reroute) {
        std::memcpy(out_sin, sin, sizeof(struct sockaddr_in));
        inet_pton(AF_INET, cfg.emulator_host.c_str(), &(out_sin->sin_addr));
        return true;
    }

    return false;
}

void SetTargetSteamID(uint64_t steam_id) {
    if (steam_id != 0) {
        g_target_steam_id = steam_id;
    }
}

// Netcode Gamepad Streaming
void TransmitNetcodeGamepadPacket(const struct _XINPUT_GAMEPAD& pad) {
    uint8_t packet[24];
    // 8-byte ProtoTunnel header
    uint16_t session_id = htons(1);
    uint16_t peer_id = htons(0); // Broadcast to peer in room
    uint16_t payload_len = htons(16);
    uint16_t proto_flags = htons(0x0000);
    std::memcpy(packet + 0, &session_id, 2);
    std::memcpy(packet + 2, &peer_id, 2);
    std::memcpy(packet + 4, &payload_len, 2);
    std::memcpy(packet + 6, &proto_flags, 2);

    // 16-byte gamepad payload
    uint16_t magic = 0xF220;
    uint8_t type = 0x01; // INPUT_FRAME
    uint8_t flags = 0x00;
    std::memcpy(packet + 8, &magic, 2);
    packet[10] = type;
    packet[11] = flags;
    std::memcpy(packet + 12, &pad, sizeof(XINPUT_GAMEPAD));

    // Send via Steam P2P if available
    if (SteamBridge::Instance().HasSteamNetworking() && g_target_steam_id != 0) {
        SteamBridge::Instance().SendP2PPacket(g_target_steam_id, packet + 8, 16, 1 /* unreliable no delay */, 1 /* channel 1 */);
    }

    // Send via ProtoTunnel UDP Relay or Direct UDP
    SOCKET send_sock = g_game_socket;
    static SOCKET s_fallback_gamepad_socket = INVALID_SOCKET;
    if (send_sock == INVALID_SOCKET) {
        if (s_fallback_gamepad_socket == INVALID_SOCKET) {
            s_fallback_gamepad_socket = socket(AF_INET, SOCK_DGRAM, IPPROTO_UDP);
            if (s_fallback_gamepad_socket != INVALID_SOCKET) {
                u_long nonblocking = 1;
                ioctlsocket(s_fallback_gamepad_socket, FIONBIO, &nonblocking);
            }
        }
        send_sock = s_fallback_gamepad_socket;
    }

    if (send_sock != INVALID_SOCKET) {
        struct sockaddr_in target{};
        target.sin_family = AF_INET;
        target.sin_port = htons(GetConfig().relay_port); // 3659
        inet_pton(AF_INET, GetConfig().emulator_host.c_str(), &target.sin_addr);

        if (g_orig_sendto) {
            g_orig_sendto(send_sock, reinterpret_cast<const char*>(packet), 24, 0,
                          reinterpret_cast<const struct sockaddr*>(&target), sizeof(target));
        } else {
            sendto(send_sock, reinterpret_cast<const char*>(packet), 24, 0,
                   reinterpret_cast<const struct sockaddr*>(&target), sizeof(target));
        }
    }
}

// Hooked connect()
static int WSAAPI Hooked_connect(SOCKET s, const struct sockaddr *name, int namelen) {
    // Disable Nagle's algorithm for instant TCP packet dispatch
    int nodelay = 1;
    setsockopt(s, IPPROTO_TCP, TCP_NODELAY, reinterpret_cast<const char*>(&nodelay), sizeof(nodelay));

    struct sockaddr_in rewritten{};
    if (CheckAndRewriteSockAddr(name, &rewritten)) {
        return g_orig_connect(s, reinterpret_cast<const struct sockaddr*>(&rewritten), sizeof(rewritten));
    }
    return g_orig_connect(s, name, namelen);
}

// Hooked WSAConnect()
static int WSAAPI Hooked_WSAConnect(
    SOCKET s,
    const struct sockaddr *name,
    int namelen,
    LPWSABUF lpCallerData,
    LPWSABUF lpCalleeData,
    LPQOS lpSQOS,
    LPQOS lpGQOS
) {
    int nodelay = 1;
    setsockopt(s, IPPROTO_TCP, TCP_NODELAY, reinterpret_cast<const char*>(&nodelay), sizeof(nodelay));

    struct sockaddr_in rewritten{};
    if (CheckAndRewriteSockAddr(name, &rewritten)) {
        return g_orig_WSAConnect(s, reinterpret_cast<const struct sockaddr*>(&rewritten), sizeof(rewritten),
                                 lpCallerData, lpCalleeData, lpSQOS, lpGQOS);
    }
    return g_orig_WSAConnect(s, name, namelen, lpCallerData, lpCalleeData, lpSQOS, lpGQOS);
}

// Hooked bind() - Detect UDP game port 3659
static int WSAAPI Hooked_bind(SOCKET s, const struct sockaddr *name, int namelen) {
    if (name && name->sa_family == AF_INET) {
        const auto *sin = reinterpret_cast<const struct sockaddr_in*>(name);
        uint16_t port = ntohs(sin->sin_port);
        if (port == 3659 || (port >= 3658 && port <= 3665)) {
            g_game_socket = s;
            int bufsize = 65536;
            setsockopt(s, SOL_SOCKET, SO_RCVBUF, reinterpret_cast<const char*>(&bufsize), sizeof(bufsize));
            setsockopt(s, SOL_SOCKET, SO_SNDBUF, reinterpret_cast<const char*>(&bufsize), sizeof(bufsize));
            std::ostringstream ss;
            ss << "[WinSock] Detected FIFA 22 DirtySDK CommUDP/NetGameDist socket bind on port " << port
               << " (Configured 64KB low-latency socket buffers)";
            LogInfo(ss.str());
        }
    }
    return g_orig_bind(s, name, namelen);
}

// Hooked sendto() - Three-Tier Transport (P2P -> ProtoTunnel Relay -> Steam P2P)
static int WSAAPI Hooked_sendto(SOCKET s, const char *buf, int len, int flags, const struct sockaddr *to, int tolen) {
    bool is_game_udp = false;
    uint16_t dst_port = 0;
    if (to && to->sa_family == AF_INET) {
        const auto *sin = reinterpret_cast<const struct sockaddr_in*>(to);
        dst_port = ntohs(sin->sin_port);
        if (dst_port == 3659 || s == g_game_socket) {
            is_game_udp = true;
        }
    }

    if (!is_game_udp) {
        return g_orig_sendto(s, buf, len, flags, to, tolen);
    }

    uint64_t now = GetTickCount64();
    {
        std::lock_guard<std::mutex> lock(g_metrics_mutex);
        g_metrics.packets_sent++;
        g_last_send_time = now;

        // Transport Mode Auto-Selection
        if (GetConfig().force_relay) {
            g_metrics.mode = TransportMode::ProtoTunnelRelay;
        } else if (g_last_recv_time > 0 && (now - g_last_recv_time > 2500) && g_metrics.packets_sent > 10) {
            // Direct P2P probe timeout (> 2500ms without opponent packet) -> fallback to ProtoTunnel
            g_metrics.mode = TransportMode::ProtoTunnelRelay;
        } else if (SteamBridge::Instance().HasSteamNetworking() && g_target_steam_id != 0) {
            g_metrics.mode = TransportMode::SteamP2P;
        } else if (g_metrics.mode != TransportMode::ProtoTunnelRelay) {
            g_metrics.mode = TransportMode::DirectP2P;
        }
    }

    // Periodic sub-50ms ping probe transmission
    static uint64_t s_last_ping_probe = 0;
    if (now - s_last_ping_probe > 1000 && s != INVALID_SOCKET) {
        s_last_ping_probe = now;
        char ping_pkt[24];
        uint16_t session_id = htons(1);
        uint16_t peer_id = htons(0);
        uint16_t payload_len = htons(16);
        uint16_t proto_flags = htons(0x0000);
        std::memcpy(ping_pkt + 0, &session_id, 2);
        std::memcpy(ping_pkt + 2, &peer_id, 2);
        std::memcpy(ping_pkt + 4, &payload_len, 2);
        std::memcpy(ping_pkt + 6, &proto_flags, 2);

        uint16_t magic = 0xF220;
        std::memcpy(ping_pkt + 8, &magic, 2);
        ping_pkt[10] = 0x02; // PING_PROBE
        ping_pkt[11] = 0x00;
        std::memcpy(ping_pkt + 12, &now, 8); // 64-bit send timestamp

        struct sockaddr_in target{};
        target.sin_family = AF_INET;
        target.sin_port = htons(GetConfig().relay_port);
        inet_pton(AF_INET, GetConfig().emulator_host.c_str(), &target.sin_addr);

        g_orig_sendto(s, ping_pkt, 24, 0, reinterpret_cast<const struct sockaddr*>(&target), sizeof(target));
    }

    TransportMode active_mode;
    {
        std::lock_guard<std::mutex> lock(g_metrics_mutex);
        active_mode = g_metrics.mode;
    }

    // Mode 3: Steam P2P Relay (ISteamNetworking)
    if (active_mode == TransportMode::SteamP2P && g_target_steam_id != 0) {
        if (SteamBridge::Instance().SendP2PPacket(g_target_steam_id, buf, len, 1 /* unreliable no delay */, 0)) {
            return len;
        }
    }

    // Mode 2: ProtoTunnel UDP Relay Fallback (8-byte header to ServerIp:3659)
    if (active_mode == TransportMode::ProtoTunnelRelay) {
        char relay_buf[2048];
        if (len + 8 <= static_cast<int>(sizeof(relay_buf))) {
            uint16_t session_id = htons(1);
            uint16_t peer_id = htons(2);
            uint16_t payload_len = htons(static_cast<uint16_t>(len));
            uint16_t proto_flags = htons(0x0000); // FLAG_DATA

            std::memcpy(relay_buf + 0, &session_id, 2);
            std::memcpy(relay_buf + 2, &peer_id, 2);
            std::memcpy(relay_buf + 4, &payload_len, 2);
            std::memcpy(relay_buf + 6, &proto_flags, 2);
            std::memcpy(relay_buf + 8, buf, len);

            struct sockaddr_in relay_addr{};
            relay_addr.sin_family = AF_INET;
            relay_addr.sin_port = htons(GetConfig().relay_port); // 3659
            inet_pton(AF_INET, GetConfig().emulator_host.c_str(), &relay_addr.sin_addr);

            int sent = g_orig_sendto(s, relay_buf, len + 8, flags, reinterpret_cast<const struct sockaddr*>(&relay_addr), sizeof(relay_addr));
            if (sent > 8) return sent - 8;
            return (sent < 0) ? sent : 0;
        }
    }

    // Mode 1: Direct P2P UDP
    return g_orig_sendto(s, buf, len, flags, to, tolen);
}

// Hooked recvfrom() - Demultiplex ProtoTunnel & compute live latency
static int WSAAPI Hooked_recvfrom(SOCKET s, char *buf, int len, int flags, struct sockaddr *from, int *fromlen) {
    // Mode 3: Check if Steam P2P has datagram ready (Channel 0: gameplay, Channel 1: input)
    if (SteamBridge::Instance().HasSteamNetworking()) {
        uint32_t input_sz = 0;
        if (SteamBridge::Instance().IsP2PPacketAvailable(&input_sz, 1) && input_sz >= 16) {
            uint8_t p2p_buf[64];
            uint64_t remote_steam_id = 0;
            uint32_t read_bytes = 0;
            if (SteamBridge::Instance().ReadP2PPacket(p2p_buf, sizeof(p2p_buf), &read_bytes, &remote_steam_id, 1)) {
                if (read_bytes >= 16 && *reinterpret_cast<const uint16_t*>(p2p_buf) == 0xF220) {
                    if (p2p_buf[2] == 0x01) {
                        int slot = static_cast<int>(p2p_buf[3]);
                        if (slot <= 0 || slot >= 4) slot = 1;
                        XINPUT_GAMEPAD remote_pad{};
                        std::memcpy(&remote_pad, p2p_buf + 4, sizeof(XINPUT_GAMEPAD));
                        SetRemotePeerGamepad(slot, remote_pad);
                    }
                }
            }
        }

        uint32_t msg_sz = 0;
        if (SteamBridge::Instance().IsP2PPacketAvailable(&msg_sz, 0) && msg_sz > 0) {
            uint64_t remote_steam_id = 0;
            uint32_t read_bytes = 0;
            if (SteamBridge::Instance().ReadP2PPacket(buf, len, &read_bytes, &remote_steam_id, 0)) {
                std::lock_guard<std::mutex> lock(g_metrics_mutex);
                g_metrics.packets_recv++;
                g_last_recv_time = GetTickCount64();
                return static_cast<int>(read_bytes);
            }
        }
    }

    int bytes = g_orig_recvfrom(s, buf, len, flags, from, fromlen);
    if (bytes <= 0) return bytes;

    bool is_game_udp = (s == g_game_socket);
    if (from && from->sa_family == AF_INET) {
        const auto *sin = reinterpret_cast<const struct sockaddr_in*>(from);
        uint16_t port = ntohs(sin->sin_port);
        if (port == 3659 || port == GetConfig().relay_port) {
            is_game_udp = true;
        }
    }

    if (is_game_udp) {
        uint64_t now = GetTickCount64();
        std::lock_guard<std::mutex> lock(g_metrics_mutex);
        g_metrics.packets_recv++;

        // Demultiplex internal 0xF220 stream (inputs, ping/pong probes)
        const char* data_ptr = buf;
        int data_len = bytes;
        if (bytes >= 24 && *reinterpret_cast<const uint16_t*>(buf + 8) == 0xF220) {
            data_ptr = buf + 8;
            data_len = bytes - 8;
        }

        if (data_len >= 16 && *reinterpret_cast<const uint16_t*>(data_ptr) == 0xF220) {
            uint8_t pkt_type = static_cast<uint8_t>(data_ptr[2]);
            if (pkt_type == 0x01 && data_len >= 16) {
                // Input frame from remote friend
                int slot = static_cast<int>(data_ptr[3]);
                if (slot <= 0 || slot >= 4) slot = 1;
                XINPUT_GAMEPAD remote_pad{};
                std::memcpy(&remote_pad, data_ptr + 4, sizeof(XINPUT_GAMEPAD));
                SetRemotePeerGamepad(slot, remote_pad);
                return 0; // Filter out from DirtySDK
            } else if (pkt_type == 0x02 && data_len >= 16) {
                // Ping probe -> Reply Pong (0x03)
                char pong[24];
                uint16_t session_id = htons(1);
                uint16_t peer_id = htons(0);
                uint16_t payload_len = htons(16);
                uint16_t proto_flags = htons(0x0000);
                std::memcpy(pong + 0, &session_id, 2);
                std::memcpy(pong + 2, &peer_id, 2);
                std::memcpy(pong + 4, &payload_len, 2);
                std::memcpy(pong + 6, &proto_flags, 2);

                uint16_t magic = 0xF220;
                std::memcpy(pong + 8, &magic, 2);
                pong[10] = 0x03; // PONG
                pong[11] = 0x00;
                std::memcpy(pong + 12, data_ptr + 4, 8); // Echo back timestamp

                if (from) {
                    g_orig_sendto(s, pong, 24, 0, from, *fromlen);
                }
                return 0;
            } else if (pkt_type == 0x03 && data_len >= 16) {
                // Pong received! Compute accurate RTT!
                uint64_t sent_ts = 0;
                std::memcpy(&sent_ts, data_ptr + 4, 8);
                if (now >= sent_ts) {
                    uint32_t rtt = static_cast<uint32_t>(now - sent_ts);
                    if (rtt < 500) {
                        g_metrics.ping_ms = (g_metrics.ping_ms * 3 + rtt) / 4;
                        uint32_t diff = (rtt > g_metrics.ping_ms) ? (rtt - g_metrics.ping_ms) : (g_metrics.ping_ms - rtt);
                        g_metrics.jitter_ms = (g_metrics.jitter_ms * 3 + diff) / 4;
                    }
                }
                return 0;
            }
        }

        // Mode 2: Strip ProtoTunnel 8-byte header if relayed
        if (g_metrics.mode == TransportMode::ProtoTunnelRelay && bytes >= 8) {
            uint16_t payload_len = ntohs(*reinterpret_cast<const uint16_t*>(buf + 4));
            if (payload_len <= static_cast<uint16_t>(bytes - 8)) {
                std::memmove(buf, buf + 8, bytes - 8);
                bytes -= 8;
            }
        }

        // Live RTT and Jitter calculation (only for continuous frame exchange <= 100ms)
        if (g_last_send_time > 0 && now >= g_last_send_time) {
            uint32_t sample = static_cast<uint32_t>(now - g_last_send_time);
            if (sample <= 100) { // Reject idle intervals so ping does not falsely blow up to 400ms!
                g_metrics.ping_ms = (g_metrics.ping_ms * 7 + sample) / 8;
                uint32_t diff = (sample > g_metrics.ping_ms) ? (sample - g_metrics.ping_ms) : (g_metrics.ping_ms - sample);
                g_metrics.jitter_ms = (g_metrics.jitter_ms * 7 + diff) / 8;
            }
        }
        g_last_recv_time = now;
        g_metrics.sync_ok = true;
    }

    return bytes;
}

// Hooked WSASendTo()
static int WSAAPI Hooked_WSASendTo(
    SOCKET s,
    LPWSABUF lpBuffers,
    DWORD dwBufferCount,
    LPDWORD lpNumberOfBytesSent,
    DWORD dwFlags,
    const struct sockaddr *lpTo,
    int iTolen,
    LPWSAOVERLAPPED lpOverlapped,
    LPWSAOVERLAPPED_COMPLETION_ROUTINE lpCompletionRoutine
) {
    if (dwBufferCount == 1 && lpBuffers && !lpOverlapped) {
        int res = Hooked_sendto(s, lpBuffers[0].buf, lpBuffers[0].len, dwFlags, lpTo, iTolen);
        if (res >= 0) {
            if (lpNumberOfBytesSent) *lpNumberOfBytesSent = static_cast<DWORD>(res);
            return 0;
        }
        return SOCKET_ERROR;
    }
    return g_orig_WSASendTo(s, lpBuffers, dwBufferCount, lpNumberOfBytesSent, dwFlags, lpTo, iTolen, lpOverlapped, lpCompletionRoutine);
}

// Hooked WSARecvFrom()
static int WSAAPI Hooked_WSARecvFrom(
    SOCKET s,
    LPWSABUF lpBuffers,
    DWORD dwBufferCount,
    LPDWORD lpNumberOfBytesRecvd,
    LPDWORD lpFlags,
    struct sockaddr *lpFrom,
    LPINT lpFromlen,
    LPWSAOVERLAPPED lpOverlapped,
    LPWSAOVERLAPPED_COMPLETION_ROUTINE lpCompletionRoutine
) {
    if (dwBufferCount == 1 && lpBuffers && !lpOverlapped) {
        int flags = lpFlags ? *lpFlags : 0;
        int res = Hooked_recvfrom(s, lpBuffers[0].buf, lpBuffers[0].len, flags, lpFrom, lpFromlen);
        if (res >= 0) {
            if (lpNumberOfBytesRecvd) *lpNumberOfBytesRecvd = static_cast<DWORD>(res);
            return 0;
        }
        return SOCKET_ERROR;
    }
    return g_orig_WSARecvFrom(s, lpBuffers, dwBufferCount, lpNumberOfBytesRecvd, lpFlags, lpFrom, lpFromlen, lpOverlapped, lpCompletionRoutine);
}

bool InstallWinsockHooks() {
    HMODULE ws2 = GetModuleHandleW(L"ws2_32.dll");
    if (!ws2) {
        ws2 = LoadLibraryW(L"ws2_32.dll");
    }
    if (!ws2) {
        LogError("Could not acquire ws2_32.dll handle");
        return false;
    }

    void *p_getaddrinfo = reinterpret_cast<void*>(GetProcAddress(ws2, "getaddrinfo"));
    void *p_GetAddrInfoW = reinterpret_cast<void*>(GetProcAddress(ws2, "GetAddrInfoW"));
    void *p_connect = reinterpret_cast<void*>(GetProcAddress(ws2, "connect"));
    void *p_WSAConnect = reinterpret_cast<void*>(GetProcAddress(ws2, "WSAConnect"));

    void *p_bind = reinterpret_cast<void*>(GetProcAddress(ws2, "bind"));
    void *p_sendto = reinterpret_cast<void*>(GetProcAddress(ws2, "sendto"));
    void *p_WSASendTo = reinterpret_cast<void*>(GetProcAddress(ws2, "WSASendTo"));
    void *p_recvfrom = reinterpret_cast<void*>(GetProcAddress(ws2, "recvfrom"));
    void *p_WSARecvFrom = reinterpret_cast<void*>(GetProcAddress(ws2, "WSARecvFrom"));

    auto create_hook_checked = [](void* target, void* detour, void** original, const char* name) {
        if (!target) {
            LogWarn(std::string("WinSock export not found: ") + name);
            return;
        }
        MH_STATUS s = MH_CreateHook(target, detour, original);
        if (s != MH_OK) {
            LogError(std::string("Failed to hook WinSock ") + name + ": " + MH_StatusToString(s));
        }
    };

    create_hook_checked(p_getaddrinfo, reinterpret_cast<void*>(&Hooked_getaddrinfo),
                        reinterpret_cast<void**>(&g_orig_getaddrinfo), "getaddrinfo");
    create_hook_checked(p_GetAddrInfoW, reinterpret_cast<void*>(&Hooked_GetAddrInfoW),
                        reinterpret_cast<void**>(&g_orig_GetAddrInfoW), "GetAddrInfoW");
    create_hook_checked(p_connect, reinterpret_cast<void*>(&Hooked_connect),
                        reinterpret_cast<void**>(&g_orig_connect), "connect");
    create_hook_checked(p_WSAConnect, reinterpret_cast<void*>(&Hooked_WSAConnect),
                        reinterpret_cast<void**>(&g_orig_WSAConnect), "WSAConnect");

    create_hook_checked(p_bind, reinterpret_cast<void*>(&Hooked_bind),
                        reinterpret_cast<void**>(&g_orig_bind), "bind");
    create_hook_checked(p_sendto, reinterpret_cast<void*>(&Hooked_sendto),
                        reinterpret_cast<void**>(&g_orig_sendto), "sendto");
    create_hook_checked(p_WSASendTo, reinterpret_cast<void*>(&Hooked_WSASendTo),
                        reinterpret_cast<void**>(&g_orig_WSASendTo), "WSASendTo");
    create_hook_checked(p_recvfrom, reinterpret_cast<void*>(&Hooked_recvfrom),
                        reinterpret_cast<void**>(&g_orig_recvfrom), "recvfrom");
    create_hook_checked(p_WSARecvFrom, reinterpret_cast<void*>(&Hooked_WSARecvFrom),
                        reinterpret_cast<void**>(&g_orig_WSARecvFrom), "WSARecvFrom");

    return true;
}

} // namespace f22
