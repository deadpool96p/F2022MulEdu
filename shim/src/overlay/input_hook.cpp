#include "input_hook.h"
#include "f22_shim.h"
#include <MinHook.h>
#include <xinput.h>
#include <imgui.h>

// Forward declaration of ImGui Win32 message handler
extern IMGUI_IMPL_API LRESULT ImGui_ImplWin32_WndProcHandler(HWND hWnd, UINT msg, WPARAM wParam, LPARAM lParam);

namespace f22 {

static WNDPROC g_original_wndproc = nullptr;
static HWND g_hooked_hwnd = nullptr;
static bool g_overlay_focused = false;
static bool g_status_pill_visible = true;

// XInput function pointer
typedef DWORD (WINAPI *PFN_XInputGetState)(DWORD dwUserIndex, XINPUT_STATE* pState);
static PFN_XInputGetState g_original_XInputGetState = nullptr;
static bool g_combo_pressed_last = false;

static LRESULT CALLBACK HookedWndProc(HWND hWnd, UINT msg, WPARAM wParam, LPARAM lParam) {
    if (msg == WM_KEYDOWN) {
        if (wParam == VK_F1) {
            g_overlay_focused = !g_overlay_focused;
            LogInfo(std::string("Top-bar overlay toggled via F1: ") + (g_overlay_focused ? "ACTIVE" : "HIDDEN"));
            return 0;
        } else if (wParam == VK_F2) {
            g_status_pill_visible = !g_status_pill_visible;
            LogInfo(std::string("Status pill toggled via F2: ") + (g_status_pill_visible ? "VISIBLE" : "HIDDEN"));
            return 0;
        }
    }

    if (ImGui::GetCurrentContext()) {
        ImGui_ImplWin32_WndProcHandler(hWnd, msg, wParam, lParam);

        ImGuiIO& io = ImGui::GetIO();
        if (g_overlay_focused) {
            // Mask mouse events when ImGui wants mouse
            if (io.WantCaptureMouse) {
                switch (msg) {
                case WM_LBUTTONDOWN:
                case WM_LBUTTONUP:
                case WM_RBUTTONDOWN:
                case WM_RBUTTONUP:
                case WM_MBUTTONDOWN:
                case WM_MBUTTONUP:
                case WM_MOUSEWHEEL:
                case WM_MOUSEHWHEEL:
                case WM_LBUTTONDBLCLK:
                case WM_RBUTTONDBLCLK:
                    return 0;
                }
            }

            // Mask keyboard events when typing or focused
            if (io.WantCaptureKeyboard) {
                switch (msg) {
                case WM_KEYDOWN:
                case WM_KEYUP:
                case WM_SYSKEYDOWN:
                case WM_SYSKEYUP:
                case WM_CHAR:
                    return 0;
                }
            }
        }
    }

    if (g_original_wndproc) {
        return CallWindowProcW(g_original_wndproc, hWnd, msg, wParam, lParam);
    }
    return DefWindowProcW(hWnd, msg, wParam, lParam);
}

bool InstallInputHooks(HWND hWnd) {
    if (!hWnd || g_hooked_hwnd == hWnd) {
        return true;
    }

    if (g_hooked_hwnd && g_original_wndproc) {
        RemoveInputHooks(g_hooked_hwnd);
    }

    g_hooked_hwnd = hWnd;
    g_original_wndproc = reinterpret_cast<WNDPROC>(
        SetWindowLongPtrW(hWnd, GWLP_WNDPROC, reinterpret_cast<LONG_PTR>(HookedWndProc))
    );

    if (g_original_wndproc) {
        LogInfo("WndProc subclassed successfully for minimal overlay input handling.");
        return true;
    }

    LogError("Failed to subclass WndProc.");
    return false;
}

void RemoveInputHooks(HWND hWnd) {
    if (hWnd && g_original_wndproc) {
        SetWindowLongPtrW(hWnd, GWLP_WNDPROC, reinterpret_cast<LONG_PTR>(g_original_wndproc));
        g_original_wndproc = nullptr;
        g_hooked_hwnd = nullptr;
        LogInfo("WndProc restored.");
    }
}

static DWORD WINAPI Hooked_XInputGetState(DWORD dwUserIndex, XINPUT_STATE* pState) {
    if (!g_original_XInputGetState) {
        return ERROR_DEVICE_NOT_CONNECTED;
    }

    DWORD result = g_original_XInputGetState(dwUserIndex, pState);
    if (result == ERROR_SUCCESS && pState) {
        // Check L3 + R3 (Thumb buttons combo)
        WORD combo = XINPUT_GAMEPAD_LEFT_THUMB | XINPUT_GAMEPAD_RIGHT_THUMB;
        bool combo_pressed = (pState->Gamepad.wButtons & combo) == combo;

        if (combo_pressed && !g_combo_pressed_last) {
            g_overlay_focused = !g_overlay_focused;
            LogInfo(std::string("Top-bar overlay toggled via Gamepad (L3+R3): ") + (g_overlay_focused ? "ACTIVE" : "HIDDEN"));
        }
        g_combo_pressed_last = combo_pressed;

        // If overlay is focused, mask gamepad inputs from reaching Frostbite
        if (g_overlay_focused) {
            pState->Gamepad.wButtons = 0;
            pState->Gamepad.bLeftTrigger = 0;
            pState->Gamepad.bRightTrigger = 0;
            pState->Gamepad.sThumbLX = 0;
            pState->Gamepad.sThumbLY = 0;
            pState->Gamepad.sThumbRX = 0;
            pState->Gamepad.sThumbRY = 0;
        }
    }

    return result;
}

bool InstallXInputHooks() {
    const char* xinput_dlls[] = {
        "xinput1_4.dll",
        "xinput1_3.dll",
        "XInput9_1_0.dll"
    };

    HMODULE hXInput = nullptr;
    for (const char* dll_name : xinput_dlls) {
        hXInput = GetModuleHandleA(dll_name);
        if (!hXInput) {
            hXInput = LoadLibraryA(dll_name);
        }
        if (hXInput) {
            break;
        }
    }

    if (!hXInput) {
        LogWarn("No XInput DLL found to hook.");
        return false;
    }

    void* target = reinterpret_cast<void*>(GetProcAddress(hXInput, "XInputGetState"));
    if (!target) {
        LogWarn("Failed to find XInputGetState export.");
        return false;
    }

    if (MH_CreateHook(target, reinterpret_cast<void*>(&Hooked_XInputGetState),
                      reinterpret_cast<void**>(&g_original_XInputGetState)) == MH_OK) {
        MH_EnableHook(target);
        LogInfo("XInputGetState hooked successfully for gamepad navigation & input masking.");
        return true;
    }

    LogError("MinHook failed to hook XInputGetState.");
    return false;
}

void RemoveXInputHooks() {
    if (g_original_XInputGetState) {
        MH_DisableHook(reinterpret_cast<void*>(g_original_XInputGetState));
        g_original_XInputGetState = nullptr;
    }
}

bool IsOverlayFocused() {
    return g_overlay_focused;
}

void SetOverlayFocused(bool focused) {
    g_overlay_focused = focused;
}

bool IsStatusPillVisible() {
    return g_status_pill_visible;
}

void SetStatusPillVisible(bool visible) {
    g_status_pill_visible = visible;
}

} // namespace f22
