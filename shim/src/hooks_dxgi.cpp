#include "hooks_dxgi.h"
#include "f22_shim.h"
#include "overlay/input_hook.h"
#include "overlay/minimal_overlay.h"
#include "control_client.h"
#include "steam_bridge.hpp"

#include <MinHook.h>
#include <d3d11.h>
#include <d3d12.h>
#include <dxgi.h>
#include <dxgi1_4.h>
#include <imgui.h>
#include <backends/imgui_impl_win32.h>
#include <backends/imgui_impl_dx11.h>
#include <backends/imgui_impl_dx12.h>
#include <vector>

namespace f22 {

enum class RenderAPI {
    Unknown,
    D3D11,
    D3D12
};

using PFN_Present = HRESULT(WINAPI*)(IDXGISwapChain*, UINT, UINT);
using PFN_ResizeBuffers = HRESULT(WINAPI*)(IDXGISwapChain*, UINT, UINT, UINT, DXGI_FORMAT, UINT);

static PFN_Present g_original_present = nullptr;
static PFN_ResizeBuffers g_original_resize_buffers = nullptr;

static bool g_initialized = false;
static RenderAPI g_render_api = RenderAPI::Unknown;
static HWND g_hwnd = nullptr;

// D3D11 resources
static ID3D11Device* g_d3d11_device = nullptr;
static ID3D11DeviceContext* g_d3d11_context = nullptr;
static ID3D11RenderTargetView* g_d3d11_rtv = nullptr;

// D3D12 resources
static ID3D12Device* g_d3d12_device = nullptr;
static ID3D12CommandQueue* g_d3d12_cmd_queue = nullptr;
static ID3D12DescriptorHeap* g_d3d12_srv_heap = nullptr;
static ID3D12DescriptorHeap* g_d3d12_rtv_heap = nullptr;
static std::vector<ID3D12Resource*> g_d3d12_render_targets;
static std::vector<ID3D12CommandAllocator*> g_d3d12_cmd_allocators;
static ID3D12GraphicsCommandList* g_d3d12_cmd_list = nullptr;
static UINT g_buffer_count = 2;

static void CleanupD3D11RTV() {
    if (g_d3d11_rtv) {
        g_d3d11_rtv->Release();
        g_d3d11_rtv = nullptr;
    }
}

static bool CreateD3D11RTV(IDXGISwapChain* pSwapChain) {
    CleanupD3D11RTV();
    ID3D11Texture2D* pBackBuffer = nullptr;
    if (SUCCEEDED(pSwapChain->GetBuffer(0, __uuidof(ID3D11Texture2D), (void**)&pBackBuffer))) {
        HRESULT hr = g_d3d11_device->CreateRenderTargetView(pBackBuffer, nullptr, &g_d3d11_rtv);
        pBackBuffer->Release();
        return SUCCEEDED(hr);
    }
    return false;
}

using PFN_ExecuteCommandLists = void(WINAPI*)(ID3D12CommandQueue*, UINT, ID3D12CommandList* const*);
static PFN_ExecuteCommandLists g_original_ExecuteCommandLists = nullptr;

static void WINAPI Hooked_ExecuteCommandLists(ID3D12CommandQueue* pQueue, UINT NumCommandLists, ID3D12CommandList* const* ppCommandLists) {
    if (!g_d3d12_cmd_queue && pQueue) {
        if (pQueue->GetDesc().Type == D3D12_COMMAND_LIST_TYPE_DIRECT) {
            g_d3d12_cmd_queue = pQueue;
            LogInfo("Captured active D3D12 Direct Command Queue from ExecuteCommandLists.");
        }
    }
    g_original_ExecuteCommandLists(pQueue, NumCommandLists, ppCommandLists);
}

static void CleanupD3D12RTV() {
    for (auto* rt : g_d3d12_render_targets) {
        if (rt) rt->Release();
    }
    g_d3d12_render_targets.clear();
}

static bool CreateD3D12RTV(IDXGISwapChain* pSwapChain) {
    CleanupD3D12RTV();

    DXGI_SWAP_CHAIN_DESC desc;
    if (FAILED(pSwapChain->GetDesc(&desc))) {
        return false;
    }
    g_buffer_count = desc.BufferCount > 0 ? desc.BufferCount : 2;

    D3D12_DESCRIPTOR_HEAP_DESC rtv_desc = {};
    rtv_desc.Type = D3D12_DESCRIPTOR_HEAP_TYPE_RTV;
    rtv_desc.NumDescriptors = g_buffer_count;
    rtv_desc.Flags = D3D12_DESCRIPTOR_HEAP_FLAG_NONE;

    if (!g_d3d12_rtv_heap) {
        if (FAILED(g_d3d12_device->CreateDescriptorHeap(&rtv_desc, __uuidof(ID3D12DescriptorHeap), (void**)&g_d3d12_rtv_heap))) {
            LogError("Failed to create D3D12 RTV descriptor heap.");
            return false;
        }
    }

    SIZE_T rtv_handle_size = g_d3d12_device->GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_RTV);
    D3D12_CPU_DESCRIPTOR_HANDLE rtv_handle = g_d3d12_rtv_heap->GetCPUDescriptorHandleForHeapStart();

    g_d3d12_render_targets.resize(g_buffer_count, nullptr);
    if (g_d3d12_cmd_allocators.empty()) {
        g_d3d12_cmd_allocators.resize(g_buffer_count, nullptr);
    }

    for (UINT i = 0; i < g_buffer_count; ++i) {
        if (FAILED(pSwapChain->GetBuffer(i, __uuidof(ID3D12Resource), (void**)&g_d3d12_render_targets[i]))) {
            LogError("Failed to get D3D12 backbuffer from swapchain.");
            return false;
        }

        D3D12_CPU_DESCRIPTOR_HANDLE handle = { rtv_handle.ptr + (i * rtv_handle_size) };
        g_d3d12_device->CreateRenderTargetView(g_d3d12_render_targets[i], nullptr, handle);

        if (!g_d3d12_cmd_allocators[i]) {
            if (FAILED(g_d3d12_device->CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT, __uuidof(ID3D12CommandAllocator), (void**)&g_d3d12_cmd_allocators[i]))) {
                LogError("Failed to create D3D12 command allocator.");
                return false;
            }
        }
    }

    if (!g_d3d12_cmd_list) {
        if (FAILED(g_d3d12_device->CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, g_d3d12_cmd_allocators[0], nullptr, __uuidof(ID3D12GraphicsCommandList), (void**)&g_d3d12_cmd_list))) {
            LogError("Failed to create D3D12 command list.");
            return false;
        }
        g_d3d12_cmd_list->Close();
    }

    return true;
}

static bool InitImGuiD3D11(IDXGISwapChain* pSwapChain, ID3D11Device* pDevice) {
    g_d3d11_device = pDevice;
    g_d3d11_device->GetImmediateContext(&g_d3d11_context);

    ImGui::CreateContext();
    InitOverlayStyle();

    if (!ImGui_ImplWin32_Init(g_hwnd)) {
        LogError("ImGui_ImplWin32_Init failed!");
        return false;
    }

    if (!ImGui_ImplDX11_Init(g_d3d11_device, g_d3d11_context)) {
        LogError("ImGui_ImplDX11_Init failed!");
        return false;
    }

    CreateD3D11RTV(pSwapChain);
    g_render_api = RenderAPI::D3D11;
    g_initialized = true;
    LogInfo("Initialized ImGui with DirectX 11 backend.");
    return true;
}

static bool InitImGuiD3D12(IDXGISwapChain* pSwapChain, ID3D12Device* pDevice) {
    if (!g_d3d12_cmd_queue) {
        pSwapChain->GetDevice(__uuidof(ID3D12CommandQueue), (void**)&g_d3d12_cmd_queue);
    }
    if (!g_d3d12_cmd_queue) {
        LogError("InitImGuiD3D12: CommandQueue not yet acquired. Will retry.");
        return false;
    }

    g_d3d12_device = pDevice;

    DXGI_SWAP_CHAIN_DESC desc;
    pSwapChain->GetDesc(&desc);
    g_buffer_count = desc.BufferCount > 0 ? desc.BufferCount : 2;

    // Create Descriptor Heap for ImGui Font SRV
    D3D12_DESCRIPTOR_HEAP_DESC srv_desc = {};
    srv_desc.Type = D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV;
    srv_desc.NumDescriptors = 1;
    srv_desc.Flags = D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE;
    if (FAILED(g_d3d12_device->CreateDescriptorHeap(&srv_desc, __uuidof(ID3D12DescriptorHeap), (void**)&g_d3d12_srv_heap))) {
        LogError("Failed to create D3D12 SRV descriptor heap for ImGui.");
        return false;
    }

    if (!CreateD3D12RTV(pSwapChain)) {
        LogError("Failed to initialize D3D12 RTVs and Command Allocators.");
        return false;
    }

    ImGui::CreateContext();
    InitOverlayStyle();

    if (!ImGui_ImplWin32_Init(g_hwnd)) {
        LogError("ImGui_ImplWin32_Init failed for D3D12!");
        return false;
    }

    // Initialize ImGui DX12
    ImGui_ImplDX12_InitInfo init_info = {};
    init_info.Device = g_d3d12_device;
    init_info.CommandQueue = g_d3d12_cmd_queue;
    init_info.NumFramesInFlight = g_buffer_count;
    init_info.RTVFormat = desc.BufferDesc.Format;
    init_info.SrvDescriptorHeap = g_d3d12_srv_heap;
    init_info.LegacySingleSrvCpuDescriptor = g_d3d12_srv_heap->GetCPUDescriptorHandleForHeapStart();
    init_info.LegacySingleSrvGpuDescriptor = g_d3d12_srv_heap->GetGPUDescriptorHandleForHeapStart();

    if (!ImGui_ImplDX12_Init(&init_info)) {
        LogError("ImGui_ImplDX12_Init failed!");
        return false;
    }

    g_render_api = RenderAPI::D3D12;
    g_initialized = true;
    LogInfo("Initialized ImGui with DirectX 12 backend.");
    return true;
}

static HRESULT WINAPI Hooked_Present(IDXGISwapChain* pSwapChain, UINT SyncInterval, UINT Flags) {
    if (!pSwapChain) {
        return g_original_present(pSwapChain, SyncInterval, Flags);
    }

    SteamBridge::Instance().RunCallbacks();

    if (!g_initialized) {
        DXGI_SWAP_CHAIN_DESC desc;
        if (SUCCEEDED(pSwapChain->GetDesc(&desc))) {
            g_hwnd = desc.OutputWindow;
            InstallInputHooks(g_hwnd);
            InstallXInputHooks();

            // Try DirectX 11 first
            ID3D11Device* pD3D11 = nullptr;
            if (SUCCEEDED(pSwapChain->GetDevice(__uuidof(ID3D11Device), (void**)&pD3D11)) && pD3D11) {
                InitImGuiD3D11(pSwapChain, pD3D11);
            } else {
                // Try DirectX 12
                ID3D12Device* pD3D12 = nullptr;
                if (SUCCEEDED(pSwapChain->GetDevice(__uuidof(ID3D12Device), (void**)&pD3D12)) && pD3D12) {
                    if (!g_d3d12_cmd_queue) {
                        pSwapChain->GetDevice(__uuidof(ID3D12CommandQueue), (void**)&g_d3d12_cmd_queue);
                    }
                    if (g_d3d12_cmd_queue) {
                        InitImGuiD3D12(pSwapChain, pD3D12);
                    }
                }
            }
        }
    }

    // Render Overlay Frame
    if (g_initialized) {
        if (g_render_api == RenderAPI::D3D11) {
            if (!g_d3d11_rtv) {
                CreateD3D11RTV(pSwapChain);
            }

            if (g_d3d11_rtv && g_d3d11_context) {
                ImGui_ImplDX11_NewFrame();
                ImGui_ImplWin32_NewFrame();
                ImGui::NewFrame();

                RenderMinimalOverlay();

                ImGui::Render();
                g_d3d11_context->OMSetRenderTargets(1, &g_d3d11_rtv, nullptr);
                ImGui_ImplDX11_RenderDrawData(ImGui::GetDrawData());
            }
        } else if (g_render_api == RenderAPI::D3D12) {
            IDXGISwapChain3* pSwapChain3 = nullptr;
            UINT back_buffer_idx = 0;
            if (SUCCEEDED(pSwapChain->QueryInterface(__uuidof(IDXGISwapChain3), (void**)&pSwapChain3))) {
                back_buffer_idx = pSwapChain3->GetCurrentBackBufferIndex();
                pSwapChain3->Release();
            }

            if (g_d3d12_cmd_queue && g_d3d12_cmd_list && back_buffer_idx < g_d3d12_render_targets.size() && g_d3d12_render_targets[back_buffer_idx]) {
                ImGui_ImplDX12_NewFrame();
                ImGui_ImplWin32_NewFrame();
                ImGui::NewFrame();

                RenderMinimalOverlay();

                ImGui::Render();

                auto* cmd_allocator = g_d3d12_cmd_allocators[back_buffer_idx];
                cmd_allocator->Reset();
                g_d3d12_cmd_list->Reset(cmd_allocator, nullptr);

                D3D12_RESOURCE_BARRIER barrier = {};
                barrier.Type = D3D12_RESOURCE_BARRIER_TYPE_TRANSITION;
                barrier.Flags = D3D12_RESOURCE_BARRIER_FLAG_NONE;
                barrier.Transition.pResource = g_d3d12_render_targets[back_buffer_idx];
                barrier.Transition.Subresource = D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES;
                barrier.Transition.StateBefore = D3D12_RESOURCE_STATE_PRESENT;
                barrier.Transition.StateAfter = D3D12_RESOURCE_STATE_RENDER_TARGET;

                g_d3d12_cmd_list->ResourceBarrier(1, &barrier);

                SIZE_T rtv_handle_size = g_d3d12_device->GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_RTV);
                D3D12_CPU_DESCRIPTOR_HANDLE rtv_handle = { g_d3d12_rtv_heap->GetCPUDescriptorHandleForHeapStart().ptr + (back_buffer_idx * rtv_handle_size) };

                g_d3d12_cmd_list->OMSetRenderTargets(1, &rtv_handle, FALSE, nullptr);
                ID3D12DescriptorHeap* heaps[] = { g_d3d12_srv_heap };
                g_d3d12_cmd_list->SetDescriptorHeaps(1, heaps);

                ImGui_ImplDX12_RenderDrawData(ImGui::GetDrawData(), g_d3d12_cmd_list);

                barrier.Transition.StateBefore = D3D12_RESOURCE_STATE_RENDER_TARGET;
                barrier.Transition.StateAfter = D3D12_RESOURCE_STATE_PRESENT;
                g_d3d12_cmd_list->ResourceBarrier(1, &barrier);

                g_d3d12_cmd_list->Close();

                ID3D12CommandList* const ppCmdLists[] = { g_d3d12_cmd_list };
                g_d3d12_cmd_queue->ExecuteCommandLists(1, ppCmdLists);
            }
        }
    }

    return g_original_present(pSwapChain, SyncInterval, Flags);
}

static HRESULT WINAPI Hooked_ResizeBuffers(IDXGISwapChain* pSwapChain, UINT BufferCount, UINT Width, UINT Height,
                                         DXGI_FORMAT NewFormat, UINT SwapChainFlags) {
    if (g_render_api == RenderAPI::D3D11) {
        CleanupD3D11RTV();
    } else if (g_render_api == RenderAPI::D3D12) {
        CleanupD3D12RTV();
    }

    HRESULT hr = g_original_resize_buffers(pSwapChain, BufferCount, Width, Height, NewFormat, SwapChainFlags);

    if (SUCCEEDED(hr)) {
        if (g_render_api == RenderAPI::D3D11) {
            CreateD3D11RTV(pSwapChain);
        } else if (g_render_api == RenderAPI::D3D12) {
            CreateD3D12RTV(pSwapChain);
        }
    }

    return hr;
}

bool InstallDXGIHooks() {
    WNDCLASSEXA wc = { sizeof(WNDCLASSEXA), CS_CLASSDC, DefWindowProcA, 0L, 0L,
                       GetModuleHandleA(nullptr), nullptr, nullptr, nullptr, nullptr,
                       "DXGI_Dummy_Class", nullptr };
    RegisterClassExA(&wc);

    HWND hWnd = CreateWindowA("DXGI_Dummy_Class", "DXGI Dummy Window", WS_OVERLAPPEDWINDOW,
                              0, 0, 100, 100, nullptr, nullptr, wc.hInstance, nullptr);
    if (!hWnd) {
        LogError("Failed to create dummy window for DXGI VTable resolution.");
        UnregisterClassA("DXGI_Dummy_Class", wc.hInstance);
        return false;
    }

    D3D_FEATURE_LEVEL featureLevel = D3D_FEATURE_LEVEL_11_0;
    DXGI_SWAP_CHAIN_DESC sd = {};
    sd.BufferCount = 1;
    sd.BufferDesc.Format = DXGI_FORMAT_R8G8B8A8_UNORM;
    sd.BufferDesc.Width = 100;
    sd.BufferDesc.Height = 100;
    sd.BufferUsage = DXGI_USAGE_RENDER_TARGET_OUTPUT;
    sd.OutputWindow = hWnd;
    sd.SampleDesc.Count = 1;
    sd.Windowed = TRUE;
    sd.SwapEffect = DXGI_SWAP_EFFECT_DISCARD;

    ID3D11Device* pDevice = nullptr;
    ID3D11DeviceContext* pContext = nullptr;
    IDXGISwapChain* pSwapChain = nullptr;

    HRESULT hr = D3D11CreateDeviceAndSwapChain(
        nullptr, D3D_DRIVER_TYPE_HARDWARE, nullptr, 0,
        &featureLevel, 1, D3D11_SDK_VERSION, &sd,
        &pSwapChain, &pDevice, nullptr, &pContext
    );

    if (FAILED(hr)) {
        // Fallback to WARP driver if hardware device creation fails in headless / CI environments
        hr = D3D11CreateDeviceAndSwapChain(
            nullptr, D3D_DRIVER_TYPE_WARP, nullptr, 0,
            &featureLevel, 1, D3D11_SDK_VERSION, &sd,
            &pSwapChain, &pDevice, nullptr, &pContext
        );
    }

    if (FAILED(hr) || !pSwapChain) {
        LogError("Failed to create dummy D3D11 device and swap chain for VTable hooking.");
        DestroyWindow(hWnd);
        UnregisterClassA("DXGI_Dummy_Class", wc.hInstance);
        return false;
    }

    void** vtable = *reinterpret_cast<void***>(pSwapChain);
    void* present_target = vtable[8];
    void* resize_buffers_target = vtable[13];

    // Release dummy objects before hooking
    pSwapChain->Release();
    pContext->Release();
    pDevice->Release();
    DestroyWindow(hWnd);
    UnregisterClassA("DXGI_Dummy_Class", wc.hInstance);

    MH_STATUS s_pres = MH_CreateHook(present_target, reinterpret_cast<void*>(&Hooked_Present),
                                     reinterpret_cast<void**>(&g_original_present));
    if (s_pres != MH_OK) {
        LogError(std::string("MinHook failed to hook IDXGISwapChain::Present: ") + MH_StatusToString(s_pres));
        return false;
    }

    MH_STATUS s_res = MH_CreateHook(resize_buffers_target, reinterpret_cast<void*>(&Hooked_ResizeBuffers),
                                    reinterpret_cast<void**>(&g_original_resize_buffers));
    if (s_res != MH_OK) {
        LogError(std::string("MinHook failed to hook IDXGISwapChain::ResizeBuffers: ") + MH_StatusToString(s_res));
        return false;
    }

    MH_EnableHook(present_target);
    MH_EnableHook(resize_buffers_target);

    // Also attempt to hook ID3D12CommandQueue::ExecuteCommandLists to capture active D3D12 queue
    HMODULE hD3D12 = GetModuleHandleA("d3d12.dll");
    if (!hD3D12) {
        hD3D12 = LoadLibraryA("d3d12.dll");
    }
    if (hD3D12) {
        auto pfn_D3D12CreateDevice = reinterpret_cast<HRESULT(WINAPI*)(IUnknown*, D3D_FEATURE_LEVEL, REFIID, void**)>(
            GetProcAddress(hD3D12, "D3D12CreateDevice")
        );
        if (pfn_D3D12CreateDevice) {
            ID3D12Device* pDummyDevice = nullptr;
            if (SUCCEEDED(pfn_D3D12CreateDevice(nullptr, D3D_FEATURE_LEVEL_11_0, __uuidof(ID3D12Device), (void**)&pDummyDevice))) {
                D3D12_COMMAND_QUEUE_DESC qdesc = {};
                qdesc.Type = D3D12_COMMAND_LIST_TYPE_DIRECT;
                ID3D12CommandQueue* pDummyQueue = nullptr;
                if (SUCCEEDED(pDummyDevice->CreateCommandQueue(&qdesc, __uuidof(ID3D12CommandQueue), (void**)&pDummyQueue))) {
                    void** qvtable = *reinterpret_cast<void***>(pDummyQueue);
                    void* exec_target = qvtable[10]; // ExecuteCommandLists is index 10
                    if (MH_CreateHook(exec_target, reinterpret_cast<void*>(&Hooked_ExecuteCommandLists),
                                      reinterpret_cast<void**>(&g_original_ExecuteCommandLists)) == MH_OK) {
                        MH_EnableHook(exec_target);
                        LogInfo("ID3D12CommandQueue::ExecuteCommandLists (VTable index 10) hooked successfully.");
                    }
                    pDummyQueue->Release();
                }
                pDummyDevice->Release();
            }
        }
    }

    LogInfo("IDXGISwapChain::Present (VTable index 8) and ResizeBuffers (VTable index 13) hooked successfully.");
    return true;
}

void RemoveDXGIHooks() {
    if (g_original_present) {
        MH_DisableHook(reinterpret_cast<void*>(g_original_present));
        g_original_present = nullptr;
    }
    if (g_original_resize_buffers) {
        MH_DisableHook(reinterpret_cast<void*>(g_original_resize_buffers));
        g_original_resize_buffers = nullptr;
    }
    if (g_original_ExecuteCommandLists) {
        MH_DisableHook(reinterpret_cast<void*>(g_original_ExecuteCommandLists));
        g_original_ExecuteCommandLists = nullptr;
    }
    CleanupD3D11RTV();
    CleanupD3D12RTV();
    for (auto* alloc : g_d3d12_cmd_allocators) {
        if (alloc) alloc->Release();
    }
    g_d3d12_cmd_allocators.clear();
    if (g_d3d12_cmd_list) {
        g_d3d12_cmd_list->Release();
        g_d3d12_cmd_list = nullptr;
    }
    if (g_d3d12_rtv_heap) {
        g_d3d12_rtv_heap->Release();
        g_d3d12_rtv_heap = nullptr;
    }
    if (g_d3d12_srv_heap) {
        g_d3d12_srv_heap->Release();
        g_d3d12_srv_heap = nullptr;
    }
    if (g_initialized) {
        if (g_render_api == RenderAPI::D3D11) {
            ImGui_ImplDX11_Shutdown();
        } else if (g_render_api == RenderAPI::D3D12) {
            ImGui_ImplDX12_Shutdown();
        }
        ImGui_ImplWin32_Shutdown();
        ImGui::DestroyContext();
        g_initialized = false;
    }
}

} // namespace f22
