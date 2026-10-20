# FIFA 22 Clean-Room Multiplayer: Distribution & Steam Sync Architecture

**Document Version:** 1.0.0  
**Status:** Approved via `/grill-me` Architectural Alignment  
**Target Engine:** Frostbite 3 / FIFA 22 PC  

---

## 1. System Architecture Overview

The system bridges local platform identity (Origin/Steam) and clean-room network emulation (Blaze/DirtySDK) to enable peer-to-peer and private-server multiplayer over LAN, Radmin VPN, or WAN.

```
+-----------------------------------------------------------------------------------------+
|                                    HOST PC (Server & P1)                                |
|                                                                                         |
|  [ FIFA22.exe ] <---> [ dxgi.dll Shim Proxy ]                                          |
|                              |                                                          |
|                              +---> Dynamic Steam Persona Sync -> anadius.cfg            |
|                              +---> WinSock Reroute to Localhost (127.0.0.1)             |
|                                         |                                               |
|  [ emulator-server.exe Daemon ] <-------+                                               |
|       - Redirector (42127 / 42230)                                                      |
|       - Blaze RPC Cluster (42128 / 44325)                                               |
|       - QoS Latency Probes (UDP 17498)                                                  |
|       - ProtoTunnel Relay (UDP 3659)                                                    |
|       - Synthetic Nucleus & Telemetry Sink (443 / 8080)                                 |
|       - Control Channel REST API (42129)                                                |
+-----------------------------------------------------------------------------------------+
                                          ^
                                          |  Radmin VPN / LAN Connection
                                          v
+-----------------------------------------------------------------------------------------+
|                                   GUEST PC (Friends / P2-P4)                            |
|                                                                                         |
|  [ FIFA22.exe ] <---> [ dxgi.dll Shim Proxy ]                                          |
|                              |                                                          |
|                              +---> Dynamic Steam Persona Sync -> anadius.cfg            |
|                              +---> WinSock Reroute -> Host Radmin IP (e.g. 26.x.x.x)    |
|                                                                                         |
|  [ start_client.bat & companion_ui.py ]                                                 |
|       - One-time Host IP Configuration prompt (stored in f22_emu.ini)                   |
|       - Steam Spacewar (AppID 480) Lobby & Room Code Sync                               |
|       - Synchronized Match Kickoff Trigger                                              |
+-----------------------------------------------------------------------------------------+
```

---

## 2. Core Functional Components

### A. Dynamic Steam Persona Synchronization
* **Mechanism:**
  * When `start_client.bat` or `tools/companion_ui.py` launches, it initializes the Steamworks API (`SteamFriends()->GetPersonaName()`, `SteamUser()->GetSteamID()`).
  * If Steam is running: It reads the live Steam Persona (e.g. `DEad>_<POol`) and automatically rewrites `anadius.cfg` (`"User" { "Username" "<SteamName>" }`).
  * If Steam is closed: It prompts the player for their desired display name in `start_client.bat` or retains the last saved persona.
  * **Result:** No player ever needs to open or edit `anadius.cfg` manually.

### B. Network Redirection & Radmin VPN Setup
* **Mechanism:**
  * In `f22_emu.ini`, `EmulatorHost` and `ServerIp` point to the host's IP (for the host: `127.0.0.1`; for friends: the host's Radmin VPN IP).
  * On initial launch, `start_client.bat` checks if `EmulatorHost` is set to default. If default, it prompts:
    ```text
    Enter Host Radmin VPN IP (or press Enter for Localhost): 
    ```
    and saves the answer to `f22_emu.ini`.
  * `dxgi.dll` intercepts all game traffic destined for `gosredirector.ea.com`, `gosca18.ea.com`, `river.data.ea.com`, and ports `42127`, `42128`, `42230`, `44325`, `443`, rerouting them directly to the specified `EmulatorHost`.

### C. Client Distribution Package (`fifa22_multiplayer_client.zip`)
* **Bundle Contents:**
  1. `dxgi.dll`: The DirectX 12 / WinSock proxy hook.
  2. `f22_emu.ini`: Pre-configured client settings template.
  3. `start_client.bat`: Automatic Steam persona updater, Radmin IP prompt, and game launcher.
  4. `companion_ui.py`: Clean-room room code overlay and match orchestration tool.
  5. `README_FRIENDS.txt`: 3-step installation guide (Extract to game folder -> Run Radmin -> Launch `start_client.bat`).

### D. Matchmaking & Online Friendlies Flow
* **Dual-Method Compatibility:**
  1. **Native In-Game Menu:** Friends appear in the in-game friends list via Blaze component `0x0019` (`AssociationLists::getLists`). Invites sent natively are delivered to the recipient.
  2. **Companion UI Room Codes:** Players join a shared 6-character room code (e.g. `JH4T6M`). Clicking **Kickoff** sends a synchronized `POST /api/v1/game/kickoff` (`0x0064` state change), launching both players directly into the Online Friendly presentation.

---

## 3. Implementation Plan & Execution Checklist

- [x] **Step 1: Automatic Steam Persona Sync Utility**
  - Implemented: `tools/sync_steam_profile.py` reads `LastGameNameUsed`/`AutoLoginUser` from `HKCU\Software\Valve\Steam`, rewrites `anadius.cfg` (`Username`), and pushes the persona to the Control Channel.
  - **Endpoint fix (verified):** `push_to_control_server` now POSTs to `/api/v1/steam/identity` (the only endpoint the Control Server implements — see `crates/emulator-server/src/control_server.rs:342`). The previous `/api/v1/steam/profile` URL did not exist (404), so every push was silently dropped and the persona never reached the session store. Confirmed live: `identity` → 200, `profile` → 404.
  - Integrated into `start_client.bat` (via `--client`) and `start_all.bat`.
- [x] **Step 2: Interactive Radmin Host IP Configuration**
  - Implemented: `start_client.bat` runs `sync_steam_profile.py --client`, which prompts for the host's Radmin VPN IP and writes `EmulatorHost`/`ServerIp` into `f22_emu.ini` (and forces `RerouteAllEA=true`).
- [x] **Step 3: Verification of Server-Side Dual-Stack Binding**
  - Verified in `crates/emulator-server/src/lib.rs`: every listener binds `0.0.0.0` (IPv4 all-adapter) with an IPv6 `[::]` fallback.
    - Redirector `42127` — `0.0.0.0` (L60) + `[::]` (L98)
    - Blaze RPC `42128` — `0.0.0.0` (L61) + `[::]` (L140)
    - Redirector alt `42230` — `0.0.0.0` (L109) + `[::]` (L117)
    - Blaze alt `44325` — `0.0.0.0` (L154) + `[::]` (L165)
    - Control `42129` — `0.0.0.0` (L71) + `[::]` (L179)
    - QoS UDP `17498` — `0.0.0.0` (L69)
    - ProtoTunnel UDP `3659` — `0.0.0.0` (L70)
    - Telemetry `443` — `0.0.0.0` (L62), fallback `8080` on conflict
  - All seven ports in the plan are bound dual-stack. The Radmin VPN adapter is covered because binding `0.0.0.0` accepts on every IPv4 interface, including the Radmin virtual adapter.
- [x] **Step 4: Creation of Friend Distribution Package**
  - Built: `dist/fifa22_multiplayer_client.zip` (1,270,271 bytes, 6 entries):
    - `dxgi.dll` (4,141,081 B) — DirectX 12 / WinSock proxy hook
    - `README_FRIENDS.txt` (1,124 B) — 3-step install guide
    - `start_client.bat` (720 B) — persona sync + Radmin prompt + launch
    - `f22_emu.ini` (1,073 B) — client settings template
    - `tools/sync_steam_profile.py` (5,771 B) — Steam identity bridge
    - `tools/companion_ui.py` (14,493 B) — room code overlay + match orchestration
- [ ] **Step 5: End-to-End Validation**
  - Partial: unit/integration tests pass (`cargo test --all`), boot handshake verified (`tests/boot_handshake.rs`), double-player hub flow verified (`tests/online_hub_flow.rs`), and `/api/v1/steam/identity` confirmed live.
  - **Remaining (requires the user to launch `FIFA22.exe`):** in-game smoke test — click Online Friendlies, observe friends list populate via `AssociationLists::getLists` (0x0019), and confirm no "PRESS [R] TO RECONNECT" prompt. Capture the wire with `tools/dissector/blaze.lua` in Wireshark to close the 0x08BA/0x0005 schema gap noted in `router.rs`.
