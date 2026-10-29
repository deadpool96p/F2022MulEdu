# clean_deploy_and_play.ps1
#
# Deploys the freshly-built dxgi.dll shim into the FIFA 22 game directory,
# (re)starts the emulator server, then launches the game so the background
# root-CA patcher has time to patch EAWebKit.dll before the QoS handshake.

$ErrorActionPreference = "Stop"
$gameDir = "G:\Games\FIFA 22"
$workDir = "F:\Fifa22Multiplayer"
$shimBuild = Join-Path $workDir "shim\build"

Write-Host "=== Stopping any running emulator-server ===" -ForegroundColor Yellow
Get-Process emulator-server -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Seconds 1

Write-Host "=== Copying fresh dxgi.dll shim to $gameDir ===" -ForegroundColor Yellow
$src = Join-Path $shimBuild "dxgi.dll"
if (-not (Test-Path $src)) {
    Write-Error "Built shim not found at $src -- run 'cmake --build shim/build' first."
    exit 1
}
Copy-Item $src (Join-Path $gameDir "dxgi.dll") -Force
Write-Host "  dxgi.dll -> $gameDir\dxgi.dll" -ForegroundColor Green

# Make sure the modulus file is present in the game dir for the shim to load.
$modSrc = Join-Path $workDir "f22_ca_modulus.bin"
$modDst = Join-Path $gameDir "f22_ca_modulus.bin"
if (Test-Path $modSrc) {
    Copy-Item $modSrc $modDst -Force
    Write-Host "  f22_ca_modulus.bin -> $gameDir\f22_ca_modulus.bin" -ForegroundColor Green
} else {
    Write-Host "  WARN: f22_ca_modulus.bin not found in work dir; the CA patcher will wait for it." -ForegroundColor Yellow
}

Write-Host "=== Launching emulator server (background) ===" -ForegroundColor Yellow
$emuExe = Join-Path $workDir "target\release\emulator-server.exe"
if (-not (Test-Path $emuExe)) {
    Write-Host "  Building emulator-server..." -ForegroundColor Cyan
    cargo build --package emulator-server --release
    $emuExe = Join-Path $workDir "target\release\emulator-server.exe"
}
$emuLog = Join-Path $workDir "emulator_server.log"
Start-Process -FilePath $emuExe -WorkingDirectory $workDir -RedirectStandardOutput $emuLog -RedirectStandardError (Join-Path $workDir "emulator_server.err.log") -NoNewWindow

Write-Host "  Waiting for emulator server to bind ports..." -ForegroundColor Cyan
Start-Sleep -Seconds 4

Write-Host "=== Launching FIFA 22 ===" -ForegroundColor Yellow
$fifaExe = Join-Path $gameDir "FIFA22.exe"
Start-Process -FilePath $fifaExe -WorkingDirectory $gameDir
Write-Host "  FIFA 22 launched. Watch f22_shim.log for the CA-patcher and emulator_server.log for postAuth." -ForegroundColor Green
Write-Host "=== Done ===" -ForegroundColor Cyan