#pragma once

#include "f22_shim.h"
#include <windows.h>
#include <xinput.h>
#include <string>

namespace f22 {

// Initializes input hooks on the target game window
bool InstallInputHooks(HWND hWnd);
void RemoveInputHooks(HWND hWnd);

// Enables XInput hooking for gamepad navigation & input masking
bool InstallXInputHooks();
void RemoveXInputHooks();

// Returns true if the top-bar overlay has interactive input focus
bool IsOverlayFocused();
void SetOverlayFocused(bool focused);

// Returns true if the status pill is visible
bool IsStatusPillVisible();
void SetStatusPillVisible(bool visible);

// Crash-Proof Remote Gamepad Virtualization (Slot 1)
void SetRemotePeerGamepad(const XINPUT_GAMEPAD& pad);
void SetRemotePeerConnected(bool connected, const std::string& peer_name = "");
bool IsRemotePeerConnected();
std::string GetRemotePeerName();
XINPUT_STATE GetSafeNormalizedRemoteGamepadState();
void BroadcastLocalGamepad(const XINPUT_GAMEPAD& pad);

} // namespace f22
