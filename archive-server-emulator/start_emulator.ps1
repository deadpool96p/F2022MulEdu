# Launch script for FIFA 22 Emulator Server

$mingwPath = "C:\Users\soham\AppData\Local\Microsoft\WinGet\Packages\BrechtSanders.WinLibs.POSIX.MSVCRT.LLVM_Microsoft.Winget.Source_8wekyb3d8bbwe\mingw64\bin"
if ($env:PATH -notlike "*$mingwPath*") {
    $env:PATH = "$mingwPath;$env:PATH"
}

if (Test-Path "target\release\emulator-server.exe") {
    Write-Host "[*] Launching FIFA 22 Emulator Server (Release)..." -ForegroundColor Cyan
    & "target\release\emulator-server.exe"
} else {
    Write-Host "[*] Building and launching FIFA 22 Emulator Server..." -ForegroundColor Cyan
    cargo run --package emulator-server --release
}
