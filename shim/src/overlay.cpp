#include "overlay.h"
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
#include <mutex>
#include <cstring>

extern IMGUI_IMPL_API LRESULT ImGui_ImplWin32_WndProcHandler(HWND hWnd, UINT msg, WPARAM wParam, LPARAM lParam);

static std::mutex g_hud_mutex;
static SeamlessHudState g_hud_state;

void UpdateHudState(const SeamlessHudState& state) {
    std::lock_guard<std::mutex> lock(g_hud_mutex);
    g_hud_state = state;
}

SeamlessHudState GetHudState() {
    std::lock_guard<std::mutex> lock(g_hud_mutex);
    return g_hud_state;
}

enum class RenderAPI {
    Unknown,
    D3D11,
    D3D12
};

using PFN_Present = HRESULT(WINAPI*)(IDXGISwapChain*, UINT, UINT);
using PFN_ResizeBuffers = HRESULT(WINAPI*)(IDXGISwapChain*, UINT, UINT, UINT, DXGI_FORMAT, UINT);
using PFN_ExecuteCommandLists = void(WINAPI*)(ID3D12CommandQueue*, UINT, ID3D12CommandList* const*);

static PFN_Present g_orig_Present = nullptr;
static PFN_ResizeBuffers g_orig_ResizeBuffers = nullptr;
static PFN_ExecuteCommandLists g_orig_ExecuteCommandLists = nullptr;

static bool g_imgui_initialized = false;
static RenderAPI g_render_api = RenderAPI::Unknown;
static HWND g_game_hwnd = nullptr;
static WNDPROC g_orig_wndproc = nullptr;

// D3D11
static ID3D11Device* g_d3d11_device = nullptr;
static ID3D11DeviceContext* g_d3d11_context = nullptr;
static ID3D11RenderTargetView* g_d3d11_rtv = nullptr;

// D3D12
static ID3D12Device* g_d3d12_device = nullptr;
static ID3D12CommandQueue* g_d3d12_cmd_queue = nullptr;
static ID3D12DescriptorHeap* g_d3d12_srv_heap = nullptr;
static ID3D12DescriptorHeap* g_d3d12_rtv_heap = nullptr;
static std::vector<ID3D12Resource*> g_d3d12_render_targets;
static std::vector<ID3D12CommandAllocator*> g_d3d12_cmd_allocators;
static ID3D12GraphicsCommandList* g_d3d12_cmd_list = nullptr;
static UINT g_buffer_count = 2;

static LRESULT CALLBACK Hooked_WndProc(HWND hWnd, UINT uMsg, WPARAM wParam, LPARAM lParam) {
    if (g_imgui_initialized) {
        ImGui_ImplWin32_WndProcHandler(hWnd, uMsg, wParam, lParam);
    }
    return CallWindowProc(g_orig_wndproc, hWnd, uMsg, wParam, lParam);
}

static void CleanupD3D11() {
    if (g_d3d11_rtv) { g_d3d11_rtv->Release(); g_d3d11_rtv = nullptr; }
    if (g_d3d11_context) { g_d3d11_context->Release(); g_d3d11_context = nullptr; }
    if (g_d3d11_device) { g_d3d11_device->Release(); g_d3d11_device = nullptr; }
}

static void CleanupD3D12() {
    for (auto* rtv : g_d3d12_render_targets) {
        if (rtv) rtv->Release();
    }
    g_d3d12_render_targets.clear();

    for (auto* alloc : g_d3d12_cmd_allocators) {
        if (alloc) alloc->Release();
    }
    g_d3d12_cmd_allocators.clear();

    if (g_d3d12_cmd_list) { g_d3d12_cmd_list->Release(); g_d3d12_cmd_list = nullptr; }
    if (g_d3d12_rtv_heap) { g_d3d12_rtv_heap->Release(); g_d3d12_rtv_heap = nullptr; }
    if (g_d3d12_srv_heap) { g_d3d12_srv_heap->Release(); g_d3d12_srv_heap = nullptr; }
    if (g_d3d12_device) { g_d3d12_device->Release(); g_d3d12_device = nullptr; }
}

static void WINAPI Hooked_ExecuteCommandLists(ID3D12CommandQueue* pQueue, UINT NumCommandLists, ID3D12CommandList* const* ppCommandLists) {
    if (!g_d3d12_cmd_queue && pQueue) {
        if (pQueue->GetDesc().Type == D3D12_COMMAND_LIST_TYPE_DIRECT) {
            g_d3d12_cmd_queue = pQueue;
        }
    }
    if (g_orig_ExecuteCommandLists) {
        g_orig_ExecuteCommandLists(pQueue, NumCommandLists, ppCommandLists);
    }
}

static void RenderSeamlessHud() {
    SeamlessHudState hud = GetHudState();

    ImGui::SetNextWindowPos(ImVec2(16.0f, 16.0f), ImGuiCond_Always);
    ImGui::SetNextWindowSize(ImVec2(320.0f, 0.0f), ImGuiCond_Always);

    ImGuiWindowFlags flags = ImGuiWindowFlags_NoDecoration |
                             ImGuiWindowFlags_NoInputs |
                             ImGuiWindowFlags_NoNav |
                             ImGuiWindowFlags_AlwaysAutoResize |
                             ImGuiWindowFlags_NoFocusOnAppearing;

    ImGui::PushStyleColor(ImGuiCol_WindowBg, ImVec4(0.08f, 0.09f, 0.12f, 0.85f));
    ImGui::PushStyleColor(ImGuiCol_Border, ImVec4(0.2f, 0.5f, 0.9f, 0.6f));
    ImGui::PushStyleVar(ImGuiStyleVar_WindowRounding, 8.0f);
    ImGui::PushStyleVar(ImGuiStyleVar_WindowBorderSize, 1.5f);
    ImGui::PushStyleVar(ImGuiStyleVar_WindowPadding, ImVec2(12.0f, 10.0f));

    if (ImGui::Begin("SeamlessHUD", nullptr, flags)) {
        ImGui::TextColored(ImVec4(0.3f, 0.8f, 1.0f, 1.0f), "[SF22] SEAMLESS FIFA 22");
        ImGui::Separator();

        // Status indicator dot
        ImVec4 dotColor = ImVec4(0.9f, 0.2f, 0.2f, 1.0f); // Red
        if (hud.is_peer_connected) {
            dotColor = ImVec4(0.2f, 0.9f, 0.3f, 1.0f); // Green
        } else if (hud.is_connected) {
            dotColor = ImVec4(0.95f, 0.8f, 0.2f, 1.0f); // Yellow
        }

        ImGui::TextColored(dotColor, "O");
        ImGui::SameLine();
        ImGui::Text("%s", hud.status_text.c_str());

        ImGui::TextColored(ImVec4(0.7f, 0.7f, 0.7f, 1.0f), "Room :");
        ImGui::SameLine();
        ImGui::TextColored(ImVec4(1.0f, 1.0f, 1.0f, 1.0f), "%s", hud.room_name.c_str());

        ImGui::TextColored(ImVec4(0.7f, 0.7f, 0.7f, 1.0f), "Role :");
        ImGui::SameLine();
        ImGui::TextColored(ImVec4(0.4f, 0.9f, 0.5f, 1.0f), "%s", hud.role_text.c_str());

        ImGui::Separator();
        ImGui::TextColored(ImVec4(0.6f, 0.8f, 1.0f, 1.0f), "Ping: %u ms  |  TX: %llu  RX: %llu",
                           hud.rtt_ms, hud.packets_sent, hud.packets_recv);
    }
    ImGui::End();

    ImGui::PopStyleVar(3);
    ImGui::PopStyleColor(2);
}

static HRESULT WINAPI Hooked_Present(IDXGISwapChain* pSwapChain, UINT SyncInterval, UINT Flags) {
    if (!g_imgui_initialized) {
        DXGI_SWAP_CHAIN_DESC desc;
        if (FAILED(pSwapChain->GetDesc(&desc))) {
            return g_orig_Present(pSwapChain, SyncInterval, Flags);
        }

        g_game_hwnd = desc.OutputWindow;
        g_orig_wndproc = (WNDPROC)SetWindowLongPtr(g_game_hwnd, GWLP_WNDPROC, (LONG_PTR)Hooked_WndProc);

        ID3D12Device* d3d12_dev = nullptr;
        if (SUCCEEDED(pSwapChain->GetDevice(__uuidof(ID3D12Device), (void**)&d3d12_dev)) && g_d3d12_cmd_queue) {
            g_render_api = RenderAPI::D3D12;
            g_d3d12_device = d3d12_dev;
            g_buffer_count = desc.BufferCount;

            D3D12_DESCRIPTOR_HEAP_DESC rtv_desc = {};
            rtv_desc.Type = D3D12_DESCRIPTOR_HEAP_TYPE_RTV;
            rtv_desc.NumDescriptors = g_buffer_count;
            g_d3d12_device->CreateDescriptorHeap(&rtv_desc, IID_PPV_ARGS(&g_d3d12_rtv_heap));

            D3D12_DESCRIPTOR_HEAP_DESC srv_desc = {};
            srv_desc.Type = D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV;
            srv_desc.NumDescriptors = 1;
            srv_desc.Flags = D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE;
            g_d3d12_device->CreateDescriptorHeap(&srv_desc, IID_PPV_ARGS(&g_d3d12_srv_heap));

            SIZE_T rtv_step = g_d3d12_device->GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_RTV);
            D3D12_CPU_DESCRIPTOR_HANDLE rtv_handle = g_d3d12_rtv_heap->GetCPUDescriptorHandleForHeapStart();

            g_d3d12_render_targets.resize(g_buffer_count);
            g_d3d12_cmd_allocators.resize(g_buffer_count);

            for (UINT i = 0; i < g_buffer_count; ++i) {
                pSwapChain->GetBuffer(i, IID_PPV_ARGS(&g_d3d12_render_targets[i]));
                g_d3d12_device->CreateRenderTargetView(g_d3d12_render_targets[i], nullptr, rtv_handle);
                rtv_handle.ptr += rtv_step;

                g_d3d12_device->CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT, IID_PPV_ARGS(&g_d3d12_cmd_allocators[i]));
            }

            g_d3d12_device->CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, g_d3d12_cmd_allocators[0], nullptr, IID_PPV_ARGS(&g_d3d12_cmd_list));
            g_d3d12_cmd_list->Close();

            ImGui::CreateContext();
            ImGui_ImplWin32_Init(g_game_hwnd);

            ImGui_ImplDX12_InitInfo init_info = {};
            init_info.Device = g_d3d12_device;
            init_info.CommandQueue = g_d3d12_cmd_queue;
            init_info.NumFramesInFlight = g_buffer_count;
            init_info.RTVFormat = desc.BufferDesc.Format;
            init_info.SrvDescriptorHeap = g_d3d12_srv_heap;
            init_info.LegacySingleSrvCpuDescriptor = g_d3d12_srv_heap->GetCPUDescriptorHandleForHeapStart();
            init_info.LegacySingleSrvGpuDescriptor = g_d3d12_srv_heap->GetGPUDescriptorHandleForHeapStart();

            ImGui_ImplDX12_Init(&init_info);

            g_imgui_initialized = true;
        }
    }

    if (g_imgui_initialized && g_render_api == RenderAPI::D3D12 && g_d3d12_cmd_queue) {
        ImGui_ImplDX12_NewFrame();
        ImGui_ImplWin32_NewFrame();
        ImGui::NewFrame();

        RenderSeamlessHud();

        ImGui::Render();

        IDXGISwapChain3* swap3 = nullptr;
        if (SUCCEEDED(pSwapChain->QueryInterface(IID_PPV_ARGS(&swap3)))) {
            UINT back_buffer_idx = swap3->GetCurrentBackBufferIndex();
            swap3->Release();

            ID3D12CommandAllocator* alloc = g_d3d12_cmd_allocators[back_buffer_idx];
            alloc->Reset();

            D3D12_RESOURCE_BARRIER barrier = {};
            barrier.Type = D3D12_RESOURCE_BARRIER_TYPE_TRANSITION;
            barrier.Flags = D3D12_RESOURCE_BARRIER_FLAG_NONE;
            barrier.Transition.pResource = g_d3d12_render_targets[back_buffer_idx];
            barrier.Transition.Subresource = D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES;
            barrier.Transition.StateBefore = D3D12_RESOURCE_STATE_PRESENT;
            barrier.Transition.StateAfter = D3D12_RESOURCE_STATE_RENDER_TARGET;

            g_d3d12_cmd_list->Reset(alloc, nullptr);
            g_d3d12_cmd_list->ResourceBarrier(1, &barrier);

            SIZE_T rtv_step = g_d3d12_device->GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_RTV);
            D3D12_CPU_DESCRIPTOR_HANDLE rtv_handle = g_d3d12_rtv_heap->GetCPUDescriptorHandleForHeapStart();
            rtv_handle.ptr += back_buffer_idx * rtv_step;

            g_d3d12_cmd_list->OMSetRenderTargets(1, &rtv_handle, FALSE, nullptr);
            g_d3d12_cmd_list->SetDescriptorHeaps(1, &g_d3d12_srv_heap);

            ImGui_ImplDX12_RenderDrawData(ImGui::GetDrawData(), g_d3d12_cmd_list);

            barrier.Transition.StateBefore = D3D12_RESOURCE_STATE_RENDER_TARGET;
            barrier.Transition.StateAfter = D3D12_RESOURCE_STATE_PRESENT;
            g_d3d12_cmd_list->ResourceBarrier(1, &barrier);
            g_d3d12_cmd_list->Close();

            ID3D12CommandList* lists[] = { g_d3d12_cmd_list };
            g_d3d12_cmd_queue->ExecuteCommandLists(1, lists);
        }
    }

    return g_orig_Present(pSwapChain, SyncInterval, Flags);
}

static HRESULT WINAPI Hooked_ResizeBuffers(IDXGISwapChain* pSwapChain, UINT BufferCount, UINT Width, UINT Height, DXGI_FORMAT NewFormat, UINT SwapChainFlags) {
    if (g_imgui_initialized) {
        if (g_render_api == RenderAPI::D3D12) {
            for (auto* rtv : g_d3d12_render_targets) {
                if (rtv) rtv->Release();
            }
            g_d3d12_render_targets.clear();
        }
    }

    HRESULT hr = g_orig_ResizeBuffers(pSwapChain, BufferCount, Width, Height, NewFormat, SwapChainFlags);

    if (g_imgui_initialized && g_render_api == RenderAPI::D3D12) {
        g_buffer_count = BufferCount;
        SIZE_T rtv_step = g_d3d12_device->GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_RTV);
        D3D12_CPU_DESCRIPTOR_HANDLE rtv_handle = g_d3d12_rtv_heap->GetCPUDescriptorHandleForHeapStart();

        g_d3d12_render_targets.resize(g_buffer_count);
        for (UINT i = 0; i < g_buffer_count; ++i) {
            pSwapChain->GetBuffer(i, IID_PPV_ARGS(&g_d3d12_render_targets[i]));
            g_d3d12_device->CreateRenderTargetView(g_d3d12_render_targets[i], nullptr, rtv_handle);
            rtv_handle.ptr += rtv_step;
        }
    }

    return hr;
}

bool InstallDxgiOverlayHooks() {
    WNDCLASSEXW wc = { sizeof(WNDCLASSEXW), CS_CLASSDC, DefWindowProcW, 0L, 0L, GetModuleHandleW(nullptr), nullptr, nullptr, nullptr, nullptr, L"DXGI_Dummy", nullptr };
    RegisterClassExW(&wc);
    HWND hWnd = CreateWindowW(wc.lpszClassName, L"Dummy", WS_OVERLAPPEDWINDOW, 0, 0, 100, 100, nullptr, nullptr, wc.hInstance, nullptr);

    D3D_FEATURE_LEVEL featureLevel = D3D_FEATURE_LEVEL_11_0;
    DXGI_SWAP_CHAIN_DESC scd = {};
    scd.BufferCount = 1;
    scd.BufferDesc.Format = DXGI_FORMAT_R8G8B8A8_UNORM;
    scd.BufferUsage = DXGI_USAGE_RENDER_TARGET_OUTPUT;
    scd.OutputWindow = hWnd;
    scd.SampleDesc.Count = 1;
    scd.Windowed = TRUE;

    ID3D11Device* dummyDevice = nullptr;
    ID3D11DeviceContext* dummyContext = nullptr;
    IDXGISwapChain* dummySwapChain = nullptr;

    HRESULT hr = D3D11CreateDeviceAndSwapChain(
        nullptr, D3D_DRIVER_TYPE_HARDWARE, nullptr, 0,
        &featureLevel, 1, D3D11_SDK_VERSION, &scd,
        &dummySwapChain, &dummyDevice, nullptr, &dummyContext
    );

    if (FAILED(hr)) {
        DestroyWindow(hWnd);
        UnregisterClassW(wc.lpszClassName, wc.hInstance);
        return false;
    }

    void** vtbl = *reinterpret_cast<void***>(dummySwapChain);
    void* pPresent = vtbl[8];
    void* pResizeBuffers = vtbl[13];

    MH_CreateHook(pPresent, reinterpret_cast<LPVOID>(&Hooked_Present), reinterpret_cast<LPVOID*>(&g_orig_Present));
    MH_EnableHook(pPresent);

    MH_CreateHook(pResizeBuffers, reinterpret_cast<LPVOID>(&Hooked_ResizeBuffers), reinterpret_cast<LPVOID*>(&g_orig_ResizeBuffers));
    MH_EnableHook(pResizeBuffers);

    dummySwapChain->Release();
    dummyContext->Release();
    dummyDevice->Release();
    DestroyWindow(hWnd);
    UnregisterClassW(wc.lpszClassName, wc.hInstance);

    // Also detour ExecuteCommandLists for D3D12 command queue capture
    HMODULE hD3D12 = GetModuleHandleW(L"d3d12.dll");
    if (!hD3D12) hD3D12 = LoadLibraryW(L"d3d12.dll");
    if (hD3D12) {
        using PFN_D3D12CreateDevice = HRESULT(WINAPI*)(IUnknown*, D3D_FEATURE_LEVEL, REFIID, void**);
        auto pCreateDevice = reinterpret_cast<PFN_D3D12CreateDevice>(GetProcAddress(hD3D12, "D3D12CreateDevice"));
        if (pCreateDevice) {
            ID3D12Device* dev = nullptr;
            if (SUCCEEDED(pCreateDevice(nullptr, D3D_FEATURE_LEVEL_11_0, IID_PPV_ARGS(&dev)))) {
                D3D12_COMMAND_QUEUE_DESC qd = {};
                qd.Type = D3D12_COMMAND_LIST_TYPE_DIRECT;
                ID3D12CommandQueue* queue = nullptr;
                if (SUCCEEDED(dev->CreateCommandQueue(&qd, IID_PPV_ARGS(&queue)))) {
                    void** q_vtbl = *reinterpret_cast<void***>(queue);
                    void* pExec = q_vtbl[10]; // ExecuteCommandLists index in ID3D12CommandQueue
                    MH_CreateHook(pExec, reinterpret_cast<LPVOID>(&Hooked_ExecuteCommandLists), reinterpret_cast<LPVOID*>(&g_orig_ExecuteCommandLists));
                    MH_EnableHook(pExec);
                    queue->Release();
                }
                dev->Release();
            }
        }
    }

    return true;
}

void RemoveDxgiOverlayHooks() {
    MH_DisableHook(MH_ALL_HOOKS);
    if (g_game_hwnd && g_orig_wndproc) {
        SetWindowLongPtr(g_game_hwnd, GWLP_WNDPROC, (LONG_PTR)g_orig_wndproc);
    }
    if (g_imgui_initialized) {
        if (g_render_api == RenderAPI::D3D12) {
            ImGui_ImplDX12_Shutdown();
        }
        ImGui_ImplWin32_Shutdown();
        ImGui::DestroyContext();
        CleanupD3D12();
        CleanupD3D11();
        g_imgui_initialized = false;
    }
}
