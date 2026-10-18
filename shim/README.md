# FIFA 22 Multiplayer Emulator Network Shim (`dxgi.dll`)

A high-performance, 64-bit dynamic library proxy and network interceptor for FIFA 22 (Frostbite 3, DirtySDK).

## Features
- **Dynamic DXGI Proxying:** Intercepts `CreateDXGIFactory`, `CreateDXGIFactory1`, `CreateDXGIFactory2`, `DXGIDeclareAdapterRemovalSupport`, and `DXGIGetDebugInterface1` forwarded dynamically to `C:\Windows\System32\dxgi.dll`.
- **WinSock Loopback Redirection:** Intercepts `getaddrinfo`, `GetAddrInfoW`, `connect`, and `WSAConnect` to automatically reroute traffic destined for:
  - `gosredirector.ea.com` (Port `42127`)
  - Core Blaze Server (Port `42128`)
  - Telemetry Sinks (`river.data.ea.com`, `pin-river.data.ea.com`)
  - EA backend domains (`*.ea.com`, `*.ea.net`, `*.dice.se`)
  Rerouted to the configured emulator host (default: `127.0.0.1`).
- **SSL / TLS Certificate Bypass:** Hooks `CertVerifyCertificateChainPolicy` in `crypt32.dll` and performs in-memory pattern scanning for DirtySDK `ProtoSSL` verification routines to allow self-signed certificates without Windows Certificate Authority errors.
- **Configurable Runtime:** Reads `f22_emu.ini` on startup for IP, port, and debug logging configuration.
- **Diagnostics:** Logs all intercepted sockets, lookups, and hook operations to `f22_shim.log` and `OutputDebugStringA`.

## Building

### Requirements
- CMake 3.20+
- Ninja build tool
- MinGW-W64 GCC 14+ or Clang/LLVM (MSVCRT target)

### Build Commands
```powershell
# From the repository root:
cmake -B shim/build -S shim -G Ninja
cmake --build shim/build
```

The resulting `shim/build/dxgi.dll` is statically linked against `libgcc` and `libstdc++`, requiring only standard Windows system DLLs (`KERNEL32.dll`, `msvcrt.dll`, `WS2_32.dll`).

## Deployment
1. Copy `shim/build/dxgi.dll` into the FIFA 22 game root folder (alongside `FIFA22.exe`).
2. Copy `shim/f22_emu.ini` into the same folder.
3. Configure `EmulatorHost` in `f22_emu.ini` if connecting to a remote server on your LAN or WAN.
4. Launch `FIFA22.exe` and check `f22_shim.log` for connection and redirection diagnostics.
