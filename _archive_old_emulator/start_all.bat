@echo off
taskkill /F /IM FIFA22.exe /T >nul 2>&1
taskkill /F /IM emulator-server.exe /T >nul 2>&1
ping 127.0.0.1 -n 2 >nul
del /Q "F:\Fifa22Multiplayer\emulator_server.log" >nul 2>&1
del /Q "G:\Games\FIFA 22\f22_shim.log" >nul 2>&1
copy /Y "F:\Fifa22Multiplayer\shim\build\dxgi.dll" "G:\Games\FIFA 22\dxgi.dll" >nul 2>&1
echo Starting Emulator Server...
:: cmd /k keeps the window open if the server crashes, so the error is
:: readable instead of the window vanishing instantly.
start "Emulator Server" cmd /k "F:\Fifa22Multiplayer\target\release\emulator-server.exe"
echo Waiting for server to bind ports...
ping 127.0.0.1 -n 6 > nul
echo Launching FIFA 22...
cd /D "G:\Games\FIFA 22"
start "" "FIFA22.exe"
exit