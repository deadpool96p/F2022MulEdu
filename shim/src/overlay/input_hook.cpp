#include "input_hook.h"
#include "f22_shim.h"
#include <MinHook.h>
#include <xinput.h>
#include <imgui.h>
#include <mutex>
#include <cstring>

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
    if (msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN) {
        bool is_repeat = (lParam & (1 << 30)) != 0;
        if (!is_repeat) {
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

// Remote Gamepad State (Slot 1)
static std::mutex g_pad_mutex;
static XINPUT_GAMEPAD g_remote_gamepad{};
static bool g_remote_peer_connected = false;
static std::string g_remote_peer_name = "Friend";
static uint32_t g_remote_packet_number = 0;
static uint64_t g_remote_last_seen = 0;
static uint64_t g_last_broadcast_time = 0;

void SetRemotePeerGamepad(const XINPUT_GAMEPAD& pad) {
    std::lock_guard<std::mutex> lock(g_pad_mutex);
    g_remote_gamepad = pad;
    g_remote_packet_number++;
    g_remote_last_seen = GetTickCount64();
    g_remote_peer_connected = true;
}

void SetRemotePeerConnected(bool connected, const std::string& peer_name) {
    std::lock_guard<std::mutex> lock(g_pad_mutex);
    g_remote_peer_connected = connected;
    if (!peer_name.empty()) {
        g_remote_peer_name = peer_name;
    }
    if (!connected) {
        std::memset(&g_remote_gamepad, 0, sizeof(g_remote_gamepad));
    }
}

bool IsRemotePeerConnected() {
    std::lock_guard<std::mutex> lock(g_pad_mutex);
    if (!g_remote_peer_connected) return false;
    if (g_remote_last_seen > 0 && (GetTickCount64() - g_remote_last_seen > 4000)) {
        return false;
    }
    return true;
}

std::string GetRemotePeerName() {
    std::lock_guard<std::mutex> lock(g_pad_mutex);
    return g_remote_peer_name;
}

static int16_t ApplyDeadzone(int16_t value, int16_t deadzone) {
    if (value > deadzone) {
        float norm = static_cast<float>(value - deadzone) / (32767.0f - deadzone);
        return static_cast<int16_t>(norm * 32767.0f);
    } else if (value < -deadzone) {
        float norm = static_cast<float>(value + deadzone) / (32768.0f - deadzone);
        return static_cast<int16_t>(norm * 32768.0f);
    }
    return 0;
}

XINPUT_STATE GetSafeNormalizedRemoteGamepadState() {
    std::lock_guard<std::mutex> lock(g_pad_mutex);
    XINPUT_STATE state{};
    state.dwPacketNumber = g_remote_packet_number;
    state.Gamepad = g_remote_gamepad;

    // Standard thumbstick deadzones
    state.Gamepad.sThumbLX = ApplyDeadzone(state.Gamepad.sThumbLX, XINPUT_GAMEPAD_LEFT_THUMB_DEADZONE);
    state.Gamepad.sThumbLY = ApplyDeadzone(state.Gamepad.sThumbLY, XINPUT_GAMEPAD_LEFT_THUMB_DEADZONE);
    state.Gamepad.sThumbRX = ApplyDeadzone(state.Gamepad.sThumbRX, XINPUT_GAMEPAD_RIGHT_THUMB_DEADZONE);
    state.Gamepad.sThumbRY = ApplyDeadzone(state.Gamepad.sThumbRY, XINPUT_GAMEPAD_RIGHT_THUMB_DEADZONE);

    if (state.Gamepad.bLeftTrigger < XINPUT_GAMEPAD_TRIGGER_THRESHOLD) {
        state.Gamepad.bLeftTrigger = 0;
    }
    if (state.Gamepad.bRightTrigger < XINPUT_GAMEPAD_TRIGGER_THRESHOLD) {
        state.Gamepad.bRightTrigger = 0;
    }

    return state;
}

void BroadcastLocalGamepad(const XINPUT_GAMEPAD& pad) {
    uint64_t now = GetTickCount64();
    // Rate limit gamepad broadcasting to 60Hz (~16ms) to avoid saturating network
    if (now - g_last_broadcast_time >= 15) {
        g_last_broadcast_time = now;
        TransmitNetcodeGamepadPacket(pad);
    }
}

static DWORD WINAPI Hooked_XInputGetState(DWORD dwUserIndex, XINPUT_STATE* pState) {
    if (!pState) {
        return ERROR_BAD_ARGUMENTS;
    }

    // Slot 1: Crash-Proof Virtual Remote Controller for Friend
    if (dwUserIndex == 1) {
        if (IsRemotePeerConnected()) {
            *pState = GetSafeNormalizedRemoteGamepadState();
            return ERROR_SUCCESS;
        }
        return ERROR_DEVICE_NOT_CONNECTED;
    }

    if (!g_original_XInputGetState) {
        return ERROR_DEVICE_NOT_CONNECTED;
    }

    DWORD result = g_original_XInputGetState(dwUserIndex, pState);
    if (result == ERROR_SUCCESS) {
        if (dwUserIndex == 0) {
            // Check L3 + R3 (Thumb buttons combo)
            WORD combo = XINPUT_GAMEPAD_LEFT_THUMB | XINPUT_GAMEPAD_RIGHT_THUMB;
            bool combo_pressed = (pState->Gamepad.wButtons & combo) == combo;

            if (combo_pressed && !g_combo_pressed_last) {
                g_overlay_focused = !g_overlay_focused;
                LogInfo(std::string("Top-bar overlay toggled via Gamepad (L3+R3): ") + (g_overlay_focused ? "ACTIVE" : "HIDDEN"));
            }
            g_combo_pressed_last = combo_pressed;

            // Transmit local physical gamepad state to connected peer
            BroadcastLocalGamepad(pState->Gamepad);
        }

        // Only mask gamepad buttons if the user is actively typing in the overlay's text field
        // NEVER zero out analog sticks or triggers, so gameplay and menu navigation remain smooth!
        if (g_overlay_focused && ImGui::GetCurrentContext() && ImGui::GetIO().WantTextInput) {
            pState->Gamepad.wButtons &= ~(XINPUT_GAMEPAD_A | XINPUT_GAMEPAD_B | XINPUT_GAMEPAD_X | XINPUT_GAMEPAD_Y |
                                          XINPUT_GAMEPAD_DPAD_UP | XINPUT_GAMEPAD_DPAD_DOWN |
                                          XINPUT_GAMEPAD_DPAD_LEFT | XINPUT_GAMEPAD_DPAD_RIGHT);
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
