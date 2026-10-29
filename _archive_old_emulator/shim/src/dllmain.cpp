#include "f22_shim.h"
#include "control_client.h"
#include "hooks_dxgi.h"
#include "steam_bridge.hpp"
#include <MinHook.h>
#include <dxgi.h>
#include <dxgi1_2.h>
#include <dxgi1_3.h>
#include <dxgi1_6.h>
#include <algorithm>

namespace f22 {

// Real DXGI function pointers
using PFN_CreateDXGIFactory = HRESULT(WINAPI*)(REFIID riid, void **ppFactory);
using PFN_CreateDXGIFactory1 = HRESULT(WINAPI*)(REFIID riid, void **ppFactory);
using PFN_CreateDXGIFactory2 = HRESULT(WINAPI*)(UINT Flags, REFIID riid, void **ppFactory);
using PFN_DXGIDeclareAdapterRemovalSupport = HRESULT(WINAPI*)(void);
using PFN_DXGIGetDebugInterface1 = HRESULT(WINAPI*)(UINT Flags, REFIID riid, void **pDebug);
using PFN_DXGIReportActiveObjects = HRESULT(WINAPI*)(void);
using PFN_DXGID3D10CreateDevice = HRESULT(WINAPI*)(void*, void*, void*, void*, void*, void*);
using PFN_DXGID3D10RegisterLayers = HRESULT(WINAPI*)(const void*, UINT);

static HMODULE g_real_dxgi = nullptr;
static PFN_CreateDXGIFactory g_pfn_CreateDXGIFactory = nullptr;
static PFN_CreateDXGIFactory1 g_pfn_CreateDXGIFactory1 = nullptr;
static PFN_CreateDXGIFactory2 g_pfn_CreateDXGIFactory2 = nullptr;
static PFN_DXGIDeclareAdapterRemovalSupport g_pfn_DXGIDeclareAdapterRemovalSupport = nullptr;
static PFN_DXGIGetDebugInterface1 g_pfn_DXGIGetDebugInterface1 = nullptr;
static PFN_DXGIReportActiveObjects g_pfn_DXGIReportActiveObjects = nullptr;
static PFN_DXGID3D10CreateDevice g_pfn_DXGID3D10CreateDevice = nullptr;
static PFN_DXGID3D10RegisterLayers g_pfn_DXGID3D10RegisterLayers = nullptr;

#include <wincrypt.h>
#include <vector>

constexpr const char* TARGET_FIFA_HASH_HEX = "FDDC0B5B4A093C460A5D0E07E117634366E2DA0050F19EED3776C4340A56471A";
constexpr const char* TARGET_CARDSDLL_HASH_HEX = "D6711E3A227A5EAAC3285971C44F0F583B90B42B5C2749A1C95F2E479F7164C5";
constexpr const char* SHIM_VERSION_STRING = "F2022MulEdu Shim Proxy v0.1.0-alpha (Target: FIFA 22 1.0.77.45722)";

static bool ComputeSha256File(const char* filepath, std::string& out_hex) {
    HANDLE hFile = CreateFileA(filepath, GENERIC_READ, FILE_SHARE_READ, nullptr, OPEN_EXISTING, FILE_FLAG_SEQUENTIAL_SCAN, nullptr);
    if (hFile == INVALID_HANDLE_VALUE) {
        return false;
    }

    HCRYPTPROV hProv = 0;
    HCRYPTHASH hHash = 0;
    if (!CryptAcquireContext(&hProv, nullptr, nullptr, PROV_RSA_AES, CRYPT_VERIFYCONTEXT)) {
        CloseHandle(hFile);
        return false;
    }

    if (!CryptCreateHash(hProv, CALG_SHA_256, 0, 0, &hHash)) {
        CryptReleaseContext(hProv, 0);
        CloseHandle(hFile);
        return false;
    }

    constexpr DWORD BUF_SIZE = 65536;
    std::vector<BYTE> buffer(BUF_SIZE);
    DWORD bytesRead = 0;
    while (ReadFile(hFile, buffer.data(), BUF_SIZE, &bytesRead, nullptr) && bytesRead > 0) {
        CryptHashData(hHash, buffer.data(), bytesRead, 0);
    }

    CloseHandle(hFile);

    BYTE hash_bytes[32] = {0};
    DWORD hash_len = sizeof(hash_bytes);
    bool ok = CryptGetHashParam(hHash, HP_HASHVAL, hash_bytes, &hash_len, 0);
    CryptDestroyHash(hHash);
    CryptReleaseContext(hProv, 0);

    if (!ok || hash_len != 32) {
        return false;
    }

    char hex[65] = {0};
    for (int i = 0; i < 32; ++i) {
        snprintf(hex + i * 2, 3, "%02X", hash_bytes[i]);
    }
    out_hex = hex;
    return true;
}

bool RunStartupSelfCheck() {
    char local_appdata[MAX_PATH] = {0};
    if (GetEnvironmentVariableA("LOCALAPPDATA", local_appdata, MAX_PATH) == 0) {
        strncpy(local_appdata, "C:\\Temp", MAX_PATH - 1);
    }
    std::string err_dir = std::string(local_appdata) + "\\F2022MulEdu";
    CreateDirectoryA(err_dir.c_str(), nullptr);
    std::string err_log_path = err_dir + "\\shim.log";

    auto write_to_shim_log = [&](const std::string& line) {
        FILE* f = fopen(err_log_path.c_str(), "a");
        if (f) {
            fprintf(f, "%s\n", line.c_str());
            fclose(f);
        }
    };

    // 0. Log Shim Version
    std::string ver_line = std::string("[INFO] ") + SHIM_VERSION_STRING;
    write_to_shim_log(ver_line);
    LogInfo(ver_line);

    bool all_ok = true;

    // 1. Verify real DXGI is loaded (either dxgi_org.dll or System32\dxgi.dll)
    char dll_dir[MAX_PATH] = {0};
    GetModuleFileNameA(nullptr, dll_dir, MAX_PATH);
    char* last_slash = strrchr(dll_dir, '\\');
    std::string base_dir = ".";
    if (last_slash) {
        *last_slash = '\0';
        base_dir = dll_dir;
    }

    if (g_real_dxgi != nullptr) {
        std::string ok_msg = "[INFO] Startup self-check: Real DXGI backend verified and loaded.";
        write_to_shim_log(ok_msg);
        LogInfo(ok_msg);
    } else {
        std::string err = "[ERROR] Startup self-check: Real DXGI backend handle is null!";
        write_to_shim_log(err);
        LogError(err);
        all_ok = false;
    }

    // 2. Verify loaded FIFA executable hash matches target build (1.0.77.45722)
    char exe_path[MAX_PATH] = {0};
    GetModuleFileNameA(nullptr, exe_path, MAX_PATH);

    std::string actual_hash;
    if (ComputeSha256File(exe_path, actual_hash)) {
        std::string hash_log = "[INFO] Loaded Executable (" + std::string(exe_path) + ") SHA-256: " + actual_hash;
        write_to_shim_log(hash_log);
        LogInfo(hash_log);

        std::string exe_str = exe_path;
        std::transform(exe_str.begin(), exe_str.end(), exe_str.begin(), [](unsigned char c) { return (char)tolower(c); });

        if (exe_str.find("fifa") != std::string::npos) {
            if (_stricmp(actual_hash.c_str(), TARGET_FIFA_HASH_HEX) != 0) {
                std::string err = "[ERROR] Startup self-check: Executable hash mismatch for " + std::string(exe_path) +
                                  "! Expected (1.0.77.45722): " + TARGET_FIFA_HASH_HEX +
                                  ", Got: " + actual_hash;
                write_to_shim_log(err);
                LogError(err);
                all_ok = false;
            } else {
                std::string ok_msg = "[INFO] Startup self-check: FIFA 22 (build 1.0.77.45722) hash verified: " + actual_hash;
                write_to_shim_log(ok_msg);
                LogInfo(ok_msg);
            }
        }
    } else {
        std::string err = "[ERROR] Startup self-check: Failed to compute SHA-256 hash for loaded executable: " + std::string(exe_path);
        write_to_shim_log(err);
        LogError(err);
        all_ok = false;
    }

    // 3. Check and log CardsDLL_Win64_retail.dll hash if present
    std::string cards_dll_path = base_dir + "\\CardsDLL_Win64_retail.dll";
    DWORD cards_attr = GetFileAttributesA(cards_dll_path.c_str());
    if (cards_attr != INVALID_FILE_ATTRIBUTES) {
        std::string cards_hash;
        if (ComputeSha256File(cards_dll_path.c_str(), cards_hash)) {
            std::string cards_msg = "[INFO] CardsDLL_Win64_retail.dll SHA-256: " + cards_hash;
            write_to_shim_log(cards_msg);
            LogInfo(cards_msg);
            if (_stricmp(cards_hash.c_str(), TARGET_CARDSDLL_HASH_HEX) == 0) {
                std::string match_msg = "[INFO] Startup self-check: CardsDLL_Win64_retail.dll verified: " + cards_hash;
                write_to_shim_log(match_msg);
                LogInfo(match_msg);
            }
        }
    } else {
        std::string cards_note = "[INFO] CardsDLL_Win64_retail.dll not found at: " + cards_dll_path + " (non-cards invocation)";
        write_to_shim_log(cards_note);
        LogInfo(cards_note);
    }

    return all_ok;
}

bool InitDXGIProxy() {
    if (g_real_dxgi) {
        return true;
    }

    // Try dxgi_org.dll in game root first
    g_real_dxgi = LoadLibraryW(L"dxgi_org.dll");
    if (g_real_dxgi) {
        LogInfo("Loaded real dxgi_org.dll from game directory successfully.");
    } else {
        wchar_t sys_path[MAX_PATH];
        if (GetSystemDirectoryW(sys_path, MAX_PATH) == 0) {
            LogError("Failed to get system directory for dxgi.dll");
            return false;
        }

        std::wstring real_dxgi_path = std::wstring(sys_path) + L"\\dxgi.dll";
        g_real_dxgi = LoadLibraryW(real_dxgi_path.c_str());
        if (!g_real_dxgi) {
            LogError("Failed to load real System32 dxgi.dll");
            return false;
        }
        LogInfo("Loaded real System32 dxgi.dll successfully.");
    }

    g_pfn_CreateDXGIFactory = reinterpret_cast<PFN_CreateDXGIFactory>(
        GetProcAddress(g_real_dxgi, "CreateDXGIFactory"));
    g_pfn_CreateDXGIFactory1 = reinterpret_cast<PFN_CreateDXGIFactory1>(
        GetProcAddress(g_real_dxgi, "CreateDXGIFactory1"));
    g_pfn_CreateDXGIFactory2 = reinterpret_cast<PFN_CreateDXGIFactory2>(
        GetProcAddress(g_real_dxgi, "CreateDXGIFactory2"));
    g_pfn_DXGIDeclareAdapterRemovalSupport = reinterpret_cast<PFN_DXGIDeclareAdapterRemovalSupport>(
        GetProcAddress(g_real_dxgi, "DXGIDeclareAdapterRemovalSupport"));
    g_pfn_DXGIGetDebugInterface1 = reinterpret_cast<PFN_DXGIGetDebugInterface1>(
        GetProcAddress(g_real_dxgi, "DXGIGetDebugInterface1"));
    g_pfn_DXGIReportActiveObjects = reinterpret_cast<PFN_DXGIReportActiveObjects>(
        GetProcAddress(g_real_dxgi, "DXGIReportActiveObjects"));
    g_pfn_DXGID3D10CreateDevice = reinterpret_cast<PFN_DXGID3D10CreateDevice>(
        GetProcAddress(g_real_dxgi, "DXGID3D10CreateDevice"));
    g_pfn_DXGID3D10RegisterLayers = reinterpret_cast<PFN_DXGID3D10RegisterLayers>(
        GetProcAddress(g_real_dxgi, "DXGID3D10RegisterLayers"));

    return true;
}

static DWORD WINAPI ShimWorkerThread(LPVOID) {
    // Small delay to ensure host process runtime and CRT are fully initialized
    Sleep(200);

    LogInfo("=============================================================");
    LogInfo(" FIFA 22 Multiplayer Emulator Shim (dxgi.dll proxy) Active   ");
    LogInfo("=============================================================");

    LoadConfig();
    if (!RunStartupSelfCheck()) {
        std::string err = "[FATAL] Startup self-check failed! Refusing to install hooks to protect target stability.";
        LogError(err);
        return 1;
    }

    if (MH_Initialize() != MH_OK) {
        LogError("MinHook initialization failed!");
        return 1;
    }
    LogInfo("MinHook initialized successfully.");

    if (InstallWinsockHooks()) {
        LogInfo("WinSock network redirection hooks installed.");
    } else {
        LogError("Failed to install WinSock hooks.");
    }

    if (InstallCertBypassHooks()) {
        LogInfo("SSL/TLS certificate verification bypass hooks installed.");
    } else {
        LogWarn("Some SSL/TLS bypass hooks could not be installed.");
    }

    if (GetConfig().enable_overlay) {
        if (InstallDXGIHooks()) {
            LogInfo("DXGI Present & ResizeBuffers overlay hooks installed.");
        } else {
            LogWarn("Failed to install DXGI overlay hooks.");
        }
    }

    ControlClient::Instance().Start(GetConfig().emulator_host, 42129);
    LogInfo("Control client connected to emulator control API on port 42129.");

    if (GetConfig().enable_steam) {
        SteamBridge::Instance().Initialize(GetConfig().steam_appid);
    }

    if (MH_EnableHook(MH_ALL_HOOKS) != MH_OK) {
        LogError("Failed to enable MinHook hooks!");
        return 1;
    }

    LogInfo("All shim hooks activated. Traffic rerouting is live.");
    return 0;
}

} // namespace f22

// Exported DXGI Forwarders
extern "C" {

HRESULT WINAPI CreateDXGIFactory(REFIID riid, void **ppFactory) {
    if (!f22::g_pfn_CreateDXGIFactory) {
        f22::InitDXGIProxy();
    }
    if (f22::g_pfn_CreateDXGIFactory) {
        return f22::g_pfn_CreateDXGIFactory(riid, ppFactory);
    }
    return E_FAIL;
}

HRESULT WINAPI CreateDXGIFactory1(REFIID riid, void **ppFactory) {
    if (!f22::g_pfn_CreateDXGIFactory1) {
        f22::InitDXGIProxy();
    }
    if (f22::g_pfn_CreateDXGIFactory1) {
        return f22::g_pfn_CreateDXGIFactory1(riid, ppFactory);
    }
    return E_FAIL;
}

HRESULT WINAPI CreateDXGIFactory2(UINT Flags, REFIID riid, void **ppFactory) {
    if (!f22::g_pfn_CreateDXGIFactory2) {
        f22::InitDXGIProxy();
    }
    if (f22::g_pfn_CreateDXGIFactory2) {
        return f22::g_pfn_CreateDXGIFactory2(Flags, riid, ppFactory);
    }
    return E_FAIL;
}

HRESULT WINAPI DXGIDeclareAdapterRemovalSupport(void) {
    if (!f22::g_pfn_DXGIDeclareAdapterRemovalSupport) {
        f22::InitDXGIProxy();
    }
    if (f22::g_pfn_DXGIDeclareAdapterRemovalSupport) {
        return f22::g_pfn_DXGIDeclareAdapterRemovalSupport();
    }
    return E_FAIL;
}

HRESULT WINAPI DXGIGetDebugInterface1(UINT Flags, REFIID riid, void **pDebug) {
    if (!f22::g_pfn_DXGIGetDebugInterface1) {
        f22::InitDXGIProxy();
    }
    if (f22::g_pfn_DXGIGetDebugInterface1) {
        return f22::g_pfn_DXGIGetDebugInterface1(Flags, riid, pDebug);
    }
    return E_FAIL;
}

HRESULT WINAPI DXGIReportActiveObjects(void) {
    if (!f22::g_pfn_DXGIReportActiveObjects) {
        f22::InitDXGIProxy();
    }
    if (f22::g_pfn_DXGIReportActiveObjects) {
        return f22::g_pfn_DXGIReportActiveObjects();
    }
    return S_OK;
}

HRESULT WINAPI DXGID3D10CreateDevice(void* p1, void* p2, void* p3, void* p4, void* p5, void* p6) {
    if (!f22::g_pfn_DXGID3D10CreateDevice) {
        f22::InitDXGIProxy();
    }
    if (f22::g_pfn_DXGID3D10CreateDevice) {
        return f22::g_pfn_DXGID3D10CreateDevice(p1, p2, p3, p4, p5, p6);
    }
    return E_FAIL;
}

HRESULT WINAPI DXGID3D10RegisterLayers(const void* pLayers, UINT NumLayers) {
    if (!f22::g_pfn_DXGID3D10RegisterLayers) {
        f22::InitDXGIProxy();
    }
    if (f22::g_pfn_DXGID3D10RegisterLayers) {
        return f22::g_pfn_DXGID3D10RegisterLayers(pLayers, NumLayers);
    }
    return S_OK;
}

BOOL WINAPI DllMain(HINSTANCE hinstDLL, DWORD fdwReason, LPVOID lpvReserved) {
    switch (fdwReason) {
    case DLL_PROCESS_ATTACH: {
        DisableThreadLibraryCalls(hinstDLL);

        // Step 1: Early Diagnostic File Logging before any other initialization
        char dll_path[MAX_PATH] = {0};
        char log_path[MAX_PATH] = {0};
        if (GetModuleFileNameA(hinstDLL, dll_path, MAX_PATH) > 0) {
            char* last_slash = strrchr(dll_path, '\\');
            if (last_slash) {
                *(last_slash + 1) = '\0';
                snprintf(log_path, sizeof(log_path), "%sf22_shim.log", dll_path);
                f22::SetLogDirectory(dll_path);
            }
        }

        FILE* f = fopen(log_path[0] ? log_path : "f22_shim.log", "w");
        if (!f) {
            f = fopen("f22_shim.log", "w");
        }
        if (!f) {
            CreateDirectoryA("C:\\Temp", nullptr);
            f = fopen("C:\\Temp\\f22_shim.log", "w");
        }
        if (f) {
            fprintf(f, "[+] dxgi.dll DllMain DLL_PROCESS_ATTACH invoked successfully\n");
            fprintf(f, "[+] Process ID: %lu, Thread ID: %lu\n", GetCurrentProcessId(), GetCurrentThreadId());
            fprintf(f, "[+] Module directory: %s\n", dll_path[0] ? dll_path : "<unknown>");
            fflush(f);
            fclose(f);
        }

        f22::InitDXGIProxy();
        CreateThread(nullptr, 0, f22::ShimWorkerThread, nullptr, 0, nullptr);
        break;
    }
    case DLL_PROCESS_DETACH:
        f22::RemoveDXGIHooks();
        f22::SteamBridge::Instance().Shutdown();
        f22::ControlClient::Instance().Stop();
        MH_Uninitialize();
        if (f22::g_real_dxgi) {
            FreeLibrary(f22::g_real_dxgi);
            f22::g_real_dxgi = nullptr;
        }
        break;
    }
    return TRUE;
}

} // extern "C"
