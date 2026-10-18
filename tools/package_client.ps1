# Script to package client-side files for friends

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = Split-Path -Parent $ScriptDir

$PackageDir = Join-Path $RepoRoot "client_distribution"
$ZipPath = Join-Path $RepoRoot "FIFA22_Multiplayer_Client.zip"

Write-Host "[*] Packaging FIFA 22 Multiplayer Client for distribution..." -ForegroundColor Cyan

# Create staging directory
if (Test-Path $PackageDir) {
    Remove-Item -Path $PackageDir -Recurse -Force
}
New-Item -ItemType Directory -Path $PackageDir | Out-Null

# Copy required client files
Copy-Item (Join-Path $RepoRoot "shim\build\dxgi.dll") (Join-Path $PackageDir "dxgi.dll") -Force
Copy-Item (Join-Path $RepoRoot "shim\vendor\steam\bin\win64\steam_api64.dll") (Join-Path $PackageDir "steam_api64.dll") -Force
Copy-Item (Join-Path $RepoRoot "shim\vendor\steam\bin\win64\steam_appid.txt") (Join-Path $PackageDir "steam_appid.txt") -Force
Copy-Item (Join-Path $RepoRoot "shim\f22_emu.ini") (Join-Path $PackageDir "f22_emu.ini") -Force

# Create instructions for friend
$FriendReadme = @"
=====================================================================
 FIFA 22 Multiplayer Client (Plug & Play)
=====================================================================

HOW TO INSTALL (3 Easy Steps):

1. COPY ALL 4 FILES in this folder:
   - dxgi.dll
   - steam_api64.dll
   - steam_appid.txt
   - f22_emu.ini
   Paste them directly into your FIFA 22 game installation folder
   (the folder that contains FIFA22.exe, e.g. C:\Games\FIFA 22).

2. CONFIGURE YOUR FRIEND'S HOST IP:
   Open 'f22_emu.ini' with Notepad and change:
   
   EmulatorHost = <Host's IP Address>
   
   (Ask your friend for their Radmin VPN / Tailscale / Public IP).

3. PLAY!
   - Make sure your Steam client is open in the background.
   - Launch FIFA 22.
   - Press F1 to view room codes, or Shift+Tab to invite/join via Steam!
   - Select 'Online Friendlies' in the FIFA 22 main menu to play.
=====================================================================
"@

Set-Content -Path (Join-Path $PackageDir "README_FRIEND.txt") -Value $FriendReadme -Encoding UTF8

# Compress to ZIP
if (Test-Path $ZipPath) {
    Remove-Item -Path $ZipPath -Force
}
Compress-Archive -Path "$PackageDir\*" -DestinationPath $ZipPath -Force

# Cleanup temp staging folder
Remove-Item -Path $PackageDir -Recurse -Force

$zipItem = Get-Item $ZipPath
Write-Host "[+] Successfully created: $ZipPath" -ForegroundColor Green
Write-Host "    Size: $([math]::Round($zipItem.Length / 1MB, 2)) MB ($($zipItem.Length) bytes)" -ForegroundColor DarkCyan
Write-Host "    Send this ZIP file to your friend!" -ForegroundColor Green
