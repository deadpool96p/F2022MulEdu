#include "f22_shim.h"
#include "hook_guard.h"
#include <winhttp.h>
#include <wininet.h>
#include <MinHook.h>
#include <string>
#include <vector>

#pragma comment(lib, "winhttp.lib")
#pragma comment(lib, "wininet.lib")

namespace f22 {

// WinHTTP Option Indices
const DWORD WINHTTP_OPTION_SECURITY_FLAGS = 31;
const DWORD SECURITY_FLAG_IGNORE_UNKNOWN_CA = 0x00000100;
const DWORD SECURITY_FLAG_IGNORE_CERT_CN_INVALID = 0x00001000;
const DWORD SECURITY_FLAG_IGNORE_CERT_DATE_INVALID = 0x00002000;

// WinINet Option Indices
const DWORD INTERNET_OPTION_SECURITY_FLAGS = 31; // Same as WinHTTP usually

typedef BOOL (WINAPI *PFN_WinHttpSetOption)(HINTERNET, DWORD, LPVOID, DWORD);
static PFN_WinHttpSetOption g_orig_WinHttpSetOption = nullptr;

typedef BOOL (WINAPI *PFN_InternetSetOptionA)(HINTERNET, DWORD, LPVOID, DWORD);
static PFN_InternetSetOptionA g_orig_InternetSetOptionA = nullptr;

typedef BOOL (WINAPI *PFN_InternetSetOptionW)(HINTERNET, DWORD, LPVOID, DWORD);
static PFN_InternetSetOptionW g_orig_InternetSetOptionW = nullptr;

static BOOL WINAPI Hooked_WinHttpSetOption(HINTERNET hInternet, DWORD dwOption, LPVOID lpValue, DWORD dwBufferLength) {
    if (dwOption == WINHTTP_OPTION_SECURITY_FLAGS && lpValue) {
        DWORD flags = *static_cast<DWORD*>(lpValue);
        DWORD original_flags = flags;
        flags |= (SECURITY_FLAG_IGNORE_UNKNOWN_CA | SECURITY_FLAG_IGNORE_CERT_CN_INVALID | SECURITY_FLAG_IGNORE_CERT_DATE_INVALID);

        if (flags != original_flags) {
            *static_cast<DWORD*>(lpValue) = flags;
            LogInfo("[WinHTTP Bypass] Overrode SECURITY_FLAGS: 0x" + std::to_string(original_flags) + " -> 0x" + std::to_string(flags));
        }
    }
    return g_orig_WinHttpSetOption(hInternet, dwOption, lpValue, dwBufferLength);
}

static BOOL WINAPI Hooked_InternetSetOptionA(HINTERNET hInternet, DWORD dwOption, LPVOID lpValue, DWORD dwBufferLength) {
    if (dwOption == INTERNET_OPTION_SECURITY_FLAGS && lpValue) {
        DWORD flags = *static_cast<DWORD*>(lpValue);
        *static_cast<DWORD*>(lpValue) |= (SECURITY_FLAG_IGNORE_UNKNOWN_CA | SECURITY_FLAG_IGNORE_CERT_CN_INVALID | SECURITY_FLAG_IGNORE_CERT_DATE_INVALID);
        LogInfo("[WinINet Bypass] Overrode INTERNET_OPTION_SECURITY_FLAGS (A)");
    }
    return g_orig_InternetSetOptionA(hInternet, dwOption, lpValue, dwBufferLength);
}

static BOOL WINAPI Hooked_InternetSetOptionW(HINTERNET hInternet, DWORD dwOption, LPVOID lpValue, DWORD dwBufferLength) {
    if (dwOption == INTERNET_OPTION_SECURITY_FLAGS && lpValue) {
        DWORD flags = *static_cast<DWORD*>(lpValue);
        *static_cast<DWORD*>(lpValue) |= (SECURITY_FLAG_IGNORE_UNKNOWN_CA | SECURITY_FLAG_IGNORE_CERT_CN_INVALID | SECURITY_FLAG_IGNORE_CERT_DATE_INVALID);
        LogInfo("[WinINet Bypass] Overrode INTERNET_OPTION_SECURITY_FLAGS (W)");
    }
    return g_orig_InternetSetOptionW(hInternet, dwOption, lpValue, dwBufferLength);
}

bool InstallWinHttpHooks() {
    static std::vector<std::unique_ptr<HookGuard>> s_winhttp_guards;

    auto create_hook = [](void* target, void* detour, void** original, const char* name) {
        if (!target) return;
        s_winhttp_guards.push_back(std::make_unique<HookGuard>("winhttp/wininet", name, target, detour, original));
    };

    HMODULE hWinHttp = GetModuleHandleW(L"winhttp.dll");
    if (hWinHttp) {
        create_hook(
            (void*)GetProcAddress(hWinHttp, "WinHttpSetOption"),
            (void*)&Hooked_WinHttpSetOption,
            (void**)&g_orig_WinHttpSetOption,
            "WinHttpSetOption"
        );
    }

    HMODULE hWinInet = GetModuleHandleW(L"wininet.dll");
    if (hWinInet) {
        create_hook(
            (void*)GetProcAddress(hWinInet, "InternetSetOptionA"),
            (void*)&Hooked_InternetSetOptionA,
            (void**)&g_orig_InternetSetOptionA,
            "InternetSetOptionA"
        );
        create_hook(
            (void*)GetProcAddress(hWinInet, "InternetSetOptionW"),
            (void*)&Hooked_InternetSetOptionW,
            (void**)&g_orig_InternetSetOptionW,
            "InternetSetOptionW"
        );
    }

    return true;
}

} // namespace f22
