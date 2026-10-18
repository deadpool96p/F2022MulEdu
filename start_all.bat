@echo off
REM FIFA 22 Multiplayer — Start Backend + Companion UI (Alternative C)
REM No dxgi.dll hook needed. Game untouched.

echo [*] Checking emulator server ports...

REM Start server (reuse existing start_emulator.ps1 logic in .bat form)
start powershell -ExecutionPolicy Bypass -File start_emulator.ps1

REM Wait briefly for listeners (42127, 42128, 42129, 443, 17498, 3659)
timeout /t 3 /nobreak >nul

echo [*] Starting Companion UI (Alternative C — no injection)...
python tools\companion_ui.py

echo [*] Done. Server and UI running. Game (FIFA22.exe) untouched.
