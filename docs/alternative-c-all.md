# Alternative C — Complete (Skills + UI + .bat + Explanation)

## REA Skills Used (all 5 from rea-main/.agents/skills)
- reverse-engineer-anything: read SKILL.md; doctor (healthy=false — missing Ghidra/IDA/Hopper, .claude.json unregistered); binary check (592MB unmodified)
- rea-analysis-optimization, rea-tool-design, rea-ci-tests-audit, rea-changelog-update: read SKILL.md; applied to session

## REA Findings (no binary broken)
- FIFA22.exe exists at G:\Games\FIFA 22\FIFA22.exe (592MB)
- No disk writes, no hooks installed, no VirtualProtect
- REA needs setup (Ghidra + .claude.json register) for deep disassembly; not required for Alternative C

## Companion App (Alternative C — no dxgi.dll)
- tools/companion_app.py — REST client (status/create/join)
- tools/companion_ui.py — Tkinter window (room code + ping pill)
- Connects to existing emulator control server 127.0.0.1:42129

## Launch Script
- start_all.bat — starts server via start_emulator.ps1, waits 3s, launches companion_ui.py

## Game Protection
- FIFA22.exe never modified
- No DirectX hooks, no WinSock detours, no input masking
- All traffic via localhost TCP to emulator (already running: ports 42127/42128/42129/443)

## End-to-End
User runs start_all.bat → server listens → companion connects → room codes manage matchmaking → game runs unmodified.
