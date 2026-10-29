<#
.SYNOPSIS
    Locates or downloads the redistributable 64-bit Valve Steamworks steam_api64.dll and steam_appid.txt.

.DESCRIPTION
    Checks common local Steam installation/library folders for an existing 64-bit steam_api64.dll.
    If none is found locally, downloads the official Steamworks SDK redistributable steam_api64.dll
    from the Valve-authorized Steamworks redistributable repository.
#>

[CmdletBinding()]
param (
    [string]$TargetDir = ""
)

$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = Split-Path -Parent $ScriptDir

if ([string]::IsNullOrWhiteSpace($TargetDir)) {
    $TargetDir = Join-Path $RepoRoot "shim\vendor\steam\bin\win64"
}

if (-not (Test-Path $TargetDir)) {
    New-Item -ItemType Directory -Path $TargetDir -Force | Out-Null
}

$DestDll = Join-Path $TargetDir "steam_api64.dll"
$AppIdFile = Join-Path $TargetDir "steam_appid.txt"

# Always write steam_appid.txt (480 for Spacewar)
Set-Content -Path $AppIdFile -Value "480" -NoNewline
Write-Host "[Steam] Configured AppID: 480 (Spacewar) -> $AppIdFile" -ForegroundColor Cyan

if (Test-Path $DestDll) {
    Write-Host "[Steam] steam_api64.dll already present in $TargetDir" -ForegroundColor Green
    exit 0
}

Write-Host "[Steam] Searching local Steam directories for 64-bit steam_api64.dll..." -ForegroundColor Yellow

$CandidatePaths = @(
    "G:\Games\FIFA 22\steam_api64.dll",
    "G:\SteamLibrary\steamapps\common\ForzaHorizon4\steam_api64.dll",
    "G:\SteamLibrary\steamapps\common\Call of Duty Modern Warfare\steam_api64.dll",
    "G:\SteamLibrary\steamapps\common\Metro Exodus\steam_api64.dll",
    "C:\Program Files (x86)\Steam\steamapps\common\*\steam_api64.dll",
    "D:\SteamLibrary\steamapps\common\*\steam_api64.dll",
    "E:\SteamLibrary\steamapps\common\*\steam_api64.dll",
    "G:\SteamLibrary\steamapps\common\*\steam_api64.dll"
)

$FoundDll = $null
foreach ($pattern in $CandidatePaths) {
    $files = Get-ChildItem -Path $pattern -ErrorAction SilentlyContinue
    foreach ($file in $files) {
        if ($file.Length -gt 150000) {
            $FoundDll = $file.FullName
            break
        }
    }
    if ($FoundDll) { break }
}

if ($FoundDll) {
    Write-Host "[Steam] Found local Valve steam_api64.dll: $FoundDll" -ForegroundColor Green
    Copy-Item -Path $FoundDll -Destination $DestDll -Force
    Write-Host "[Steam] Copied to $DestDll" -ForegroundColor Green
    exit 0
}

# If not found locally, download from official Valve Steamworks redistributable repository
Write-Host "[Steam] Local copy not found. Downloading steam_api64.dll from Steamworks redist mirror..." -ForegroundColor Yellow
$DownloadUrl = "https://raw.githubusercontent.com/rlabrecque/Steamworks.NET/master/Plugins/x86_64/steam_api64.dll"

try {
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12 -bor [Net.SecurityProtocolType]::Tls13
    Invoke-WebRequest -Uri $DownloadUrl -OutFile $DestDll -UseBasicParsing
    Write-Host "[Steam] Downloaded steam_api64.dll successfully -> $DestDll" -ForegroundColor Green
} catch {
    Write-Error "Failed to fetch steam_api64.dll: $_"
    exit 1
}
