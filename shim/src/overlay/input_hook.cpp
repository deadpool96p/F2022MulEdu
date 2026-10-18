#include "input_hook.h"
#include "f22_shim.h"
#include "../steam_bridge.hpp"
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
            // Only mask mouse clicks when mouse cursor is within the top bar (top 48 pixels)
            if (io.WantCaptureMouse) {
                POINT pt;
                if (GetCursorPos(&pt) && ScreenToClient(hWnd, &pt)) {
                    if (pt.y <= 48) {
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
                }
            }

            // ONLY mask keyboard events if user is actively typing in an ImGui text input box!
            if (io.WantTextInput) {
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

// 4-Player Remote Gamepad Slots (Slot 0 = Local/Host, Slots 1, 2, 3 = Remote Friends)
struct RemotePlayerSlot {
    bool connected = false;
    std::string name = "Friend";
    XINPUT_GAMEPAD gamepad{};
    uint32_t packet_number = 0;
    uint64_t last_seen = 0;
};

static std::mutex g_pad_mutex;
static RemotePlayerSlot g_remote_slots[4];
static uint64_t g_last_broadcast_time = 0;

void SetRemotePeerGamepad(int slot, const XINPUT_GAMEPAD& pad) {
    if (slot < 0 || slot >= 4) return;
    std::lock_guard<std::mutex> lock(g_pad_mutex);
    g_remote_slots[slot].gamepad = pad;
    g_remote_slots[slot].packet_number++;
    g_remote_slots[slot].last_seen = GetTickCount64();
    g_remote_slots[slot].connected = true;
}

void SetRemotePeerConnected(int slot, bool connected, const std::string& peer_name) {
    if (slot < 0 || slot >= 4) return;
    std::lock_guard<std::mutex> lock(g_pad_mutex);
    g_remote_slots[slot].connected = connected;
    if (!peer_name.empty()) {
        g_remote_slots[slot].name = peer_name;
    }
    if (connected) {
        g_remote_slots[slot].last_seen = GetTickCount64();
    } else {
        g_remote_slots[slot].last_seen = 0;
        std::memset(&g_remote_slots[slot].gamepad, 0, sizeof(XINPUT_GAMEPAD));
    }
}

bool IsRemotePeerConnected(int slot) {
    if (slot < 0 || slot >= 4) return false;
    std::lock_guard<std::mutex> lock(g_pad_mutex);
    if (!g_remote_slots[slot].connected) return false;
    if (g_remote_slots[slot].last_seen > 0 && (GetTickCount64() - g_remote_slots[slot].last_seen > 10000)) {
        return false;
    }
    return true;
}

std::string GetRemotePeerName(int slot) {
    if (slot < 0 || slot >= 4) return "Friend";
    std::lock_guard<std::mutex> lock(g_pad_mutex);
    return g_remote_slots[slot].name;
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

void BroadcastLocalGamepad(const XINPUT_GAMEPAD& pad) {
    uint64_t now = GetTickCount64();
    // Rate limit gamepad broadcasting to 60Hz (~16ms)
    if (now - g_last_broadcast_time >= 15) {
        g_last_broadcast_time = now;
        TransmitNetcodeGamepadPacket(pad);
    }
}

static void PollRemoteGamepadInputs() {
    if (SteamBridge::Instance().HasSteamNetworking()) {
        SteamBridge::Instance().RunCallbacks();
        uint32_t input_sz = 0;
        while (SteamBridge::Instance().IsP2PPacketAvailable(&input_sz, 1) && input_sz >= 16) {
            uint8_t p2p_buf[64];
            uint64_t remote_steam_id = 0;
            uint32_t read_bytes = 0;
            if (SteamBridge::Instance().ReadP2PPacket(p2p_buf, sizeof(p2p_buf), &read_bytes, &remote_steam_id, 1)) {
                if (read_bytes >= 16 && *reinterpret_cast<const uint16_t*>(p2p_buf) == 0xF220) {
                    if (p2p_buf[2] == 0x01) {
                        int slot = static_cast<int>(p2p_buf[3]);
                        if (slot <= 0 || slot >= 4) slot = 1;
                        XINPUT_GAMEPAD remote_pad{};
                        std::memcpy(&remote_pad, p2p_buf + 4, sizeof(XINPUT_GAMEPAD));
                        SetRemotePeerGamepad(slot, remote_pad);
                    }
                }
            }
        }
    }
}

static DWORD WINAPI Hooked_XInputGetState(DWORD dwUserIndex, XINPUT_STATE* pState) {
    if (!pState || dwUserIndex >= 4) {
        return ERROR_BAD_ARGUMENTS;
    }

    // Pump incoming Steam P2P controller inputs from friends every frame
    if (dwUserIndex == 0) {
        PollRemoteGamepadInputs();
    }

    // 1. ALWAYS query the real local physical controller FIRST!
    DWORD local_res = ERROR_DEVICE_NOT_CONNECTED;
    if (g_original_XInputGetState) {
        local_res = g_original_XInputGetState(dwUserIndex, pState);
    }

    if (local_res == ERROR_SUCCESS) {
        // Real physical controller is connected on this PC! Never block or zero it!
        if (dwUserIndex == 0) {
            WORD combo = XINPUT_GAMEPAD_LEFT_THUMB | XINPUT_GAMEPAD_RIGHT_THUMB;
            bool combo_pressed = (pState->Gamepad.wButtons & combo) == combo;

            if (combo_pressed && !g_combo_pressed_last) {
                g_overlay_focused = !g_overlay_focused;
                LogInfo(std::string("Top-bar overlay toggled via Gamepad (L3+R3): ") + (g_overlay_focused ? "ACTIVE" : "HIDDEN"));
            }
            g_combo_pressed_last = combo_pressed;

            // Broadcast local pad state to connected friends
            BroadcastLocalGamepad(pState->Gamepad);
        }

        // Only mask buttons if user is actively typing in a text field
        if (g_overlay_focused && ImGui::GetCurrentContext() && ImGui::GetIO().WantTextInput) {
            pState->Gamepad.wButtons &= ~(XINPUT_GAMEPAD_A | XINPUT_GAMEPAD_B | XINPUT_GAMEPAD_X | XINPUT_GAMEPAD_Y |
                                          XINPUT_GAMEPAD_DPAD_UP | XINPUT_GAMEPAD_DPAD_DOWN |
                                          XINPUT_GAMEPAD_DPAD_LEFT | XINPUT_GAMEPAD_DPAD_RIGHT);
        }

        return ERROR_SUCCESS;
    }

    // 2. If no local physical controller is connected on this slot, check if a remote friend is assigned to it!
    {
        std::lock_guard<std::mutex> lock(g_pad_mutex);
        if (g_remote_slots[dwUserIndex].connected) {
            uint64_t now = GetTickCount64();
            if (g_remote_slots[dwUserIndex].last_seen > 0 && (now - g_remote_slots[dwUserIndex].last_seen < 5000)) {
                pState->dwPacketNumber = g_remote_slots[dwUserIndex].packet_number;
                pState->Gamepad = g_remote_slots[dwUserIndex].gamepad;

                pState->Gamepad.sThumbLX = ApplyDeadzone(pState->Gamepad.sThumbLX, XINPUT_GAMEPAD_LEFT_THUMB_DEADZONE);
                pState->Gamepad.sThumbLY = ApplyDeadzone(pState->Gamepad.sThumbLY, XINPUT_GAMEPAD_LEFT_THUMB_DEADZONE);
                pState->Gamepad.sThumbRX = ApplyDeadzone(pState->Gamepad.sThumbRX, XINPUT_GAMEPAD_RIGHT_THUMB_DEADZONE);
                pState->Gamepad.sThumbRY = ApplyDeadzone(pState->Gamepad.sThumbRY, XINPUT_GAMEPAD_RIGHT_THUMB_DEADZONE);

                if (pState->Gamepad.bLeftTrigger < XINPUT_GAMEPAD_TRIGGER_THRESHOLD) {
                    pState->Gamepad.bLeftTrigger = 0;
                }
                if (pState->Gamepad.bRightTrigger < XINPUT_GAMEPAD_TRIGGER_THRESHOLD) {
                    pState->Gamepad.bRightTrigger = 0;
                }

                return ERROR_SUCCESS;
            }
        }
    }

    return ERROR_DEVICE_NOT_CONNECTED;
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
