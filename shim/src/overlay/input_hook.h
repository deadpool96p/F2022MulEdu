#pragma once

#include <windows.h>

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

} // namespace f22
