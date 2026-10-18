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

**Total Test Results:** `34 passed; 0 failed; 0 ignored` (100% passing).

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

## 6. Next Steps & Roadmap

1. **Host/Guest Match Kickoff:**
   * Validate squad hash negotiation between Player 1 and Player 2 before match initialization.
   * Verify deterministic match seed exchange to ensure identical weather, pitch, and physics seeds.
2. **Input Frame Rate Optimization:**
   * Fine-tune 60 Hz virtual gamepad injection to maintain sub-16 ms input delivery.
3. **P2P NAT Traversal Polish:**
   * Complete automated fallback from direct UDP STUN punch-through to ProtoTunnel relay (Port 3659) when playing across restrictive WAN NAT configurations.
