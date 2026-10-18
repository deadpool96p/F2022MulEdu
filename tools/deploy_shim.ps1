<#
.SYNOPSIS
    Deploys or uninstalls the FIFA 22 Multiplayer Emulator Proxy Shim (dxgi.dll & f22_emu.ini).

.DESCRIPTION
    Safely copies the compiled 64-bit dxgi.dll proxy and f22_emu.ini configuration file
    into the target FIFA 22 game installation directory, or cleanly removes them without
    modifying any original Frostbite engine binaries or assets.

.PARAMETER GameDir
    Path to the FIFA 22 game installation root directory. Default is "G:\Games\FIFA 22".

.PARAMETER Install
    Copies dxgi.dll and f22_emu.ini into GameDir.

.PARAMETER Uninstall
    Removes dxgi.dll, f22_emu.ini, and f22_shim.log from GameDir.

.EXAMPLE
    .\tools\deploy_shim.ps1 -Install
    .\tools\deploy_shim.ps1 -Uninstall
    .\tools\deploy_shim.ps1 -Install -GameDir "D:\Games\FIFA 22"
#>

[CmdletBinding(DefaultParameterSetName = 'Install')]
param (
    [Parameter(Position = 0)]
    [string]$GameDir = "G:\Games\FIFA 22",

    [Parameter(ParameterSetName = 'Install')]
    [switch]$Install,

    [Parameter(ParameterSetName = 'Uninstall')]
    [switch]$Uninstall
)

$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = Split-Path -Parent $ScriptDir

$ShimDll = Join-Path $RepoRoot "shim\build\dxgi.dll"
$ShimIni = Join-Path $RepoRoot "shim\f22_emu.ini"

$TargetDll = Join-Path $GameDir "dxgi.dll"
$TargetIni = Join-Path $GameDir "f22_emu.ini"
$TargetLog = Join-Path $GameDir "f22_shim.log"

if (-not (Test-Path $GameDir)) {
    Write-Warning "Target game directory does not exist: $GameDir"
    Write-Warning "Ensure FIFA 22 is installed or pass the correct path using -GameDir."
    exit 1
}

$ExePath = Join-Path $GameDir "FIFA22.exe"
if (-not (Test-Path $ExePath)) {
    Write-Warning "FIFA22.exe was not found in $GameDir. Please verify the game installation path."
}

if ($Uninstall) {
    Write-Host "[Uninstall] Removing emulator shim files from: $GameDir" -ForegroundColor Yellow

    if (Test-Path $TargetDll) {
        Remove-Item -Path $TargetDll -Force
        Write-Host "  [-] Removed: dxgi.dll" -ForegroundColor Green
    } else {
        Write-Host "  [.] dxgi.dll was not present." -ForegroundColor Gray
    }

    if (Test-Path $TargetIni) {
        Remove-Item -Path $TargetIni -Force
        Write-Host "  [-] Removed: f22_emu.ini" -ForegroundColor Green
    } else {
        Write-Host "  [.] f22_emu.ini was not present." -ForegroundColor Gray
    }

    if (Test-Path $TargetLog) {
        Remove-Item -Path $TargetLog -Force
        Write-Host "  [-] Removed: f22_shim.log" -ForegroundColor Green
    }

    Write-Host "[Uninstall] Done. Game installation restored to original clean state." -ForegroundColor Cyan
    exit 0
}

# Default action: Install
Write-Host "[Install] Deploying emulator shim to: $GameDir" -ForegroundColor Cyan

if (-not (Test-Path $ShimDll)) {
    Write-Host "[Build] Compiled dxgi.dll not found. Compiling with CMake..." -ForegroundColor Yellow
    $msvcrtBin = "C:\Users\soham\AppData\Local\Microsoft\WinGet\Packages\BrechtSanders.WinLibs.POSIX.MSVCRT.LLVM_Microsoft.Winget.Source_8wekyb3d8bbwe\mingw64\bin"
    $env:PATH = "$msvcrtBin;C:\Program Files\LLVM\bin;$env:PATH"

    Push-Location (Join-Path $RepoRoot "shim")
    try {
        cmake -B build -S . -G Ninja
        cmake --build build
    } finally {
        Pop-Location
    }
}

if (-not (Test-Path $ShimDll)) {
    Write-Error "Build failed or dxgi.dll not generated at: $ShimDll"
    exit 1
}

# Copy DLL
Copy-Item -Path $ShimDll -Destination $TargetDll -Force
Write-Host "  [+] Installed: dxgi.dll -> $TargetDll" -ForegroundColor Green

# Copy INI if not present or prompt overwrite
if (-not (Test-Path $TargetIni)) {
    Copy-Item -Path $ShimIni -Destination $TargetIni -Force
    Write-Host "  [+] Installed: f22_emu.ini -> $TargetIni" -ForegroundColor Green
} else {
    Write-Host "  [.] Existing f22_emu.ini preserved (remove first to re-deploy default template)." -ForegroundColor Gray
}

Write-Host "=================================================================" -ForegroundColor Cyan
Write-Host " FIFA 22 Multiplayer Shim Installed Successfully!              " -ForegroundColor Green
Write-Host " Start emulator-server, then launch FIFA22.exe.                 " -ForegroundColor Green
Write-Host " Monitor diagnostics in: $TargetLog                              " -ForegroundColor Cyan
Write-Host "=================================================================" -ForegroundColor Cyan
