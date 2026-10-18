# Alternative C Plan: External Companion App (No dxgi.dll)

## Why
REInvestigation found: dxgi.dll overlay is fragile (DX11/12 hooks, WndProc masking, XInput interception). External app avoids all of this.

## How
- Use existing emulator-server REST/WebSocket on 127.0.0.1:42129 (control_server.rs)
- Endpoints: POST /api/v1/rooms/create, POST /api/v1/rooms/join, GET /api/v1/status
- Companion runs as separate process; FIFA22.exe untouched

## Game Safety
- No writes to G:\Games\FIFA 22\FIFA22.exe
- No hooks, no memory patches
- Companion communicates over localhost TCP only

## Status
Plan created; REA skills invoked safely; no binary changes.
