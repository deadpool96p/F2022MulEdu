# FIFA 22 Seamless Co-op: Project Progress & Architecture Report

## 1. Project Overview & Pivot
The goal of this project is to enable seamless peer-to-peer multiplayer ("Seamless Co-op") on FIFA 22 without relying on official EA servers (which have reached End of Life / EOS).

Instead of attempting to simulate EA's massive multi-tier cloud infrastructure (Nucleus OAuth, Blaze TDF fire2/heat2 protocol, protoSSL, and dynamic QoS probes), the project leverages **Seamless Co-op Architecture**:
- Intercepting the Frostbite engine's internal `DirtySDK CommUDP` socket at the Winsock boundary.
- Wrapping raw lockstep gameplay synchronization frames into a lightweight 46-byte binary envelope protocol (`SF22`).
- Relaying frames over a dedicated, low-latency Tokio/Rust server (`seamless-relay`) running on port `27015`.

---

## 2. Key Achievements & Technical Milestones

### A. Winsock Detours & Network Layer (`seamless_fifa.dll`)
1. **DirtySDK Port 3659 Match Binding Hook**:
   - Intercepts `bind()` calls for UDP port `3659` (and range `3658-3665`).
   - Ensures `g_orig_bind` executes cleanly before registering the socket handle to prevent `10022` (`WSAEINVAL`) ephemeral socket collisions.
   - Automatically dispatches `HANDSHAKE_INIT` to the relay server upon successful match socket binding.
2. **Double-Wrapping Elimination in `Hooked_sendto`**:
   - Added destination check: if destination IP and port match `g_relay_addr` (port `27015`), or if the buffer already starts with the `PROTOCOL_MAGIC` (`SF22`), packets pass through directly via `g_orig_sendto` without duplicate encapsulation.
3. **Anadius LSX Emulator Neutralization**:
   - Intercepts TCP responses on port `3216` (`Hooked_recv`).
   - In-memory sanitization replaces crack shutdown responses (`"shutdown" -> "running_"`, `"EOS" -> "OK_"`, `"offline" -> "online_"`) to preserve TCP connection persistence without triggering `ExitProcess()`.
4. **Denuvo Anti-Tamper Guard Page Safety**:
   - Refactored PE image memory scanners (`PatchDirtySdkCaTableInModule`) to use `VirtualQuery`.
   - Skips uncommitted (`MEM_RESERVE`) and protected guard pages (`PAGE_GUARD`), preventing Denuvo VEH termination triggers.

---

## 3. Frostbite Engine & Game State Machine Reverse Engineering

Using static analysis, disassembly (`llvm-objdump`), and xref analysis of `FIFA22.exe`, we mapped out the internal game flow and match setup structures:

### A. Memory Patches (Verified & Active)
| Symbol / Function | Virtual Address (VA) | RVA | Patch Type | Purpose |
|---|---|---|---|---|
| `Root Entitlement Check` | `0x14196B650` | `0x0196B650` | `B8 01 00 00 00 C3` | Forces `IsEntitled()` $\to$ `true` |
| `OriginCheckOnline` | `0x1424D92D0` | `0x024D92D0` | `48 85 C9 74 03 C6 01 01 31 C0 C3` | Forces `*rcx = 1; return 0;` (Online) |
| `OnlineAccess Local Flag` | `0x144F32B5F` | `0x04F32B5F` | `40 B6 01` | Sets `sil = 1` (Online entitlement granted) |
| `OriginSDK Internal Online State` | `0x1424DDCEA` | `0x024DDCEA` | `C6 06 01 31 DB EB 0F` | Forces online state in OriginSDK internal state machine |
| `Server Status Popup Bypass` | `0x144D89F4C` | `0x04D89F4C` | `90 90 90` | NOPs shutdown dialog dispatch |
| `Sub-call Failure Coercion` | `0x1427E2C72` | `0x027E2C72` | `B8 00 00 00 00` | Coerces failure returns to 0 (Success) |
| `Frostbite isPlayTileSelectable` | `0x145BC58F2` | `0x05BC58F2` | `C6 44 24 70 01` | Forces Play Tile selection to unlocked |

### B. Match Flow & State Transition Architecture
1. **Screen Flow & State Transition**:
   - `ScreenFlowController::ChangeState() oldState(%d), newState(%d)`: logged at string VA `0x147AC71A8`.
   - `ActionEnterPreMatchFlowFromHub::Execute`: entry point at VA `0x147B1AB68`.
   - `ScreenControllerPlayMatch`: sets up Kick-Off match flow at VA `0x14625248E`.
   - `PlayMatchAsyncLoading`: asynchronous stadium and fixture loader callback at VA `0x141FCD1C0`.
2. **Match Setup & Configuration Globals**:
   - `0x1487D3258`: Home Team ID (`uint32`)
   - `0x1487D325C`: Away Team ID (`uint32`)
   - `0x1487D3260`: Stadium ID (`uint32`)
   - `0x1487D3264`: Weather Type (`uint32`)
   - `0x1487D3268`: Time Of Day (`uint32`)
3. **Match Management & Session State**:
   - `MatchService::MatchManager` initialized at VA `0x1458E7CD0`.
   - `MatchStatesDB` state machine table at VA `0x147AB1A58`.

---

## 4. Current Repository Structure
- `crates/seamless-relay`: High-performance asynchronous Rust UDP relay server (Tokio + BLAKE3).
- `crates/blaze-auth-server`: Standalone offline Blaze auth/redirector service for legacy protocol compatibility.
- `shim`: C++ WinSock hook and Denuvo-safe memory patching proxy (`seamless_fifa.dll`).
- `launcher`: Native C++ injector and launcher (`FIFA22_Seamless.exe`).
- `docs/seamless-fifa`: Architecture specifications, relay packet definitions, and network detour design.

---

## 5. Next Steps
1. Implement hotkey / trigger (e.g. F10) in `seamless_fifa.cpp` to invoke `ActionEnterPreMatchFlowFromHub::Execute` or transition `ScreenFlowController` directly into match loading.
2. Confirm two clients connected to `seamless-relay` exchanging lockstep DirtySDK frames on port `3659`.
3. Validate in-game synchronized pitch gameplay between host and guest.
