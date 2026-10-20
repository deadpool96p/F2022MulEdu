<#
.SYNOPSIS
    Clean Deploy, Cache Reset & Launch Script for FIFA 22 Multiplayer Emulator.

.DESCRIPTION
    1. Gracefully terminates any existing FIFA 22 and EA background processes.
    2. Backs up and clears local settings & cache to purge any stale EOS/offline flags.
    3. Deploys the freshly compiled shim (dxgi.dll proxy with full rerouting and verified self-check).
    4. Launches the local Blaze & HTTP Emulator Server in release mode.
    5. Launches FIFA 22 cleanly.
#>

param(
    [string]$GameDir = "G:\Games\FIFA 22"
)

$ErrorActionPreference = "Continue"

Write-Host "=============================================================" -ForegroundColor Cyan
Write-Host "  FIFA 22 Clean Deploy, Cache Purge & Emulator Launcher     " -ForegroundColor Cyan
Write-Host "=============================================================" -ForegroundColor Cyan

# 1. Terminate conflicting processes
Write-Host "[1/5] Terminating existing FIFA 22, EA, and emulator processes..." -ForegroundColor Yellow
$processNames = @("FIFA22", "EABackgroundService", "EADesktop", "Origin", "OriginClientService", "OriginWebHelperService", "emulator-server")
foreach ($proc in $processNames) {
    $running = Get-Process -Name $proc -ErrorAction SilentlyContinue
    if ($running) {
        Write-Host "  [-] Killing process: $proc (PID $($running.Id -join ', '))" -ForegroundColor Red
        Stop-Process -Name $proc -Force -ErrorAction SilentlyContinue
    }
}
Start-Sleep -Milliseconds 800

# 2. Clear client-side settings and cache
Write-Host "[2/5] Purging local FIFA 22 cache and stale settings..." -ForegroundColor Yellow
$docPaths = @(
    "$env:USERPROFILE\Documents\FIFA 22",
    "$env:USERPROFILE\OneDrive\Documents\FIFA 22"
)
foreach ($docDir in $docPaths) {
    if (Test-Path $docDir) {
        $settingsDir = Join-Path $docDir "settings"
        $cacheDir = Join-Path $docDir "filesystemcache"
        $backupDir = Join-Path $docDir "backup_eos_$(Get-Date -Format 'yyyyMMdd_HHmmss')"

        if (Test-Path $settingsDir) {
            Write-Host "  [+] Backing up settings from: $settingsDir" -ForegroundColor Gray
            New-Item -ItemType Directory -Path $backupDir -Force | Out-Null
            Copy-Item -Path "$settingsDir\*" -Destination $backupDir -Recurse -Force -ErrorAction SilentlyContinue

            Write-Host "  [-] Clearing settings files..." -ForegroundColor Gray
            Remove-Item -Path "$settingsDir\*" -Recurse -Force -ErrorAction SilentlyContinue
        }
        if (Test-Path $cacheDir) {
            Write-Host "  [-] Clearing filesystemcache..." -ForegroundColor Gray
            Remove-Item -Path "$cacheDir\*" -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
}

# 3. Deploy updated shim binaries
Write-Host "[3/5] Deploying updated shim binaries to $GameDir..." -ForegroundColor Yellow
$RepoRoot = Split-Path -Parent $PSScriptRoot
$BuiltDll = Join-Path $RepoRoot "shim\build\dxgi.dll"
$TargetDll = Join-Path $GameDir "dxgi.dll"
$TargetOrgDll = Join-Path $GameDir "dxgi_org.dll"

if (Test-Path $BuiltDll) {
    Copy-Item -Path $BuiltDll -Destination $TargetDll -Force
    Write-Host "  [+] Deployed fresh dxgi.dll ($((Get-Item $TargetDll).Length) bytes)" -ForegroundColor Green
} else {
    Write-Error "Could not find built dxgi.dll at $BuiltDll!"
    exit 1
}

# Deploy system dxgi as dxgi_org.dll as a guaranteed fallback
$SysDxgi = "C:\Windows\System32\dxgi.dll"
if (Test-Path $SysDxgi) {
    Copy-Item -Path $SysDxgi -Destination $TargetOrgDll -Force
    Write-Host "  [+] Deployed System32 dxgi.dll -> $TargetOrgDll" -ForegroundColor Green
}

# Deploy config and cert assets
$IniSrc = Join-Path $RepoRoot "shim\f22_emu.ini"
$IniDst = Join-Path $GameDir "f22_emu.ini"
if (Test-Path $IniSrc) {
    Copy-Item -Path $IniSrc -Destination $IniDst -Force
    Write-Host "  [+] Updated f22_emu.ini" -ForegroundColor Green
}

$ModSrc = Join-Path $RepoRoot "f22_ca_modulus.bin"
$ModDst = Join-Path $GameDir "f22_ca_modulus.bin"
if (Test-Path $ModSrc) {
    Copy-Item -Path $ModSrc -Destination $ModDst -Force
}

$CertSrc = Join-Path $RepoRoot "f22_ca.crt"
$CertDst = Join-Path $GameDir "f22_ca.crt"
if (Test-Path $CertSrc) {
    Copy-Item -Path $CertSrc -Destination $CertDst -Force
}

# Remove old logs
$TargetLog = Join-Path $GameDir "f22_shim.log"
if (Test-Path $TargetLog) {
    Remove-Item -Path $TargetLog -Force -ErrorAction SilentlyContinue
    Write-Host "  [-] Cleared old log: f22_shim.log" -ForegroundColor Gray
}
$ServerLog = Join-Path $RepoRoot "emulator_server.log"
if (Test-Path $ServerLog) {
    Remove-Item -Path $ServerLog -Force -ErrorAction SilentlyContinue
    Write-Host "  [-] Cleared old log: emulator_server.log" -ForegroundColor Gray
}

# 4. Start the emulator server
Write-Host "[4/5] Starting local Blaze & HTTP Emulator Server (Release)..." -ForegroundColor Yellow
Push-Location $RepoRoot
Start-Process powershell -ArgumentList "-ExecutionPolicy Bypass -NoExit -Command `"Write-Host 'Starting Emulator Server...'; target\release\emulator-server.exe`""
Pop-Location

Write-Host "  [*] Waiting 3 seconds for emulator server sockets to bind..." -ForegroundColor Gray
Start-Sleep -Seconds 3

# 5. Launch FIFA 22
Write-Host "[5/5] Launching FIFA 22 (build 1.0.77.45722)..." -ForegroundColor Green
$ExePath = Join-Path $GameDir "FIFA22.exe"
if (Test-Path $ExePath) {
    Push-Location $GameDir
    Start-Process -FilePath $ExePath -WorkingDirectory $GameDir
    Pop-Location
    Write-Host "  [+] FIFA 22 launched successfully!" -ForegroundColor Green
} else {
    Write-Error "Could not find FIFA22.exe at $ExePath"
}

Write-Host "=============================================================" -ForegroundColor Cyan
Write-Host "  Ready! Monitor f22_shim.log and emulator server console.  " -ForegroundColor Cyan
Write-Host "=============================================================" -ForegroundColor Cyan
