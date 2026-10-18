#include "f22_shim.h"
#include "control_client.h"
#include "hooks_dxgi.h"
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

static HMODULE g_real_dxgi = nullptr;
static PFN_CreateDXGIFactory g_pfn_CreateDXGIFactory = nullptr;
static PFN_CreateDXGIFactory1 g_pfn_CreateDXGIFactory1 = nullptr;
static PFN_CreateDXGIFactory2 g_pfn_CreateDXGIFactory2 = nullptr;
static PFN_DXGIDeclareAdapterRemovalSupport g_pfn_DXGIDeclareAdapterRemovalSupport = nullptr;
static PFN_DXGIGetDebugInterface1 g_pfn_DXGIGetDebugInterface1 = nullptr;

bool InitDXGIProxy() {
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

BOOL WINAPI DllMain(HINSTANCE hinstDLL, DWORD fdwReason, LPVOID lpvReserved) {
    switch (fdwReason) {
    case DLL_PROCESS_ATTACH:
        DisableThreadLibraryCalls(hinstDLL);
        f22::InitDXGIProxy();
        CreateThread(nullptr, 0, f22::ShimWorkerThread, nullptr, 0, nullptr);
        break;
    case DLL_PROCESS_DETACH:
        f22::RemoveDXGIHooks();
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
