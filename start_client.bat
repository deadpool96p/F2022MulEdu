@echo off
title FIFA 22 Clean-Room Multiplayer - Client Launcher
echo ============================================================
echo   FIFA 22 Multiplayer Client (Connecting to Host via Radmin)
echo ============================================================

REM 1. Auto-sync Steam Persona and configure Host Radmin IP
if exist "tools\sync_steam_profile.py" (
    python tools\sync_steam_profile.py --client
)

REM 2. Launch Companion UI
if exist "tools\companion_ui.py" (
    start "" python tools\companion_ui.py
)

REM 3. Launch FIFA 22 if in current folder
if exist "FIFA22.exe" (
    echo [*] Launching FIFA 22...
    start "" "FIFA22.exe"
) else (
    echo [*] Client configured. Please launch FIFA 22 now.
)
