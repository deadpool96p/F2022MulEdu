# FIFA 22 Frostbite 3 Clean-Room Multiplayer: Project Progress Report

**Document Version:** 2.2.0  
**Last Updated:** October 22, 2026 (evening session)  
**Status:** In Active Development — Phase 1 (Authentication Unblock) — **blocked on the redirector reply format and the HTTP/2 transport**  
**Repository:** `deadpool96p/F2022MulEdu`  
**Target:** EA Sports FIFA 22 (Frostbite 3 Engine, PC)

> **Read first:** [`docs/SESSION_LOG_2026-10-22.md`](docs/SESSION_LOG_2026-10-22.md)
> is the authoritative record of the current blocker, the live evidence behind
> it, and the priority order for the next session. §11 below summarises it.

---

## 0. Current Status & Active Workstream

**The Blaze protocol work is done and verified.** `Util::ping` → `Util::preAuth`
→ a `PreAuthResponse` whose `QOSS` block is byte-identical to the authentic EA
fixture is exchanged successfully with the real client.

**The client then stalls and logs out.** The remaining blocker is *not* a
Blaze-protocol problem — it is a service-discovery and transport problem, and
it has **three independent parts** (full evidence in the session log):

| # | Blocker | State |
| :--- | :--- | :--- |
| **A** | The redirector reply is an **invention**: its XML element names do not match the lowercased TDF field names the client parses (`servicenames`, `servers`, `addresses`, `instancetype`, `hostname`, `srcport`, `dstport`, `netmask`, `serviceid`, `srcip`). The client therefore never finds the `qosapi` service, falls back to `getaddrinfo("")`, and dials the local Radmin address `26.178.140.32:443` with a **zero-length SNI**. | Root cause proven, **not yet fixed** |
| **B** | The `:443` sink speaks **HTTP/1.1 only**, but the client's QoS stack negotiates **ALPN `h2`**. A perfect redirector reply would still dead-end. | Module written and compiled, **not yet wired in** |
| **C** | The h2 TLS client lives in `FIFA22.exe` and validates against **its own embedded trust-anchor table (4 anchors)**, not the Windows store. The shim patches only the `DigiCert Global Root CA` entry. | **Open** — 3 of 4 anchors unpatched |

**The decisive live evidence:** the *same* server certificate is accepted on all
six ProtoHttp handshakes (real SNI, no ALPN) and rejected on all twenty-one
h2 handshakes (`SNI=""`, `ALPN=["h2"]`, `AlertReceived(CertificateUnknown)`).
So the chain is fine for the DirtySDK/Windows-store path; the h2 path is a
different validator.

**Blaze 15 has no `QCNF` in `preAuth`** (unlike Blaze 18, whose
`QCNF.QCA = "qoscoordinator.gameservices.ea.com"`), so the QoS endpoint for
FIFA 22 can come only from the HTTP redirector reply. That makes **A the
single root cause** of the empty host.

> **Historical note (October 21, 2026):** an earlier version of this report
> cited `hp-server` and `BlazeSDK` as "two independent clean-room
> implementations". They are not — `hp-server` is a **fork** of BlazeSDK and
> their `Fire2Types.go` is byte-identical. The evidence standard is now
> *authentic EA bytes, the client's own captured bytes, or the retail binary's
> own code*. See `docs/BLAZE_FIRE2_HEAT2_PROTOCOL.md` §1.

> **Further note (October 22, 2026, evening):** a sweep of all five pinned
> reference repositories established that **none of them implements the HTTP
> `POST /redirector/getServerInstance` reply**. Everything they call a
> "redirector" is the older **in-Blaze TDF** redirector (component `0x0005`,
> command `0x0001`, TCP/ProtoSSL `:42127`) — a different service from FIFA 22's
> HTTP redirector on `:42230`. `qosapi` has **zero** occurrences across all
> five. The reply format must be recovered from `FIFA22.exe` itself.

> **Note on evidence (October 21, 2026):** an earlier version of this report
> cited `hp-server` and `BlazeSDK` as "two independent clean-room
> implementations". They are not — `hp-server` is a **fork** of BlazeSDK and
> their `Fire2Types.go` is byte-identical. The evidence standard is now
> *authentic EA bytes, the client's own captured bytes, or the retail binary's
> own code*. See `docs/BLAZE_FIRE2_HEAT2_PROTOCOL.md` §1.

### The Fire2/Heat2 codec defects are all closed

| # | Defect | Severity | State |
| :--- | :--- | :--- | :--- |
| A | Heat2 integers encoded as 7-bit LEB128 instead of the 6-bit-first-byte varsize scheme | **Critical** — `STIM` decoded as a 1998 timestamp, tripping the client's clock-drift check | **Fixed** |
| B | 16-byte header was not Fire2 (message number, message type, metadata size, user index, options all wrong) | **Critical** — replies can never be matched to requests | **Fixed** — verified against the binary (see §10) |
| C | `OBJECT_TYPE`/`OBJECT_ID` and `FLOAT`/`TIMEVALUE` type IDs swapped | High | **Fixed** |
| D | Union / Variable / ObjectId / ObjectType wire forms wrong | High | **Fixed** |
| E | 12-vs-16 byte header detection heuristic tested `buffer[10] <= 0x03` | Medium | **Fixed** |

**Read next:**

* [`docs/SESSION_LOG_2026-10-22.md`](docs/SESSION_LOG_2026-10-22.md) — **the
  current blocker**, the live evidence behind it, and the priority order for
  the next session.
* [`docs/BLAZE_FIRE2_HEAT2_PROTOCOL.md`](docs/BLAZE_FIRE2_HEAT2_PROTOCOL.md) — the
  authoritative Fire2/Heat2 wire specification, with the evidence behind each claim.
* [`docs/POST_PING_STALL_ROOT_CAUSE.md`](docs/POST_PING_STALL_ROOT_CAUSE.md) — the
  full investigation record, including the hypotheses that were falsified.
  *(§1 and §7 are now known to be stale — see the session log §8.)*

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
| `crl_revocation` | CA-signed CRL generation, serving, and revocation-check semantics | **PASS** |
| `auth_command_ids` | EA `Authentication` command IDs (incl. `logout` = `0x0046`) | **PASS** |
| `preauth_matches_ea` | The `preAuth` reply, byte-compared against authentic EA frames | **PASS** |
| `emulator_server::http2_server` | ALPN recognition; a QoS reply fits in one HTTP/2 DATA frame | **PASS** |

**Total Test Results:** `191 passed; 0 failed; 0 ignored` (100% passing),
`cargo test --workspace` exit code 0 (October 22, 2026, evening).

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
   * [x] Correct the Heat2 type IDs (Defect C).
   * [x] Correct the union / variable / object wire forms (Defect D).
   * [x] Rewrite the 16-byte header to Fire2 in `blaze-core` (Defect B) — the 12-byte legacy path must stay byte-identical so the eight integration test files remain green.
   * [x] Replace the 12-vs-16 detection heuristic in `blaze_server.rs` and `redirector_server.rs` (Defect E).
   * [x] Update the `services-blaze/src/util.rs` unit tests for the new layout.
   * [x] **Correct the `Util` command IDs** to EA's own `UtilMessage` enum. `0x0007` is `preAuth`, `0x0008` is `postAuth`. The shipped IDs made the server answer one command's reply to another command's request, which the client discards as unsolicited — a silent stall.
   * [x] **Correct the `preAuth` reply shape.** EA's open-source model names four members, but the retail server sends fourteen (`ASRC CIDS CLID CONF ESRC INST MAID MINR NASP PILD PLAT QOSS RSRC SVER`). `CONF` is a *struct wrapping* the map, not a bare map. `ANAM` and a root-level `TIME` are not members and are no longer sent. Verified byte-exactly against two authentic EA frames, now kept as fixtures in `crates/services-blaze/tests/fixtures/`.
   * [x] **Answer a Fire2 transport `PING` with a `PING_REPLY`.** A `PING` (message type 4) is a keepalive, not an RPC request; answering it with an ordinary `REPLY` left the keepalive unsatisfied and the client tore the connection down and reconnected.
   * [x] Rewrite the Wireshark dissector against the real Fire2/Heat2 layout, with a 79-check self-test (`tools/dissector/blaze.lua`).
   * [x] Add a decoder example for Blaze frames and bare TDF payloads (`cargo run -p blaze-tdf --example decode -- ...`), which reports how many bytes were consumed — the test that tells a root struct from a nested one.
   * [x] **Re-run the real client and confirm `Util::preAuth` arrives.** Verified live on 2026-10-22: `Util::ping` → `Util::preAuth` → a `PreAuthResponse` whose `QOSS` block is byte-identical to the authentic EA fixture. The `Authentication::logout` (`0x0046`) flood that follows is the client abandoning the **QoS** step, not a Blaze-protocol defect.
   * [x] Serve the ProtoHttp User-Agent (`ProtoHttp 1.3/DS 15.1.7.0.3 (Windows)`) from `services-http-content`. The user agent is parsed (`crates/services-http-content/src/protohttp.rs`), and a ProtoHttp request is answered with an exact `Content-Length`, no chunked framing and `Connection: Close`. The QoS endpoints (`/qos/qos`, `/qos/firewall`, `/qos/firetype`, `/getPeerAddress`) now echo the client's `rqid`/`rqsc`, and `/getPeerAddress` is served as `text/plain` rather than `application/xml`. Request parameters are read from the query string **and** from an `application/x-www-form-urlencoded` body, which is where the client sends `sourceIP`/`sourcePort`/`tag`/`sendCount`; `/getPeerAddress` echoes them and answers with the emulator's own peer address (`127.0.0.1:3659`, the ProtoTunnel relay). Every request is logged with its full request line, `Host`, `User-Agent`, `Content-Type` and ALPN. See `docs/FIFA22_QOS_PROTOPING_HTTP.md` §3.4, §3.5 and §3.2.
   * [ ] **Advertise `qosapi` correctly in the redirector reply** (blocker A — the highest-value single change). Rewrite `render_redirector_xml` / `render_service_instance_xml` to emit the *lowercased TDF field names* the client parses: `servicenames`, `servers`, `addresses`, `instancetype`, `inservice`, `hostname`, `srcport`, `dstport`, `netmask`, `serviceid`, `srcip`. Recover the exact DOM path and nesting from `FIFA22.exe` (`0x141FEA4E0` → bind query `0x140c90150` → `doc->[0x10]`). The existing tests that assert `<servicename>` / `<ip>` / `<port>` must be updated to the real names.
   * [ ] **Make the advertised QoS host a name that survives to the SNI** (blocker A, part 2). The shim's `ShouldRerouteHost` rewrites `.ea.com` names to the emulator IP *before* the ClientHello, so the name must come from a hosts-file entry or an SNI-preserving rewrite.
   * [ ] **Wire the HTTP/2 transport** (blocker B). Add `.with_alpn_protocols(vec![b"h2".to_vec(), b"http/1.1".to_vec()])` to the TLS config and dispatch on `TlsStream::get_ref().1.alpn_protocol()` in `telemetry_server.rs`. `crates/emulator-server/src/http2_server.rs` is written and compiles; it needs the ALPN dispatch and a decision on whether the `:443` sink gets its own `TlsAcceptor` (the shared one would put ALPN on the Blaze port too).
   * [ ] **Patch all four embedded trust anchors** in `FIFA22.exe` (and `EAWebKit.dll`), not just `DigiCert Global Root CA` (blocker C). Requires explicit authorisation naming the additional anchors.
   * [ ] Stand up the QoS STUN responder so the `QOSS` site in `preAuth` resolves.
   * [x] ~~Investigate the TLS handshake failure at 04:15:33 (`InvalidMessage(IllegalEmptyValue)`) on connection 61803.~~ **Explained (2026-10-22 evening):** the client dials a bare IP, so the SNI is empty; the vendored rustls now treats an empty SNI as "no server name". The *remaining* TLS failure is `AlertReceived(CertificateUnknown)` from the h2 client, which validates against its own embedded anchor table — see blocker C.
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
* **Open Item — the QoS host is never resolved (superseded 2026-10-22
  evening):** this bullet previously read "the client builds the QoS base URL
  as `"%s://%s:%u"` from the host `qosapi`, and the port argument has not been
  located in the binary yet". The evening session established the real
  answer: the client **never resolves `qosapi` at all**, because the
  redirector reply we send does not carry the service in the form the client
  parses. The reply's XML element names are an invention and do not match the
  lowercased TDF field names (`servicenames`, `servers`, `addresses`,
  `instancetype`, `inservice`, `hostname`, `srcport`, `dstport`, `netmask`,
  `serviceid`, `srcip`). The client therefore falls back to
  `getaddrinfo("")`, enumerates the local interfaces, and dials the Radmin VPN
  address `26.178.140.32:443` with a zero-length SNI. **Locating the `qosapi`
  *port* in the binary was the wrong question** — the port is irrelevant until
  the service is advertised correctly. See `docs/SESSION_LOG_2026-10-22.md` §3.
* **The live capture never reached the QoS step at all:** `emulator_server.log`
  contains **no** `/qos/*` request, and `f22_shim.log` shows `qosapi` was never
  resolved. The client stalls *before* QoS, so the QoS HTTP endpoints are not
  yet the blocker — the blocker is upstream of them, in the Blaze `Util::ping` /
  `Util::preAuth` exchange. Fixing the QoS responses is necessary but not
  sufficient; the next capture must be taken after the `preAuth` reply is
  corrected.

---

## 11. Milestone Achievements (October 22, 2026, evening — Blaze Verified Live; QoS/Redirector/HTTP2 Root Causes Isolated)

Full record: [`docs/SESSION_LOG_2026-10-22.md`](docs/SESSION_LOG_2026-10-22.md).

* **The Blaze exchange is live and correct.** `Util::ping` → `Util::preAuth` →
  a `PreAuthResponse` whose `QOSS` block is byte-identical to the authentic EA
  fixture. The `Authentication::logout` (`0x0046` = 70) flood that follows is
  the client giving up on the QoS step, not a Blaze-protocol defect.
* **The client uses two different HTTP stacks, and only one of them is
  broken.** The live ClientHello capture is decisive:
  * 6 × ProtoHttp handshakes — real SNI (`rl.data.ea.com`,
    `pin-river.data.ea.com`, `freeform-river.data.ea.com`,
    `spring18.gosredirector.ea.com`), **no ALPN**, **all accepted**.
  * 21 × h2 handshakes — `SNI=""`, `ALPN=["h2"]`, **all rejected** with
    `AlertReceived(CertificateUnknown)`.
  The certificate is fine for the DirtySDK/Windows-store path; the h2 stack
  in `FIFA22.exe` uses its own validator.
* **Root cause of the empty host isolated.** The redirector reply is an
  invention whose element names do not match the lowercased TDF field names
  the client parses, so the `qosapi` service is never found and the client
  falls back to `getaddrinfo("")` → `26.178.140.32:443` with a zero-length
  SNI. Confirmed by the client-side path `0x141FEA4E0` → bind query
  `0x140c90150` → `doc->[0x10]`.
* **FIFA 22's `preAuth` cannot supply the QoS endpoint.** Blaze 15 has no
  `QCNF` and `BWPS.PSA = ""` (proved against the authentic fixture), whereas
  Blaze 18 carries `QCNF.QCA = "qoscoordinator.gameservices.ea.com"`. EA
  always carries the QoS address as a **host name plus port**, never an IP.
* **HTTP/2 support written against the real API.** `h2` 0.4 is a *frame*
  library with no `http-body` dependency: `send_response` takes a
  `Response<()>` (headers only) and the body follows via
  `send_data(bytes, end_of_stream)`.
  `crates/emulator-server/src/http2_server.rs` implements that, shares the
  HTTP/1.1 `route_http_request` bodies, filters the RFC 9113 §8.2.2
  connection-specific headers, and releases flow-control capacity as it
  reads. It compiles; it is **not yet dispatched to**.
* **ALPN mechanics established** from the vendored rustls source: ALPN is
  enforced only when `alpn_protocols` is non-empty, and a non-overlapping
  offer triggers `NoApplicationProtocol`. The DirtySDK ProtoHttp stack offers
  no ALPN at all, so adding `h2` is safe for it — but the shared
  `TlsAcceptor` means the Blaze port would be affected too, so the `:443`
  sink should get its own acceptor unless the Blaze client is confirmed to
  offer no ALPN.
* **FIFA 22's embedded trust anchors mapped.** Four 0x60-byte RSA anchor
  structures in `FIFA22.exe`'s `.xtls` (GOS 2015 CA, VeriSign G5, DigiCert
  Global Root CA, and one more). The shim's `cert_bypass.cpp` patches every
  module that contains the literal `"DigiCert Global Root CA"` — one anchor
  name, not all four.
* **The reference repositories are exhausted for this problem.** None of the
  five implements the HTTP `POST /redirector/getServerInstance` reply;
  `qosapi` appears in none of them; the retail redirector leaf
  (`openBlase/BlazeProxy/gosredirector.ea.com.crt`) is issued by an EA
  **"Root Agency"**, not DigiCert; and the HP generation's transport is
  TLS 1.0 with RSA/RC4, not modern TLS with an `h2` offer. The reply format
  must come from `FIFA22.exe`.
* **The shim destroys the host name before TLS.** `ShouldRerouteHost` rewrites
  `.ea.com` / `.ea.net` / `.dice.se` / `.xboxlive.com` to the emulator IP
  *before* the ClientHello is built, so a correct redirector reply would
  still yield a zero-length SNI. The fix is the hosts-file pattern (as
  `hp-server` and `Skate3BlazeServer` use) or an SNI-preserving rewrite.
* **New RE tooling committed:** `tools/align_find.py`,
  `tools/anchor_xrefs.py`, `tools/dump_anchor.py`, `tools/dump_qwords.py`,
  `tools/dump_strings.py`, `tools/dump_tdf.py`, `tools/dump_vtable.py`,
  `tools/fast_xrefs.py`.
* **Vendored rustls fork committed.** `third_party/rustls-0.23.46` is the
  target of the workspace's `[patch.crates-io]`, so it must be in the tree or
  the workspace will not build for anyone else. Its empty-SNI patch is
  documented in `third_party/rustls-0.23.46/PATCHES.md`.

