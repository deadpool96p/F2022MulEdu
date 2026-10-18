#include "f22_shim.h"
#include "control_client.h"
#include "hooks_dxgi.h"
#include "steam_bridge.hpp"
#include <MinHook.h>
#include <dxgi.h>
#include <dxgi1_2.h>
#include <dxgi1_3.h>
#include <dxgi1_6.h>

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

bool InitDXGIProxy() {
    if (g_real_dxgi) {
        return true;
    }

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

    LogInfo("Loaded real System32 dxgi.dll successfully.");
    return true;
}

static DWORD WINAPI ShimWorkerThread(LPVOID) {
    // Small delay to ensure host process runtime and CRT are fully initialized
    Sleep(200);

    LogInfo("=============================================================");
    LogInfo(" FIFA 22 Multiplayer Emulator Shim (dxgi.dll proxy) Active   ");
    LogInfo("=============================================================");

    LoadConfig();

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
