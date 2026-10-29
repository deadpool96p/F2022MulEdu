#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <dxgi.h>
#include <string>

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
    if (g_real_dxgi) return true;

    // Load dxgi_org.dll from game root
    g_real_dxgi = LoadLibraryW(L"dxgi_org.dll");
    if (!g_real_dxgi) {
        wchar_t sys_path[MAX_PATH];
        if (GetSystemDirectoryW(sys_path, MAX_PATH) > 0) {
            std::wstring real_dxgi_path = std::wstring(sys_path) + L"\\dxgi.dll";
            g_real_dxgi = LoadLibraryW(real_dxgi_path.c_str());
        }
    }
    if (!g_real_dxgi) return false;

    g_pfn_CreateDXGIFactory = (PFN_CreateDXGIFactory)GetProcAddress(g_real_dxgi, "CreateDXGIFactory");
    g_pfn_CreateDXGIFactory1 = (PFN_CreateDXGIFactory1)GetProcAddress(g_real_dxgi, "CreateDXGIFactory1");
    g_pfn_CreateDXGIFactory2 = (PFN_CreateDXGIFactory2)GetProcAddress(g_real_dxgi, "CreateDXGIFactory2");
    g_pfn_DXGIDeclareAdapterRemovalSupport = (PFN_DXGIDeclareAdapterRemovalSupport)GetProcAddress(g_real_dxgi, "DXGIDeclareAdapterRemovalSupport");
    g_pfn_DXGIGetDebugInterface1 = (PFN_DXGIGetDebugInterface1)GetProcAddress(g_real_dxgi, "DXGIGetDebugInterface1");
    g_pfn_DXGIReportActiveObjects = (PFN_DXGIReportActiveObjects)GetProcAddress(g_real_dxgi, "DXGIReportActiveObjects");
    g_pfn_DXGID3D10CreateDevice = (PFN_DXGID3D10CreateDevice)GetProcAddress(g_real_dxgi, "DXGID3D10CreateDevice");
    g_pfn_DXGID3D10RegisterLayers = (PFN_DXGID3D10RegisterLayers)GetProcAddress(g_real_dxgi, "DXGID3D10RegisterLayers");

    return true;
}

extern "C" {

HRESULT WINAPI CreateDXGIFactory(REFIID riid, void **ppFactory) {
    if (!g_pfn_CreateDXGIFactory) InitDXGIProxy();
    return g_pfn_CreateDXGIFactory ? g_pfn_CreateDXGIFactory(riid, ppFactory) : E_FAIL;
}

HRESULT WINAPI CreateDXGIFactory1(REFIID riid, void **ppFactory) {
    if (!g_pfn_CreateDXGIFactory1) InitDXGIProxy();
    return g_pfn_CreateDXGIFactory1 ? g_pfn_CreateDXGIFactory1(riid, ppFactory) : E_FAIL;
}

HRESULT WINAPI CreateDXGIFactory2(UINT Flags, REFIID riid, void **ppFactory) {
    if (!g_pfn_CreateDXGIFactory2) InitDXGIProxy();
    return g_pfn_CreateDXGIFactory2 ? g_pfn_CreateDXGIFactory2(Flags, riid, ppFactory) : E_FAIL;
}

HRESULT WINAPI DXGIDeclareAdapterRemovalSupport(void) {
    if (!g_pfn_DXGIDeclareAdapterRemovalSupport) InitDXGIProxy();
    return g_pfn_DXGIDeclareAdapterRemovalSupport ? g_pfn_DXGIDeclareAdapterRemovalSupport() : E_FAIL;
}

HRESULT WINAPI DXGIGetDebugInterface1(UINT Flags, REFIID riid, void **pDebug) {
    if (!g_pfn_DXGIGetDebugInterface1) InitDXGIProxy();
    return g_pfn_DXGIGetDebugInterface1 ? g_pfn_DXGIGetDebugInterface1(Flags, riid, pDebug) : E_FAIL;
}

HRESULT WINAPI DXGIReportActiveObjects(void) {
    if (!g_pfn_DXGIReportActiveObjects) InitDXGIProxy();
    return g_pfn_DXGIReportActiveObjects ? g_pfn_DXGIReportActiveObjects() : S_OK;
}

HRESULT WINAPI DXGID3D10CreateDevice(void* p1, void* p2, void* p3, void* p4, void* p5, void* p6) {
    if (!g_pfn_DXGID3D10CreateDevice) InitDXGIProxy();
    return g_pfn_DXGID3D10CreateDevice ? g_pfn_DXGID3D10CreateDevice(p1, p2, p3, p4, p5, p6) : E_FAIL;
}

HRESULT WINAPI DXGID3D10RegisterLayers(const void* pLayers, UINT NumLayers) {
    if (!g_pfn_DXGID3D10RegisterLayers) InitDXGIProxy();
    return g_pfn_DXGID3D10RegisterLayers ? g_pfn_DXGID3D10RegisterLayers(pLayers, NumLayers) : S_OK;
}

}
