# Alternative C — External Companion App (No dxgi.dll)

## What it is
Instead of injecting a proxy `dxgi.dll` into `FIFA22.exe` to draw an overlay, we run a separate desktop app (Python/Tkinter or browser) that talks to the emulator's REST control server on `127.0.0.1:42129`.

## What it does
- **Create Room:** Sends `POST /api/v1/rooms/create` to get a 6-character code (e.g. `XY79K2`).
- **Join Room:** Sends `POST /api/v1/rooms/join` with target code.
- **Status:** Reads `GET /api/v1/status` for ping (ms), transport mode (`P2P` or `RELAY`), NAT state.
- **No injection:** The companion never touches `FIFA22.exe` memory, DirectX, or WinSock.

## How it works
```
+----------------+          REST/WebSocket          +------------------+
| Companion App  |  <--->  127.0.0.1:42129  <---->| Emulator Server  |
| (Tkinter/HTML) |         (existing control)     | (control_server) |
+----------------+                                 +------------------+
                          +------------------+
                          | FIFA22.exe (game) |
                          | Unmodified        |
                          +------------------+
```

## Why it's better
- No DXGI hook = no crash on resolution change, no DirectX version mismatch.
- No `WndProc` mask = keyboard/mouse not blocked accidentally.
- No `XInputGetState` interception = controller input stays native.
- Game binary (`G:\Games\FIFA 22\FIFA22.exe`) never written or patched.

## Files built
- `tools/companion_app.py` — REST client (no UI, for scripting)
- `tools/companion_ui.py` — Tkinter window with room code + status pill
- `docs/alternative-c-plan.md` — design spec
- `docs/alternative-c-explained.md` — this explanation

## Status
Companion UI created; game untouched; all 5 REA skills invoked safely.
