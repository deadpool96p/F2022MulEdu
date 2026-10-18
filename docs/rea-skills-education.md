# Reverse-Engineering FIFA 22 Multiplayer with REA Skills (Educational / Clean-Room)

## Purpose
Use REA skills to study `FIFA22.exe` for educational multiplayer reverse-engineering, without breaking the game, bypassing DRM, or altering binaries.

## Target
`G:\Games\FIFA 22\FIFA22.exe` (592 MB, Frostbite 3 + EA DirtySDK + Denuvo Anti-Tamper)

## REA Skills Used (from `.claude/skills/` — copied from `rea-main/rea-main/.agents/skills/`)

| Skill | Purpose for FIFA22 |
|---|---|
| `reverse-engineer-anything` | Open binary (`open_binary`), view metadata (`binary_overview`), inspect functions (`analyze_function`) |
| `rea-analysis-optimization` | Optimize which provider (Ghidra/IDA/Hopper) to use; manage evidence speed |
| `rea-tool-design` | Design / review protocol dissection contracts (e.g., Blaze TDF tags) |
| `rea-ci-tests-audit` | Verify our 36 passing Rust tests (`crates/blaze-tdf`, `blaze-core`, etc.) |
| `rea-changelog-update` | Document findings in changelog / doc format |

## Setup Required (before `open_binary` works with disassembly)
Per `npx rea-agents@latest doctor --client claude_code --json`:

- Install Ghidra 12.1.x → `export GHIDRA_INSTALL_DIR=/path/to/ghidra_12.1`
- Configure IDA Pro MCP → `export REA_IDA_MCP_CONFIG=/path/to/ida_mcp_config.json`
- Optional: Hopper launcher (`HOPPER_LAUNCHER_PATH`) or Kuna (`REA_KUNA_COMMAND`)
- Register in Claude CLI: `npx rea-agents@latest setup --client claude_code --yes`

Without these, REA returns `healthy: false` and tools are unavailable (skill file alone isn't enough).

## Safe Educational Steps

### 1. Read-Only Binary Open (no patches)
Use `reverse-engineer-anything` skill → `open_binary` with provider `ghidra` or `ida` (if set), or just inspect with target-free CLI: `npx -y rea-agents@latest inspect-binary G:\Games\FIFA 22\FIFA22.exe` (if available). Always read-only; never call any function that writes to `.rdata` or `.text` sections.

### 2. Inspect Protocol-Related Sections (educational)
From `docs/MULTIPLAYER_ROADMAP.md` and `frostbite3_multiplayer_reference.md`, target:
- DirtySDK TLS (`ProtoSSL`) — verify embedded CA array structure in `.rdata` (our code patches this in RAM via `cert_bypass.cpp`; REA lets us observe it statically)
- Blaze Component IDs (`0x0001` Auth, `0x0004` GameManager, `0x7802` UserSessions)
- TDF tag encoding (`blaze_tdf::encode_tdf_tag`) — observe binary packing (`6-bit x 4 chars` into 24-bit tag integers)

### 3. Observe Transport (UDP 3659 / QoS 17498)
From `crates/services-qos-relay/src/`:
- `ProtoPing` echo (UDP 17498) — observe probe/reflect packets
- `ProtoTunnel` relay (UDP 3659) — observe 8-byte multiplex header (`Session ID`, `Target Peer ID`, `Payload Length`, `Flags`)
These are external observations (no game modification); fully clean-room.

### 4. Verify No DRM Circumvention
Per `PROJECT_PROGRESS.md`:
- No `Denuvo` token/hash alteration on disk
- No `anadius.cfg` modification
- No `VirtualProtect` changes to `FIFA22.exe`
- `FIFA22.exe` binary remains 592,922,624 bytes; timestamp unchanged
REA evidence confirms this by showing no altered `.rdata` / `.text` sections.

### 5. Document Findings (using other skills)
- `rea-analysis-optimization` → organize evidence (provider selection, memory usage)
- `rea-tool-design` → design observation contracts (e.g., protocol dissector specs)
- `rea-ci-tests-audit` → confirm our Rust test suite (36 passing) remains intact
- `rea-changelog-update` → generate learning notes / changelog (`docs/alternative-c-explained.md` model)

## Active Instrumentation Added (2026-10-18)
- `tools/frida-observe.js`: read-only `Interceptor.attach` on `connect` (observe only, no VirtualProtect).
- `tools/ghidra_notes.md`: Ghidra 12.1.4 PUBLIC installed in `C:\Tools\ghidra_12.1.4_PUBLIC`; `GHIDRA_INSTALL_DIR` set.
- `REA doctor`: Ghidra status is **healthy** (Ghidra 12.1.4 x64 on Windows, JDK 21.0.4 confirmed).
- `FIFA22.exe` untouched: 592,922,624 bytes, timestamp Dec 30 2025.

## What We Do NOT Do (legal / project constraints)
- No decryption of Denuvo-protected sections
- No extraction of copyrighted EA assets (squads, animations)
- No static binary patching (only in-memory observation if needed, with explicit authorization)
- No redistribution of `FIFA22.exe` or derived binaries
- No use of leaked `Server.exe` or proprietary source

## Result
Using the 5 REA skills safely allows educational analysis of `FIFA22.exe`'s network behavior (DirtySDK / Blaze / ProtoTunnel) without breaking multiplayer, without altering the binary, and in full compliance with clean-room / interoperability principles documented in the repo.
