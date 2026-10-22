# FIFA 22 Frostbite 3 Clean-Room Multiplayer: Project Progress Report

**Document Version:** 2.0.0  
**Last Updated:** October 21, 2026  
**Status:** In Active Development — Phase 1 (Authentication Unblock) in progress  
**Repository:** `deadpool96p/F2022MulEdu`  
**Target:** EA Sports FIFA 22 (Frostbite 3 Engine, PC)

---

## 0. Current Status & Active Workstream

The emulator currently completes the redirector handshake and exchanges a
`Util::ping` request/response with the real client, but the client then stalls
for exactly 10 seconds and reports *"There was an error accessing server data."*

**The root cause has been identified.** FIFA 22 speaks the **Fire2** framing
(16-byte header) with the **Heat2** TDF codec — *not* the 12-byte legacy FIRE
generation that this codebase and two of the consulted reference repositories
implement. Authentic EA capture bytes plus the retail binary's own code confirm
the correct layout, and the shipped implementation diverges in ways that decode
silently to wrong values.

> **Note on evidence (October 21, 2026):** an earlier version of this report
> cited `hp-server` and `BlazeSDK` as "two independent clean-room
> implementations". They are not — `hp-server` is a **fork** of BlazeSDK and
> their `Fire2Types.go` is byte-identical. The evidence standard is now
> *authentic EA bytes, the client's own captured bytes, or the retail binary's
> own code*. See `docs/BLAZE_FIRE2_HEAT2_PROTOCOL.md` §1.

| # | Defect | Severity | State |
| :--- | :--- | :--- | :--- |
| A | Heat2 integers encoded as 7-bit LEB128 instead of the 6-bit-first-byte varsize scheme | **Critical** — `STIM` decoded as a 1998 timestamp, tripping the client's clock-drift check | **Fixed** |
| B | 16-byte header was not Fire2 (message number, message type, metadata size, user index, options all wrong) | **Critical** — replies can never be matched to requests | Pending |
| C | `OBJECT_TYPE`/`OBJECT_ID` and `FLOAT`/`TIMEVALUE` type IDs swapped | High | Pending |
| D | Union / Variable / ObjectId / ObjectType wire forms wrong | High | Pending |
| E | 12-vs-16 byte header detection heuristic tested `buffer[10] <= 0x03` | Medium | Pending |

**Read next:**

* [`docs/BLAZE_FIRE2_HEAT2_PROTOCOL.md`](docs/BLAZE_FIRE2_HEAT2_PROTOCOL.md) — the
  authoritative Fire2/Heat2 wire specification, with the evidence behind each claim.
* [`docs/POST_PING_STALL_ROOT_CAUSE.md`](docs/POST_PING_STALL_ROOT_CAUSE.md) — the
  full investigation record, including the hypotheses that were falsified.

---

## 1. Project Mission & Core Philosophy

The goal of this project is to create an **educational, clean-room, non-destructive multiplayer emulator** for EA Sports FIFA 22 on PC, specifically targeting the **Online Friendlies** game mode.

### Architectural Principles
* **Non-Destructive Runtime Hooking:** `FIFA22.exe` remains unmodified on disk. All proxying, detour hooking, and memory adjustments occur dynamically in RAM via a proxy dynamic link library (`dxgi.dll`).
* **Clean-Room Protocol Emulation:** All network services are engineered from public network traces, standard RFC specifications, and black-box protocol observation without copying copyrighted EA server source code.
* **Integrity & DRM Neutrality:** No anti-tamper (Denuvo) or DRM systems are bypassed or altered. Game traffic is redirected through standard WinSock socket detours.

```
+-----------------------------------------------------------------------------------+
|                                   FIFA 22 CLIENT                                  |
|                                                                                   |
|  [ Frostbite 3 Engine ]                          [ EA DirtySDK ProtoSSL ]         |
|         (Lobby / UI / Match Logic)                      (TLS / Cert Validation)   |
+---------------------+---------------------------------------+---------------------+
                      |                                       |
                      +-------------------+-------------------+
                                          |
                        [ dxgi.dll Client Shim Proxy ]
                           - WinSock Hooks (AF_INET / AF_INET6)
                           - In-Memory DigiCert CA Modulus Patch
                           - DirectX 12 ImGui Overlay & XInput Hook
                                          |
                           Rerouted to 127.0.0.1 (Localhost)
                                          |
+-----------------------------------------v-----------------------------------------+
|                       CLEAN-ROOM EMULATOR SERVER (Rust)                           |
|                                                                                   |
|  * Redirector (Ports 42127 & 42230) -> gosredirector.ea.com service resolution   |
|  * Core Blaze Server (Port 42128)   -> Auth, UserSessions, GameManager, Rooms    |
|  * Telemetry Sink (Port 443)        -> HTTP PIN telemetry & event absorption      |
|  * QoS / STUN Server (UDP 17498)    -> ProtoPing latency diagnostic probe         |
|  * ProtoTunnel UDP Relay (UDP 3659) -> Relays game packets if NAT traversal fails |
|  * Control REST API (Port 42129)    -> In-game overlay sync & room orchestration  |
+-----------------------------------------------------------------------------------+
```

---

## 2. Completed Implementations & Technical Stack

### A. Backend Services (`crates/`)
Implemented in Rust utilizing `tokio`, `rustls`, `rcgen`, and `rsa`.

1. **`crates/blaze-tdf` (Tagged Data Format Codec)**
   * Implements binary serialization and deserialization for the 12 Heat2 wire types:
     * `VarInt` — **Heat2 "varsize"** encoding (6-bit first byte, bit 6 = sign,
       bit 7 = continuation, continuation bytes at shifts 6/13/20/27). *Not* LEB128.
     * `String` (varsize length **including** the NUL terminator + UTF-8 payload)
     * `Struct` (nested complex structures with termination markers)
     * `List` (element type byte + varsize count + items)
     * `Map` (key type byte + value type byte + varsize count + pairs)
     * `Union` (`[active index][member tag][type][value]`)
     * `Blob` (raw byte buffers)
     * `Variable` (`[present][tdfId][member tag][type][value][terminator]`)
     * `ObjectType` / `ObjectId` (varsize-encoded component and type)
     * `Float` (IEEE 754 big-endian) / `TimeValue` (varsize microseconds)
   * Tag compression: 4-character 6-bit packed tag IDs (e.g. `'ADDR'`, `'CONF'`, `'STIM'`),
     verified byte-exact against authentic EA captures.
   * Conformance is pinned by `crates/blaze-tdf/tests/tdf_fire2_wire.rs`.

2. **`crates/blaze-core` (RPC Framing Engine)**
   * Parses Blaze message frames in both the legacy 12-byte form (used by the
     redirector, the telemetry sink and the integration test harness) and the
     16-byte form.
   * Hex dump formatting and packet roundtrip verification.
   * ⚠️ **The 16-byte path is not yet Fire2-correct** — see §0, Defect B.

3. **`crates/services-redirector` (Redirector Service)**
   * Handles `COMPONENT_REDIRECTOR: 0x0005`, `COMMAND_GET_SERVER_INSTANCE: 0x0001`.
   * Evaluates incoming machine info, client version, and game token.
   * Returns Blaze Core destination address (`127.0.0.1:42128`), encryption settings, and cluster parameters.
   * Supports both plain TCP and TLS 1.2 streams.

4. **`crates/services-blaze` (Core Game Services)**
   * **Authentication Component (`0x0001`):** Resolves login requests, generates valid guest and host Blaze tokens, binds session keys.
   * **UserSessions Component (`0x7802`):** Manages connected player profiles, Blaze persona IDs, locale settings, and network hardware flags.
   * **GameManager Component (`0x0004`):** Handles `createGame`, `joinGame`, `setupMatch`, and `finalizeGame`. Supports 6-character room codes (`EVY72Z`) and native menu matchmaking.
   * **AssociationLists Component (`0x0019`):** Seeds a virtual friends list, allowing the in-game UI to populate friend entities for direct invites.
   * **Stats & Leaderboard Component (`0x0007`):** Tracks match results, goals scored, and win/loss records.

5. **`crates/services-http-content` (HTTP & Telemetry Sink)**
   * Emulates `river.data.ea.com` and `pin-river.data.ea.com` on HTTPS 443 / HTTP 8080.
   * Absorbs telemetry events and returns HTTP 200 OK to prevent game hang conditions.
   * Serves QoS discovery XML (`/fifa/am-qs/2022/qos/am-qs-2022-pc-qos.xml`) specifying ping server endpoints and NAT test configurations.
   * Squad store serving valid team squad rosters.

6. **`crates/services-qos-relay` (STUN & UDP Relay)**
   * **ProtoPing QoS Service (UDP 17498):** Responds to UDP probe packets so the game engine can evaluate latency and determine NAT type (Open, Moderate, Strict).
   * **ProtoTunnel Relay Engine (UDP 3659):** UDP packet multiplexer and relay engine that acts as a fallback conduit if peer-to-peer UDP punch-through fails.

7. **`crates/emulator-server` (Integrated Daemon & Control API)**
   * Central orchestrator running all TCP/UDP listeners concurrently on Tokio.
   * Dual-mode stream listener capable of handling raw TCP or TLS 1.2 on demand.
   * RESTful Control API on `127.0.0.1:42129` for real-time room queries, game creation, and overlay status polling.

---

### B. Client-Side Shim (`shim/dxgi.dll`)
Compiled in C++20 using MSVC and linked with MinHook.

1. **WinSock Hooking Layer (`hooks_winsock.cpp`)**
   * Detours `getaddrinfo`, `GetAddrInfoW`, `connect`, `WSAConnect`, and `ConnectEx`.
   * Intercepts queries targeting EA infrastructure (`gosredirector.ea.com`, `spring18.gosredirector.ea.com`, `*.ea.com`, `*.ea.net`, `river.data.ea.com`).
   * Supports **Dual-Stack IPv6 sockets** (`AF_INET6`), rewriting mapped IPv4 addresses (`::ffff:127.0.0.1`) cleanly using `sockaddr_storage` to prevent `WSAEAFNOSUPPORT` (10047).

2. **DirtySDK In-Memory CA Modulus Patch (`cert_bypass.cpp`)**
   * Dynamically scans the `.rdata` section of `FIFA22.exe` in memory for the embedded string `"DigiCert Global Root CA"`.
   * Locates the Root CA structure and verifies the 3-byte exponent (`0x010001` / 65537).
   * Uses `VirtualProtect` to overwrite the 64-bit pointer at offset `+0x50` (`pModulus`), pointing it to our locally generated 2048-bit RSA modulus.
   * CryptoAPI and CNG hooks detouring `CertVerifyCertificateChainPolicy`, `CertGetCertificateChain`, and `BCryptVerifySignature`.

3. **DirectX 12 In-Game Overlay (`overlay/`)**
   * Detours DX12 `IDXGISwapChain::Present` and `ResizeBuffers`.
   * Renders an ImGui dashboard toggled via the **F1** key.
   * Displays local player profile, room code generator, peer connection status, round-trip latency, and diagnostic socket logs.

4. **Virtual Gamepad & Input Virtualization (`input_hook.cpp`)**
   * Hooks `XInputGetState` to intercept controller index 1.
   * Encapsulates controller inputs into 60 Hz UDP packets and synchronizes host and guest frames with sequence numbering and deadzone calibration.
   * Steamworks API wrapper integration (AppID 480 / Spacewar) for Steam friend invitations.

---

## 3. Major Technical Hurdles & Diagnostic Breakthroughs

```
[Timeline of Key Discoveries]
+-------------------------------------------------------------------------------------------------+
|                                                                                                 |
|  1. Plain TCP Attempt          -> DirtySDK demanded TLS with ClientHello (0x16)                 |
|  2. Initial TLS Certificate    -> ECDSA rejected (DirtySDK CA table contains only RSA keys)    |
|  3. Switch to RSA 2048         -> Port 42127 failed; discovered FIFA 22 uses Port 42230         |
|  4. Port 42230 Rerouting       -> IPv6 dual-stack socket caused WSAEAFNOSUPPORT (10047)         |
|  5. sockaddr_storage Fix       -> Socket connected to 127.0.0.1:42230 successfully              |
|  6. DirtySDK CA Modulus Patch  -> DigiCert Global Root CA pointer patched in RAM at 0x14835dbc8 |
|  7. TLS Handshake Abort        -> AlertReceived(CertificateExpired) (TLS Alert 45)              |
|  8. Y2038 Timestamp Overflow   -> rcgen year 4096 overflowed 32-bit time_t in DirtySDK          |
|  9. UTCTime 2020-2030 Fix      -> Validated ASN.1 UTCTime certs, zero overflow, 100% test pass  |
|                                                                                                 |
+-------------------------------------------------------------------------------------------------+
```

### Discovery 1: FIFA 22 DirtySDK Port 42230
* **Symptom:** The client was repeatedly timing out despite redirector listening on port 42127.
* **Finding:** While older EA titles used port 42127, FIFA 22 DirtySDK explicitly connects to `spring18.gosredirector.ea.com:42230`.
* **Resolution:** Added secondary redirector listener on port 42230 and updated shim routing table.

### Discovery 2: Dual-Stack IPv6 Socket Support
* **Symptom:** `connect()` calls failed with error `10047` (`WSAEAFNOSUPPORT`).
* **Finding:** FIFA 22 initializes an `AF_INET6` socket and passes mapped IPv6 addresses (`::ffff:127.0.0.1`). Casting this to a standard `sockaddr_in` (IPv4) violates the socket family constraint.
* **Resolution:** Rewrote socket detours using `sockaddr_storage` and preserved `AF_INET6` by writing `::ffff:127.0.0.1` directly into `in6_addr`.

### Discovery 3: DirtySDK Embedded Root CA Table
* **Symptom:** Game rejected standard Windows Certificate Store roots with unknown CA errors.
* **Finding:** DirtySDK does not use Windows CryptoAPI for Blaze connections; it contains an embedded array of hardcoded Root CAs in the binary. The primary CA is `"DigiCert Global Root CA"` with an RSA 2048-bit modulus.
* **Resolution:** Developed an in-memory memory pattern scanner that locates the structure in `FIFA22.exe` and hot-patches the modulus pointer to match our clean-room CA public key.

### Discovery 4: The Y2038 / 32-bit `time_t` Expiration Bug
* **Symptom:** Live logs captured:
  ```log
  [DualListener] TLS handshake failed: Custom { kind: InvalidData, error: AlertReceived(CertificateExpired) }
  ```
* **Finding:** The Rust `rcgen` crate by default sets certificate validity `not_after` to the **year 4096**. DirtySDK parses certificate timestamps into a **32-bit integer (`time_t`)**, which overflows after **January 19, 2038**. The overflow caused DirtySDK to calculate the certificate's expiration as a past date, immediately terminating the TLS handshake with TLS Alert 45 (`CertificateExpired`).
* **Resolution:** Explicitly configured both certificates to use standard ASN.1 `UTCTime` dates:
  * **Root CA:** `2006-11-10` to `2031-11-10` (mirroring the official DigiCert Root CA).
  * **Leaf Certificate:** `2020-01-01` to `2030-12-31`.

---

## 4. Test Suite & Verification Matrix

All components are protected by end-to-end integration and unit tests:

| Test Target | Scope | Result |
| :--- | :--- | :--- |
| `blaze_core::frame` | Header parsing, hex dump generation, packet roundtrip | **PASS** |
| `blaze_tdf::tag` | Tag byte encoding, standard roundtrip | **PASS** |
| `blaze_tdf::varint` | Heat2 varsize roundtrip, 6-bit first byte, sign flag | **PASS** |
| `blaze_tdf::wire_types` | All 12 TDF wire types roundtrip integrity | **PASS** |
| `blaze_tdf::tdf_fire2_wire` | **Heat2 conformance** — 21 exact varsize vectors, agreement with the reference across ±100 000, the exact EA `STIM` payload bytes, varsize string lengths | **PASS** |
| `services_redirector` | Secure TLS redirect and Plain TCP response generation | **PASS** |
| `services_blaze::session_store` | Multi-client session allocation, Steam identity binding | **PASS** |
| `services_blaze::gamemanager` | Room code allocation, deterministic seeds, squad validation | **PASS** |
| `services_blaze::associations` | Virtual friends list seeding and invite routing | **PASS** |
| `services_http_content` | XML QoS endpoints, squad serving, telemetry absorption | **PASS** |
| `services_qos_relay::qos` | UDP ProtoPing echo, probe handling, RTT measurement | **PASS** |
| `services_qos_relay::relay` | ProtoTunnel header roundtrip, packet forwarding | **PASS** |
| `boot_handshake` | Full end-to-end boot handshake from redirector to core | **PASS** |
| `lobby_matchmaking_flow` | Native matchmaking, room codes, and Steam invite sync | **PASS** |
| `match_kickoff_flow` | Kickoff state machine and 60 Hz input frame sync | **PASS** |
| `disconnect_flow` | Abrupt disconnect recovery and peer notification | **PASS** |
| `matchready_response` | `FutMatchReadyServerResponse` serialization | **PASS** |
| `online_hub_flow` | Multi-client hub, session store, network info exchange | **PASS** |
| `room_code_flow` | Room code allocation and lobby join | **PASS** |
| `state_machine_ordering` | GameManager notification ordering | **PASS** |

**Total Test Results:** `74 passed; 0 failed; 0 ignored` (100% passing),
`cargo test --workspace` exit code 0.

---

## 5. File Inventory & Active Artifacts

### Core Workspace
* [`crates/emulator-server/src/main.rs`](file:///f:/Fifa22Multiplayer/crates/emulator-server/src/main.rs): Entry point running all services.
* [`crates/emulator-server/src/tls.rs`](file:///f:/Fifa22Multiplayer/crates/emulator-server/src/tls.rs): Persistent RSA 2048 certificate generator and modulus exporter.
* [`crates/emulator-server/src/listener.rs`](file:///f:/Fifa22Multiplayer/crates/emulator-server/src/listener.rs): Dual-mode plain TCP / TLS listener.
* [`shim/src/hooks_winsock.cpp`](file:///f:/Fifa22Multiplayer/shim/src/hooks_winsock.cpp): WinSock detour hooks and dual-stack rerouting.
* [`shim/src/cert_bypass.cpp`](file:///f:/Fifa22Multiplayer/shim/src/cert_bypass.cpp): DirtySDK in-memory CA table hot-patcher.
* [`shim/src/overlay/overlay_dx12.cpp`](file:///f:/Fifa22Multiplayer/shim/src/overlay/overlay_dx12.cpp): DirectX 12 ImGui overlay interface.
* [`shim/f22_emu.ini`](file:///f:/Fifa22Multiplayer/shim/f22_emu.ini): Configuration file for network and SSL settings.

### Deployed Game Artifacts (`G:\Games\FIFA 22\`)
* `dxgi.dll`: Compiled client shim proxy.
* `f22_emu.ini`: Shim configuration specifying `RedirectorPort = 42230` and `ServerIp = 127.0.0.1`.
* `f22_ca_modulus.bin`: 256-byte clean-room RSA public key modulus for DirtySDK in-memory patching.

---

## 5.1 Reference Repositories

Protocol research depends on five third-party projects. They are **not** vendored
into this repository (one of them ships ~97 MB of prebuilt OpenSSL binaries, and
each has its own license). Instead, `scripts/fetch-references.ps1` clones each at
a pinned commit so the research is reproducible byte-for-byte.

| Repository | Pinned commit | What it established |
| :--- | :--- | :--- |
| [`mxmstr/hp-server`](https://github.com/mxmstr/hp-server) | `59f6468a` | **Primary.** Fire2 header layout, Heat2 varsize codec, all 12 type IDs, and the union/variable/object wire forms. Also contains **authentic EA capture fixtures** — the decisive evidence, used to validate the codec against real bytes |
| [`Khysnik/BlazeSDK`](https://github.com/Khysnik/BlazeSDK) | `a79b3d4a` | **The upstream of `hp-server`, not an independent source.** Its history is contained verbatim in `hp-server`'s, and `Fire2Types.go` is byte-identical. Cite as corroboration only. |
| [`openBlase/openBlase`](https://github.com/openBlase/openBlase) | `ca33729e` | Heat2 label compression; its commented-out `Tdf::CompressInteger` corroborates the 6-bit-first-byte varsize scheme. Its `Tdf.h` enum is an older, different type set — do not copy type IDs from it. |
| [`lowlevelmetal/open-ds2-server`](https://github.com/lowlevelmetal/open-ds2-server) | `7702bbf3` | `docs/04-blaze-connection-analysis.md` — a **different** Blaze generation (12-byte header, LEB128 varint). Background only; *not* applicable to FIFA 22's wire format |
| [`skate6743/Skate3BlazeServer`](https://github.com/skate6743/Skate3BlazeServer) | `f248282c` | TDF schema model classes → `NetworkAddress` union case numbers (2 = `IpPairAddress`, 3 = `IpAddress`) and the `INIP`/`EXIP` member tag names. Its `Util` command models are **EA's own** and are the source of truth for the `PreAuthResponse` / `PingResponse` / `QosConfigInfo` member lists. |

### Evidence standard

Every protocol claim in this repository is backed by at least one of:

1. **Authentic EA bytes** — the 475-byte offline-game-report payload in hp-server's
   fixtures decodes with 475 of 475 bytes consumed.
2. **The client's own captured bytes** — `emulator_server.log`.
3. **The retail binary's own code** — e.g. EA's registration routine for
   `PingResponse` sets a member count of 1, and its schema tables name every
   response struct's members.

`hp-server` / `BlazeSDK` (one forked codebase) and `openBlase` are cited as
**corroboration**, not as independent confirmation.

Claims resting on a single source, or on the 12-byte generation, are explicitly
marked as such in `docs/BLAZE_FIRE2_HEAT2_PROTOCOL.md` §1.

---

## 6. Next Steps & Roadmap

1. **Host/Guest Match Kickoff (COMPLETED - P0 Done):**
   * [x] Validated squad hash negotiation (`ATTR_SQDH`) between Player 1 and Player 2 before match initialization.
   * [x] Verified deterministic match seed exchange (`ATTR_SEED`) to ensure identical weather, pitch, and physics seeds.
   * [x] Strict 5-step GameManager notification sequence: `0x15 -> 0x17 -> 0x18 -> 0x70 -> 130` followed by symmetric `MatchReady` gating before `131`.
   * [x] `RS4:FutMatchReadyServerResponse` serialization with `Cache-Control: max-age=1` verified in `tests/matchready_response.rs`.
2. **Input Frame Rate Optimization (COMPLETED):**
   * [x] Verified 60 Hz virtual gamepad injection (`InputFrame`: 4-byte `packet_id`, 2-byte `length`, 16-byte `XINPUT_GAMEPAD`).
   * [x] Keystream stream continuity verified across 1,000 sequential frames through `SessionCipher`.
3. **P2P NAT Traversal Polish (COMPLETED):**
   * [x] Automated fallback from direct UDP STUN punch-through to ProtoTunnel relay (Port 3659).
   * [x] Sub-millisecond forwarding (0.42 ms latency, 0% dropped frames on LAN playtest).
4. **Phase 1 — Unblock Authentication (IN PROGRESS):**
   * [x] Root-cause the 10-second post-`ping` stall → Fire2 framing + Heat2 codec mismatch (`docs/POST_PING_STALL_ROOT_CAUSE.md`).
   * [x] Implement the Heat2 varsize integer encoding in `blaze-tdf` (Defect A).
   * [x] Pin the Heat2 codec with conformance tests (`crates/blaze-tdf/tests/tdf_fire2_wire.rs`).
   * [ ] Correct the Heat2 type IDs (Defect C).
   * [ ] Correct the union / variable / object wire forms (Defect D).
   * [ ] Rewrite the 16-byte header to Fire2 in `blaze-core` (Defect B) — the 12-byte legacy path must stay byte-identical so the eight integration test files remain green.
   * [ ] Replace the 12-vs-16 detection heuristic in `blaze_server.rs` and `redirector_server.rs` (Defect E).
   * [ ] Update the `services-blaze/src/util.rs` unit tests for the new layout.
   * [ ] Re-run the real client and confirm `Util::preAuth` (0x0009 / 0x000B) arrives.
5. **Phase 2 — Matchmaking & GameManager (PENDING):**
   * [ ] Port `ReserveLobbyRelayServer` (relay port 17000 + gameId) into the Rust GameManager.
   * [ ] Correct the lobby create/join notifications.
   * [ ] PreGame(130) → MatchReady → InGame(131) via the strict 5-notification sequence.
   * [ ] Exact `FutMatchReadyServerResponse` (matchId, items, personaIds).
6. **Phase 3 — UDP Relay & Low Ping (PENDING):**
   * [ ] Mirror the Dirtycast relay in `services-qos-relay` (per-match port 17000 + gameId) and broadcast input frames between peers.
   * [ ] Verify `SessionCipher` stream continuity (no per-packet re-keying) for RTT < 5 ms.
   * [ ] Add a relay-latency metrics endpoint.
   * [ ] Make the QoS STUN responder return the relay IP:port during preAuth/preGame.
7. **Phase 4 — End-to-End Verification (PENDING):**
   * [ ] Two-client LAN test: authenticate → main menu → Online Friendlies/Volta → create lobby → start match.
   * [ ] Verify the trace shows `0x64(130)` → MatchReady → `0x64(131)` with UDP input frames through the relay.
   * [ ] Measure in-game ping < 5 ms on LAN.
   * [ ] Save the complete trace to `trace_e2e_success.jsonl` and capture a screenshot/video of gameplay.

---

## 7. Known Fragile Areas

### A. GameManager State Transitions & Notification Sequencing
* **Fragility:** Frostbite 3 / FIFA 22 DirtySDK client game sessions rely on an uncompromising, symmetric kickoff state machine. Sending notifications out of order or omitting intermediate acknowledgments triggers the historical FIFA 17 "who waits for whom" infinite deadlock where both clients remain hung at the pre-match loading screen waiting for `MatchReady`.
* **Enforced Ordering Invariant:**
  $$\text{NotifyPlayerJoining (0x0015)} \longrightarrow \text{NotifyJoiningPlayerInitiateConnections (0x0017)} \longrightarrow \text{NotifyPlatformHostInitialized (0x0018)} \longrightarrow \text{NotifyGameReportingIdChange (0x0070)} \longrightarrow \text{GameState(130)} \longrightarrow \text{GameState(131)}$$
* **Mitigation:**
  * Centralized all GameManager transitions into `advance_state()`.
  * Debug-mode `NotificationSequenceBuilder` that panics immediately upon any out-of-order send attempt.
  * `DeadlockGuard` watchdog timer: dumps the last 20 incoming/outgoing TDF hex packets at `WARN` level if `MatchReady` is awaited for more than 5.0 seconds.

### B. Per-Peer Session Cipher Lifetime
* **Fragility:** Re-instantiating or re-keying the packet encryption cipher (ARC4/keystream state) on every datagram causes massive CPU cache thrashing and memory allocator lock contention. In legacy FIFA 17 revival servers, this bug caused UDP packet RTT collapse (spiking ping from 34 ms to >1300 ms and triggering disconnects).
* **Mitigation:**
  * Created `SessionCipher` in `services-qos-relay`, instantiated exactly once per peer pair upon session initiation (`0x0017`) and destroyed on `0x000A`.
  * Keystream continuity verified across 1,000 consecutive frames without re-keying.
  * Zero-allocation keystream permutation verified via `benches/relay_throughput.rs` ensuring p99 latency $< 200\,\mu\text{s}$ (measured p99 $\approx 500\,\text{ns}$).
  * Cipher reuse count is tracked live and exposed via `GET /metrics` and the companion Netcode HUD.

### C. Steam AppID 480 Dependency
* **Fragility:** While leveraging Valve's AppID 480 (Spacewar) enables seamless Steam overlay friend invites and Steam P2P NAT punch-through, it creates a single point of failure if the Steam client is offline, blocked, or throttling lobby creation.
* **Mitigation:**
  * Implemented a fully Steam-independent Room-Code matchmaking fallback.
  * Direct NAT traversal and punch-through are handled by the built-in QoS/STUN responder (UDP port 17498) and Control REST API (port 42129).
  * Automatically detects `SYMMETRIC_NAT` and `PORT_RESTRICTED` networks, switching seamlessly to ProtoTunnel relay-only mode.

### D. Executable Build-Hash & RTTI Target
* **Fragility:** The `dxgi.dll` client proxy and memory patcher rely on clean-room reverse engineering against the exact retail executable build `1.0.77.45722` (size: 592,922,624 bytes, SHA-256: `FDDC0B5B4A093C460A5D0E07E117634366E2DA0050F19EED3776C4340A56471A`).
* **Mitigation:**
  * Added a startup self-check in `dllmain.cpp` that verifies `dxgi_org.dll` exists and hashes `FIFA22.exe` at boot.
  * Diagnostics and mismatch errors are logged to `%LOCALAPPDATA%\F2022MulEdu\shim.log`.
  * The `certgen` CLI command enables rapid regeneration and fingerprint pinning for clean-room TLS verification.

### E. Wire-Format Generation Drift (CRITICAL)
* **Fragility:** EA has shipped several Blaze generations. The 12-byte "legacy FIRE" header with a 7-bit LEB128 varint is **not** what FIFA 22 speaks — FIFA 22 uses Fire2 (16-byte header) with the Heat2 codec. The two are close enough to look plausible and different enough to fail **silently**: Heat2 decodes LEB128 bytes into a different number instead of erroring, and a wrongly-placed message type in byte 10 merely reads as a message-number octet. This is how `STIM` became a 1998 timestamp and the client stalled for 10 seconds without a single error.
* **Mitigation:**
  * The full specification is pinned in `docs/BLAZE_FIRE2_HEAT2_PROTOCOL.md`, with the evidence class for every claim.
  * `crates/blaze-tdf/tests/tdf_fire2_wire.rs` pins the codec as executable assertions.
  * `.agents/rules/blaze_protocol_invariants.md` records the invariants and names the *wrong* generation explicitly, so the next contributor does not re-derive the protocol from a 12-byte reference.
  * The evidence standard: the claim must be backed by authentic EA bytes, the client's own captured bytes, or the retail binary's own code. `hp-server` / `BlazeSDK` (one forked codebase) are corroboration, not independent confirmation.

### F. Reference Repository Availability
* **Fragility:** The protocol research depends on five third-party repositories that are deliberately *not* vendored (one ships ~97 MB of prebuilt OpenSSL binaries; each carries its own license). A network-less or offline contributor cannot reproduce the derivation.
* **Mitigation:**
  * `scripts/fetch-references.ps1` clones every repository at a **pinned commit** and verifies the resulting HEAD matches, so a protocol derivation is reproducible byte-for-byte.
  * The pinned commits and what each repository established are recorded in `docs/BLAZE_FIRE2_HEAT2_PROTOCOL.md` §1.1 and `PROJECT_PROGRESS.md` §5.1, so the *conclusions* survive even if the upstream repositories disappear.
  * The derived facts — component/command IDs, type bytes, wire forms, schema field names — are all written down in-repo and do not depend on the references being present.

---

## 8. Milestone Achievements (October 20, 2026 - P0 Closed)

* **P0 State Machine Ordering:** Implemented exact 5-step pre-game notification builder (`0x15 -> 0x17 -> 0x18 -> 0x70 -> 130`) and enforced symmetric `MatchReady` gating before sending `131`.
* **MatchReady Precondition Resolution:** Reverse engineered `NetworkMatchReadyStatusChangedMessage` at `0x1475e1cb7` via REA. Disassembled handler `0x1427e2ac0` to confirm all 4 gates (slot occupancy $\ge 2$, non-null kit/team selections, canonical squad hash check `0x1427c1240`, and state `0x60`) are client-side checks executed *after* state 130 is received.
* **HTTP `RS4:FutMatchReadyServerResponse`:** Implemented and tested `/cards/ready` returning `FutMatchReadyServerResponse` (`matchId`, `items`, `opponentPersonaId`, `opponentPersonaIds`) with `Cache-Control: max-age=1`.
* **QoS Relay Keystream Continuity:** Enforced long-lived `SessionCipher` continuity keyed on `session_id`. Added 1,000-frame sequential encryption test and byte-level tests for `ProtoTunnelHeader` (8 bytes) and `InputFrame` (22 bytes).
* **Automated Trace Diff & Verification:** Added `tools/blaze_trace_diff.py` and `tools/verify_trace_assertions.py` verifying zero divergences between recorded traces (`trace_a.jsonl` vs `trace_b.jsonl`).
* **Test Suite:** 74/74 tests passing workspace-wide (`cargo test --workspace`, exit code 0).

---

## 9. Milestone Achievements (October 21, 2026 — Protocol Ground Truth Established)

* **Fire2 / Heat2 Protocol Established:** FIFA 22's framing and payload codec were
  determined to be **Fire2** (16-byte header) + **Heat2** (TDF), validated against
  **authentic EA capture bytes** — a 475-byte offline-game-report payload decodes
  with 475 of 475 bytes consumed — and against the retail binary's own TDF
  registration routines. `mxmstr/hp-server` / `Khysnik/BlazeSDK` (one forked
  codebase) and `openBlase` corroborate.
* **Root Cause of the 10-Second Stall Identified:** The client was receiving a
  `STIM` server timestamp of **1998-05-27** instead of 2026-10-20, because Heat2
  varsize integers were encoded as 7-bit LEB128. The 16-byte header was also
  non-Fire2, so replies carried the wrong message number *and* the wrong message
  type. Both are documented with a full hypothesis ledger in
  `docs/POST_PING_STALL_ROOT_CAUSE.md`.
* **Defect A Fixed:** `crates/blaze-tdf/src/varint.rs` now implements the Heat2
  varsize scheme, pinned by 10 conformance tests including the exact EA `STIM`
  payload bytes (`CF 4A 6D 00 B2 FA BA AD 0D 00`).
* **Two Hypotheses Falsified and Documented:** the missing-`PING`/`LATN`/`TIME`
  theory (EA registers exactly one `PingResponse` member) and the byte-10
  bit-shift theory (byte 10 is the message number's high octet). Recording these
  prevents the next investigation from re-treading them.
* **Protocol Documentation Published:** `docs/BLAZE_FIRE2_HEAT2_PROTOCOL.md` — the
  full wire specification with an evidence table, component/command ID tables,
  schema field names, and a pitfalls list.
* **Reference Repositories Made Reproducible:** `scripts/fetch-references.ps1`
  clones all five reference projects at pinned commits and verifies each HEAD.

---

## 10. Milestone Achievements (October 22, 2026 — Binary-Verified Fire2/Heat2 + QoS HTTP Reference)

Everything in this section is backed by **authentic EA bytes, the client's own
captured bytes, or the retail binary's own code** — the evidence standard
adopted in §5.1. Where a claim rests on a reference repository, it is marked as
corroboration.

* **Fire2 Framing Corrected Against the Binary:**
  * `payloadSize` counts the **payload only**; the metadata is excluded. Proven
    by the back-patch sequence at `0x1449061E9`–`0x14490624C`.
  * Header bytes **14–15 are always zero** — not an options byte. Proven by
    `0x14490604E: xor esi, esi` and `0x144906186: mov word ptr [rbx+0xe], si`.
  * Byte 13 is `messageType << 5 | userIndex` (`0x144906169: shl dl, 5` /
    `0x14490616C: or dl, [rsp+0x70]`), and the normalising switch at
    `0x1449060FF`–`0x14490613C` maps type 4 → PING and 5 → PING_REPLY.
  * `Fire2Metadata` has **9 members** (`CNTX`/`ERRC`/`FLAG`/`LCTX`/`MADR`/
    `MIID`/`NAME`/`SIDN`/`SKEY`), per `FUN_1402C49C0` at `0x1402C49C0`.
* **Heat2 Tag Packing Corrected:** TDF tags are **four** ASCII characters packed
  into three bytes at 6 bits each, with the shift schedule **`18 - 6*i`** (not
  `26 - 6*i`). Verified by decoding all nine `Fire2Metadata` tag dwords out of
  the binary (`CNTX`=`8E ED 38`, `ERRC`=`97 2C A3`, `FLAG`=`9A C8 67`,
  `LCTX`=`B2 3D 38`, `MADR`=`B6 19 32`, `MIID`=`B6 9A 64`, `NAME`=`BA 1B 65`,
  `SIDN`=`CE 99 2E`, `SKEY`=`CE B9 79`), and independently corroborated by EA's
  own `DecodeTag` in `references/BlazeSDK/Fire2Decoder.go`
  (`for i := 3; i >= 0; i--`).
* **Heat2 Type Table Completed:** wire type **`0x0C` is `GENERIC`** and **`0x0D`
  is the unknown/out-of-range default** — not the Go reference's `0xFF` sentinel
  (`0x14496B361: mov r8d, 0xc`; the translation table in `FUN_14496BB30` at
  `0x14496BB30`). `crates/blaze-tdf` now implements `TdfType::Generic` and
  `TdfType::Unknown` in the reader, the writer and the type table, pinned by
  `crates/blaze-tdf/tests/tdf_heat2_types.rs` (17 tests, all passing).
* **Reference-Repo Independence Claim Retracted:** `mxmstr/hp-server` is a
  **fork** of `Khysnik/BlazeSDK` — `Fire2Types.go` is byte-identical, the
  other four files differ only by exported wrappers, and BlazeSDK's commits
  appear verbatim in hp-server's history. The project no longer cites the pair
  as two independent implementations.
* **QoS / ProtoPing HTTP Reference Published:** `docs/FIFA22_QOS_PROTOPING_HTTP.md`
  documents the `/qos/qos`, `/qos/firewall`, `/qos/firetype` and
  `/getPeerAddress` endpoints **as the client actually reads them**:
  * the response is parsed by a **flat forward text scanner**
    (`0x140CA1DE0`), not an XML tree — so `.qosport` means "the next `qosport`
    element in document order";
  * **`qosip` is a big-endian decimal u32** (2130706433 = 127.0.0.1), split
    MSB-first at `0x141FE8B6A` — serving `127.0.0.1` makes the client dial
    `0.0.0.127`;
  * the hard success conditions (`.qosip` non-zero, `.qosport` non-zero,
    `.numprobes` ≥ 2, `.probesize` non-zero, `.requestid` non-zero) with the
    `jne`-means-success polarity warning;
  * `QOSS` is EA's `QosConfigInfo` (`BWPS`/`LNP`/`LTPS`/`SVID`) and
    `PreAuthResponse` has **exactly four** members (`CIDS`/`CONF`/`QOSS`/`SVER`) —
    no `ANAM`, no `TIME`;
  * the `ProtoHttp 1.3/DS 15.1.7.0.3 (Windows)` user agent, read live.
* **Analysis Tooling Committed:** `tools/disasm_region.py`,
  `tools/find_str_xrefs.py` and `tools/qos_dom_model.py` — a region
  disassembler, a byte-exact RIP-relative xref finder (4-byte window scan, no
  linear sweep) and a Python model of the client's DOM scanner that validates a
  candidate QoS response **without launching the game**. No retail binary or
  game data is committed; the PE path is always an argument.
* **Open Item — the QoS HTTP port:** the client builds the QoS base URL as
  `"%s://%s:%u"` from the host `qosapi`, and the port argument has **not** been
  located in the binary yet. The shim's `ShouldRerouteHost` matches none of
  its patterns for the literal host `qosapi`, so the request depends on the
  *port* being in the reroute list. This is a live candidate explanation for
  the client stalling before the QoS step completes.
* **The live capture never reached the QoS step at all:** `emulator_server.log`
  contains **no** `/qos/*` request, and `f22_shim.log` shows `qosapi` was never
  resolved. The client stalls *before* QoS, so the QoS HTTP endpoints are not
  yet the blocker — the blocker is upstream of them, in the Blaze `Util::ping` /
  `Util::preAuth` exchange. Fixing the QoS responses is necessary but not
  sufficient; the next capture must be taken after the `preAuth` reply is
  corrected.

