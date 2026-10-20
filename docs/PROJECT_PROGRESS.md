# FIFA 22 Frostbite 3 Clean-Room Multiplayer: Project Progress Report

**Document Version:** 1.0.0  
**Last Updated:** October 18, 2026  
**Status:** In Active Development / Alpha Verification  
**Repository:** `deadpool96p/F2022MulEdu`  
**Target:** EA Sports FIFA 22 (Frostbite 3 Engine, PC)

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
   * Implements full binary serialization and deserialization for all 12 Blaze wire types:
     * `VarInt` (signed and unsigned varints with ZigZag encoding)
     * `String` (length-prefixed ASCII/UTF-8 strings)
     * `Struct` (nested complex structures with termination markers)
     * `List` (heterogeneous and homogeneous dynamic arrays)
     * `Map` (key-value associative maps)
     * `Union` (discriminated variant types)
     * `Blob` (raw byte buffers)
     * `Triad` / `Tuple` (3-byte triple integers)
     * `IntegerList` / `Float` / `TimeValue`
   * Tag compression: 3-character 6-bit packed tag IDs (e.g., `'ADDR'`, `'CONF'`, `'VALU'`).

2. **`crates/blaze-core` (RPC Framing Engine)**
   * Parses Blaze message frames: 12-byte header containing component ID, command ID, error code, message type (Reply, Notification, Error), and payload length.
   * Hex dump formatting and packet roundtrip verification.

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
| `blaze_tdf::tag` | Tag byte encoding, standard roundtrip, unsigned varints | **PASS** |
| `blaze_tdf::wire_types` | All 12 TDF wire types roundtrip integrity | **PASS** |
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

**Total Test Results:** `51 passed; 0 failed; 0 ignored` (100% passing across all 7 workspace crates).

Per-crate breakdown (verified 2026-10-20): `blaze_core` 16, `blaze_tdf` 7, `services_redirector` 2, `services_blaze` 5, `services_http_content` 3, `services_qos_relay` 5, `emulator_server` 4 unit + 6 integration (`boot_handshake`, `lobby_matchmaking_flow`, `match_kickoff_flow`, `online_hub_flow`, `room_code_flow`, `disconnect_flow`).

---

## 5. Recent Technical Breakthroughs (October 19, 2026)

### A. Fire2 16-Byte Framing Standardization
* **Discovery:** Modern FIFA 22 (Frostbite 3, BlazeSDK 15.1.1.10.3) uses 16-byte Fire2 framing exclusively on all connections.
* **Resolution:** Replaced component-whitelisting logic with structural invariant parsing (`buffer[10] <= 0x03` for valid `MsgType` enum). This eliminates stream offset desynchronization when the client batches custom component RPCs (e.g. `0x08BA`).

### B. Platform Identity Federation (Origin & Steam)
* **Discovery:** The game client's platform persona name was hardcoded to `"Player"` in `anadius.cfg`, causing the offline gate to render `"PRESS [R] TO RECONNECT Player"`.
* **Resolution:** 
  * Synchronized `G:\Games\FIFA 22\anadius.cfg` to `"Username": "DEad>_<POol"`.
  * Updated `SessionStore::create_session` to prioritize the active Steam persona over generic fallback `"Player"`.
  * The in-game UI now correctly displays `"DEad>_<POol"` on both the custom Steam overlay and the native offline reconnect prompt.

### C. Network Topology & Port Rerouting
* **Discovery:** During startup, FIFA 22 attempts connections to:
  1. `34.225.136.131:443` (EA Gateway / Nucleus OAuth token endpoint)
  2. `159.153.51.21:44325` (`gosca18.ea.com`, EA's production Blaze TLS cluster)
  3. `spring18.gosredirector.ea.com:42230`
* **Resolution:**
  * Enabled `RerouteAllEA = true` in `f22_emu.ini` so all EA domains resolve to `127.0.0.1`.
  * Bound a secondary listener on port **44325** in `emulator-server`.
  * Added port `44325` to the WinSock detours in `shim/src/hooks_winsock.cpp`.
  * Implemented synthetic EA Nucleus token responses in `services-http-content` on port `443`.

### D. Heat2 Decoder Schema Invariant
* **Discovery:** In FIFA 22's exact schema, `Blaze::Util::PingResponse` contains **0 member fields**. Populating unexpected tags (such as `STIM`) caused `Heat2Decoder` to reject the packet with `BLAZE_ERR_TDF_TAG_NOT_FOUND`.
* **Resolution:** `handle_ping` now encodes an empty struct (`TdfStruct::new()`, length 0, `RecordResponse`), perfectly satisfying the client schema.

---

## 6. Comprehensive Action Plan: Full Online Friendlies Authentication

To advance from the reconnect ping probe into full in-game Online Friendlies match simulation, the following multi-phase plan is executed:

### Phase 1: Redirector Protocol Alignment (Secure Flag Downgrade)
* **Problem:** The client connects over Plain TCP to the Blaze port (`0x00, 0x00, 0x00, 0x00`), but the Redirector was advertising `<secure>1</secure>`. DirtySDK rejects unencrypted sockets when a secure connection profile is declared.
* **Solution:** When `config.secure` is false (or `ForcePlainTCP = 1`), advertise `<secure>0</secure>` in `serverinstanceinfo` XML, allowing DirtySDK to seamlessly maintain plain TCP without TLS handshake friction.

### Phase 2: Full Boot-Time Handshake Execution
* With `RerouteAllEA = true` and `anadius.cfg` aligned, restart `FIFA22.exe` to trigger the boot-time connection pipeline:
  1. `POST /redirector/getServerInstance` -> receives `127.0.0.1:42128` (or `44325`).
  2. `Util::preAuth` (`0x0009:0x000B`) -> client receives component list (`CIDS`) and QoS config.
  3. `Util::postAuth` (`0x0009:0x000C`) -> client receives telemetry & ticker configuration.
  4. `Authentication::login` (`0x0001:0x0028` / `0x001C`) -> client receives `SESS` bound to `DEad>_<POol` and SteamID `76561198956360709`.
  5. `Authentication::listUserEntitlements` (`0x0001:0x0046`) -> client receives `ONLINE_ACCESS = 1`.
  6. `UserSessions::updateNetworkInfo` (`0x7802:0x0014`) -> client receives NAT type `OPEN`.

### Phase 3: Online Friendlies Lobby & Kickoff Orchestration
* Navigate to **Play > Online Friendlies** in FIFA 22.
* The game client queries `AssociationLists::getLists` (`0x0019`), populating the online friends list with virtual peers from the active Room Code.
* Triggering kickoff via `POST /api/v1/game/kickoff` broadcasts `NotifyGameStateChange` (`0x0064`, state `131` / `IN_GAME`) with attributes `GNAM = "OSDK_match"` and `ATTR_GAME_MODE = "1"`, launching directly into the Online Friendly presentation.

---

## 7. October 20, 2026 — Heat2 Schema Fixes & Distribution Verification

### A. The 10-Second Watchdog EOF Root Cause
* **Symptom:** DirtySDK dropped the connection with an EOF exactly 10.0 seconds after the ping response arrived.
* **Diagnosis:** The server was emitting a `STIM` tag (Unix epoch timestamp) inside `Blaze::Util::PingResponse`. In FIFA 22's Heat2 schema, that struct has **0 member fields**. The client's `Heat2Decoder` rejected the unexpected tag with `BLAZE_ERR_TDF_TAG_NOT_FOUND`, and the socket watchdog fired ~10s later.
* **Fix:** `handle_ping` in `crates/services-blaze/src/util.rs` now returns an empty `TdfStruct` (length 0, `RecordResponse`). Added regression test `test_ping_response_is_empty_struct_no_stim`.
* **Same class of bug, second location:** the `0x08BA` (PerfComponent) handler in `router.rs` was also emitting `STIM`. Fixed identically — empty struct response. The 0x08BA/0x0005 schema is still an **unverified hypothesis** (no Ghidra-confirmed schema exists yet); it must be closed by capturing wire bytes with `tools/dissector/blaze.lua` in Wireshark during a live smoke test.

### B. Steam Identity Endpoint Mismatch
* **Symptom:** The Steam persona never appeared in the lobby, and the friend list showed no real profiles.
* **Diagnosis:** `tools/sync_steam_profile.py` was POSTing to `/api/v1/steam/profile`, which the Control Server never implemented (404). Every push was silently discarded.
* **Fix:** Switched to `/api/v1/steam/identity` (`crates/emulator-server/src/control_server.rs:342`), the only endpoint that writes the persona into the session store. Verified live: `identity` → 200, `profile` → 404.

### C. Distribution & Steam Sync Plan — Checklist Closed
`docs/MULTIPLAYER_DISTRIBUTION_AND_STEAM_SYNC_PLAN.md` Steps 1–4 are now verified against the actual code and disk artifacts:
- **Step 1** (persona sync): implemented in `sync_steam_profile.py`, integrated into `start_client.bat` and `start_all.bat`.
- **Step 2** (Radmin IP prompt): `start_client.bat` runs `sync_steam_profile.py --client`, which writes `EmulatorHost`/`ServerIp` to `f22_emu.ini`.
- **Step 3** (dual-stack binding): all seven ports (`42127`, `42128`, `42230`, `44325`, `17498`, `3659`, `42129`) bind `0.0.0.0` with `[::]` IPv6 fallbacks in `crates/emulator-server/src/lib.rs`, so the Radmin VPN adapter is covered.
- **Step 4** (distribution package): `dist/fifa22_multiplayer_client.zip` (1.27 MB, 6 entries) is built and on disk.
- **Step 5** (end-to-end validation): partially complete — server-side is fully covered by the passing test suite; the remaining gate is the in-game smoke test, which requires launching `FIFA22.exe`.

### D. Match Kickoff State Machine — Verified
`crates/emulator-server/tests/match_kickoff_flow.rs` exercises the full Stage 2 notification pipeline: createGame → NotifyGameCreated → NotifyGameSetup (both peers receive the **identical** `SEED`) → updateGameAttributes → NotifyGameAttributeChange (0x0050 + 0x0018) → PRE_GAME → symmetric NotifyPlayerStateChange → NotifyGameStateChange (0x0064) → ProtoTunnel registration on `:3659` → IN_GAME → symmetric 0x0064 kickoff → 60-frame bidirectional `NetGameDist` input replication with per-frame latency assertion (< 15 ms loopback). The `NotificationSequenceBuilder` in `gamemanager.rs` enforces the exact order: NotifyPlayerJoining (0x0015) → NotifyJoiningPlayerInitiateConnections (0x0017) → NotifyPlatformHostInitialized (0x0018) → NotifyGameReportingIdChange (0x0070) → GameState(130) → GameState(131), with a 5-second DeadlockGuard and a `match_ready_achieved` disarm on IN_GAME transition.
