#include "f22_shim.h"
#include <MinHook.h>
#include <sstream>
#include <cstring>
#include <string>

namespace f22 {

// Original function pointers
using PFN_getaddrinfo = int(WSAAPI*)(const char *pNodeName, const char *pServiceName, const ADDRINFOA *pHints, PADDRINFOA *ppResult);
using PFN_GetAddrInfoW = int(WSAAPI*)(const PCWSTR pNodeName, const PCWSTR pServiceName, const ADDRINFOW *pHints, PADDRINFOW *ppResult);
using PFN_connect = int(WSAAPI*)(SOCKET s, const struct sockaddr *name, int namelen);
using PFN_WSAConnect = int(WSAAPI*)(SOCKET s, const struct sockaddr *name, int namelen, LPWSABUF lpCallerData, LPWSABUF lpCalleeData, LPQOS lpSQOS, LPQOS lpGQOS);

static PFN_getaddrinfo g_orig_getaddrinfo = nullptr;
static PFN_GetAddrInfoW g_orig_GetAddrInfoW = nullptr;
static PFN_connect g_orig_connect = nullptr;
static PFN_WSAConnect g_orig_WSAConnect = nullptr;

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

// Hooked connect()
static int WSAAPI Hooked_connect(SOCKET s, const struct sockaddr *name, int namelen) {
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
    struct sockaddr_in rewritten{};
    if (CheckAndRewriteSockAddr(name, &rewritten)) {
        return g_orig_WSAConnect(s, reinterpret_cast<const struct sockaddr*>(&rewritten), sizeof(rewritten),
                                 lpCallerData, lpCalleeData, lpSQOS, lpGQOS);
    }
    return g_orig_WSAConnect(s, name, namelen, lpCallerData, lpCalleeData, lpSQOS, lpGQOS);
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

    if (p_getaddrinfo) {
        MH_CreateHook(p_getaddrinfo, reinterpret_cast<void*>(&Hooked_getaddrinfo),
                      reinterpret_cast<void**>(&g_orig_getaddrinfo));
    }
    if (p_GetAddrInfoW) {
        MH_CreateHook(p_GetAddrInfoW, reinterpret_cast<void*>(&Hooked_GetAddrInfoW),
                      reinterpret_cast<void**>(&g_orig_GetAddrInfoW));
    }
    if (p_connect) {
        MH_CreateHook(p_connect, reinterpret_cast<void*>(&Hooked_connect),
                      reinterpret_cast<void**>(&g_orig_connect));
    }
    if (p_WSAConnect) {
        MH_CreateHook(p_WSAConnect, reinterpret_cast<void*>(&Hooked_WSAConnect),
                      reinterpret_cast<void**>(&g_orig_WSAConnect));
    }

    return true;
}

} // namespace f22
