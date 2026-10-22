# Session Log — 2026-10-22 (evening)

**Working period:** ~17:00 – 23:45 IST
**Branch:** `main`
**Session focus:** the QoS / HTTP-2 blocker on `:443`, and the redirector reply that
causes the client to dial a bare IP.

This is a *pause* log. It records what was proven, what was built, and what is
still open, so the next session can start from the evidence rather than from
the hypothesis.

> **Evidence standard (unchanged from `PROJECT_PROGRESS.md` §5.1):** every claim
> below is backed by *authentic EA bytes*, *the client's own captured bytes*
> (`emulator_server.log`), *the retail binary's own code* (`FIFA22.exe`), or is
> explicitly marked as a hypothesis.

---

## 1. Executive summary

The 10-second stall that blocked Phase 1 is **not** a Blaze-protocol problem.
The Blaze side is now working: `Util::ping` → `Util::preAuth` → a
`PreAuthResponse` carrying a **byte-identical** `QOSS` block → then silence.

The silence has two independent causes, and both must be fixed before the
client will ever reach `Util::postAuth`:

| # | Blocker | Status | Where it is fixed |
| :-- | :--- | :--- | :--- |
| **A** | The redirector reply is an *invention*. Its XML element names do not match the TDF field names the client lower-cases, so the client never finds the `qosapi` service. It falls back to `getaddrinfo("")` and dials the local Radmin VPN address `26.178.140.32:443` with a **zero-length SNI**. | **Root cause proven, not yet fixed** | `crates/emulator-server/src/redirector_server.rs` |
| **B** | The `:443` sink speaks HTTP/1.1 only. The client's HTTP/2 stack offers ALPN `h2`, and the sink explicitly refuses an h2 preface. Even a perfect redirector reply would dead-end here. | **Module written, not yet wired in** | `crates/emulator-server/src/http2_server.rs` |

And a third issue sits behind both:

| # | Blocker | Status |
| :-- | :--- | :--- |
| **C** | The h2 TLS client lives in `FIFA22.exe` and validates against **its own embedded trust-anchor table** (4 anchors), not the Windows store. The shim patches only the `DigiCert Global Root CA` entry. | **Open — 3 of 4 anchors unpatched** |

**Ordering matters:** A must be fixed first (otherwise the client never asks
for QoS over h2 at all), B must be fixed in parallel (otherwise A buys nothing),
and C must be fixed for the h2 handshake to complete at all.

---

## 2. The failure chain, as observed

`emulator_server.log`, 2026-10-22 17:29 – 18:04 (the last live run):

```
17:29:51  [DualListener] ClientHello from 127.0.0.1:65265: 295 bytes, SNI="", ALPN=["h2"]
17:29:51  rustls::msgs::handshake: Empty SNI hostname received: treating it as no server name
17:29:51  [DualListener] TLS handshake FAILED with 127.0.0.1:65265:
          Custom { kind: InvalidData, error: AlertReceived(CertificateUnknown) }
```

Aggregated over the whole log:

```
  21 ×  SNI=""                        ALPN=["h2"]   →  all FAILED (CertificateUnknown)
   3 ×  SNI="freeform-river.data.ea.com"  ALPN=[]  →  OK
   1 ×  SNI="rl.data.ea.com"              ALPN=[]  →  OK
   1 ×  SNI="pin-river.data.ea.com"       ALPN=[]  →  OK
   1 ×  SNI="spring18.gosredirector.ea.com" ALPN=[] →  OK
```

### 2.1 Why the Blaze side stalls

```
17:09:43  [Blaze Server] Parsed Fire2 frame: Comp=0x0001, Cmd=0x0046, Seq=0, Len=0
17:09:43  [Blaze Auth] Handled logout
          … repeated 12× …
```

`0x0046` is EA's `Authentication::logout` (70) — **not**
`listUserEntitlements`, as the code originally assumed; the retail binary's
`AuthenticationMessage` table settles this. The client is logging out because
it never got past the QoS step, and it retries on a 5-minute cadence (the h2
handshakes above are exactly 5 minutes apart).

The Blaze exchange itself is *healthy*: `preAuth` is answered, a session key is
issued, and the reply's `QOSS` block is byte-identical to the authentic EA
fixture. The client simply refuses to advance without the QoS answer.

### 2.2 The decisive observation

**The same server certificate is accepted on six ProtoHttp handshakes and
rejected on all twenty-one h2 handshakes.** The only differences are (i) the
zero-length SNI and (ii) the ALPN offer.

That splits the problem cleanly:

* The **ProtoHttp** stack (DirtySDK, `EAWebKit.dll`) validates the chain
  happily — the Windows-store path works, our CRL is served, the leaf is
  accepted.
* The **h2** stack lives in `FIFA22.exe` and has its **own** validation path.
  It never consults the Windows store; it carries its own anchor table
  (§4). It rejects our chain with `certificate_unknown`.

So "the certificate is bad" is the wrong diagnosis, and "install the CA in the
Windows root store" does not fix the h2 leg.

---

## 3. Blocker A — the redirector reply is an invention

### 3.1 What we send vs. what the client parses

The client POSTs XML to `/redirector/getServerInstance` and parses the reply
as a DOM. Its TDF struct is `Blaze::Redirector::ServerInstanceInfo`, whose
fields — recovered from the `FIFA22.exe` string pool — are:

```
defaultDnsAddress, defaultServiceId, serviceNames, servers, instanceType,
inService, maxConnections, currentConnections, clientTypes,
compatibleClientVersions, incompatibleClientVersions, buildTarget,
buildLocation, buildTime, configVersion, depotLocation,
currentWorkingDirectory, nameRemaps, addressRemaps, auxMasters, auxSlaves,
masterInstance, endpoints, protocol, encoder, decoder, bindType,
connectionTypeString, sdev, trialServiceName, certificateList,
dirtyCertVersion, x509EntryMap, warnDate, downDate, warnMessage, downMessage,
publicKeyBits, firstPartyId, xboxId, timeZoneOffset, connectionProfile,
pS3Ticket, sandboxId, scert, stest
```

with `::ServerInstance`, `::ServerAddressInfo`
{ srcIp, srcPort, dstPort, netMask, **hostname**, serviceId, xboxServerAddress },
`::ServerEndpointInfo`, `::IpAddress`, `::InstanceType`, `::ServerAddress`,
`::XboxServerAddress`, `::NameRemapEntry`, `::AddressRemapEntry`,
`::ServerInfoData`, `::XboxId`.

**The XML convention is the key, and it is not obvious:** the client serialises
and deserialises this struct as XML in which *every element name is the
lowercased TDF field name* — root = lowercased struct name
(`serverinstanceinfo`), members = lowercased member names
(`servicenames`, `servers`, `addresses`, `instancetype`, `inservice`,
`hostname`, `srcport`, `dstport`, `netmask`, `serviceid`, `srcip`).

### 3.2 What our code sends

`crates/emulator-server/src/redirector_server.rs` emits:

```xml
<serverinstanceinfo>
  <address member="0"><valu>…</valu></address>   <!-- legacy in-Blaze shape -->
  <secure/>  <msiz/>  <trialService/>  <defaultDnsAddress/>
  <type/>  <servicename/>  <servicenames/>
  <instance>…</instance>
</serverinstanceinfo>
```

with `<servicename>qosapi</servicename>`, `<hostname>…</hostname>`,
`<ip>2130706433</ip>`, `<port>443</port>`, `<serviceid>2</serviceid>`.

**These names do not match the schema above.** The client cannot find the
`qosapi` service entry, so it never learns a QoS host — and falls back to
`getaddrinfo("")` (empty host ⇒ enumerate local interfaces), which selects the
machine's Radmin VPN address `26.178.140.32` and dials
`[::ffff:26.178.140.32]:443` with a zero-length SNI.

### 3.3 The client-side path, as traced

```
FIFA22.exe 0x141FEA4E0  →  parsed redirector DOM
FIFA22.exe 0x140c90150  →  the "bind" query that resolves the service name
FIFA22.exe doc->[0x10]  →  the service list
```

The remaining unknown is the **exact DOM path and nesting** for the `qosapi`
entry. That is the next thing to recover from the binary.

### 3.4 PreAuth cannot supply it

Blaze 18's `preAuth` carries the QoS coordinator as a **host name plus port**
(`QCNF { DPSP: 4001, QCA: "qoscoordinator.gameservices.ea.com", QCP: 10010,
QPR: "dingo-1-common" }`). **FIFA 22 is Blaze 15** and its `preAuth` has **no
`QCNF` at all**; `BWPS.PSA = ""` and `BWPS.PSP = 0` (proved against the
authentic EA fixture). So for FIFA 22 the QoS endpoint can only come from the
HTTP redirector reply or from a compiled-in default — which is why the
redirector reply is *the* root cause and not merely a contributing factor.

---

## 4. Blocker C — FIFA 22's embedded trust anchors

`FIFA22.exe` (592 MB, image base `0x140000000`) carries its own RSA
trust-anchor table in `.xtls`. Each entry is a 0x60-byte struct:

```
{ char name[0x40]; u32 f40; u32 f44; u32 f48; u32 f4C(=0x100);
  u8* modulus(+0x50); u32 exp_len(+0x58=3); u32 exp(+0x5C=0x10001); }
```

Four anchors are present:

| Anchor | File offset | VA |
| :--- | :--- | :--- |
| GOS 2015 CA | `0x835B360` | `0x14835D560` |
| VeriSign G5 | `0x835B770` | `0x14835D970` |
| DigiCert Global Root CA | `0x835B978` | `0x14835DB78` (modulus at `0x146DFEAA0`) |
| (a fourth, not yet identified) | — | — |

**The shim patches exactly one of them.** `shim/src/cert_bypass.cpp` scans
every loaded module for the literal `"DigiCert Global Root CA"` and
overwrites the `+0x50` modulus pointer — a good, general mechanism, applied
to a single target name. The other three anchors are untouched, and the
h2 client's certificate chain will not validate against a CA that is not in
its table.

> **This is a certificate-validation bypass on `FIFA22.exe` / `EAWebKit.dll`,
> which is already explicitly authorised by the user.** Extending it from one
> anchor to all four is the same class of action, but it touches *additional
> trust anchors in the client binary*; it should be named explicitly in the
> next authorisation request rather than assumed.

---

## 5. Blocker B — HTTP/2

### 5.1 What the client does

The QoS connection negotiates **ALPN `h2`**. The `:443` sink currently
answers HTTP/1.1 only and explicitly warns on an h2 preface:

```rust
// crates/emulator-server/src/telemetry_server.rs
if is_http2_preface(&buffer[..n]) {
    tracing::warn!("[Telemetry:443] HTTP/2 connection preface from {} … the sink is HTTP/1.1 only; …");
}
```

### 5.2 ALPN mechanics in rustls (verified against the vendored source)

`third_party/rustls-0.23.46/src/server/hs.rs:90-120`:

* ALPN is enforced **only when `config.alpn_protocols` is non-empty**.
  With an empty list, the client's offer is silently declined — no alert, no
  error. The server simply speaks nothing back.
* If the server *does* list protocols and there is no overlap, rustls sends
  `AlertDescription::NoApplicationProtocol` and kills the handshake.

This is why the current build fails the h2 handshake *quietly at the ALPN
layer* and *loudly at the certificate layer* — and why adding `h2` to the
server's list is safe for the ProtoHttp traffic: those clients offer **no**
ALPN at all (`ALPN=[]` in the capture above), so rustls will not select
anything for them.

Reading the negotiated protocol back out:

```rust
// tokio-rustls-0.26.6/src/server.rs:377
pub fn get_ref(&self) -> (&IO, &ServerConnection)
// rustls CommonState
pub fn alpn_protocol(&self) -> Option<&[u8]>   // common_state.rs:148
```

### 5.3 The `h2` 0.4 API (verified against the vendored source)

`h2` 0.4 is a **frame** library with **no `http-body` dependency**:

```rust
let mut conn = h2::server::handshake(io).await?;                    // Connection<T, Bytes>
while let Some(req) = conn.accept().await {                          // Option<Result<(Request<RecvStream>, SendResponse<B>), Error>>
    let (request, mut respond) = req?;
    // request body:
    while let Some(chunk) = body.data().await { … body.flow_control().release_capacity(len); }
    // response: HEADERS ONLY
    let mut send = respond.send_response(Response::<()>::builder()…build(), false)?;
    send.send_data(Bytes::from(body), true)?;                        // DATA frame
}
```

The first draft of `http2_server.rs` used `send_response(response, true)` with
a body-carrying `http::Response<Bytes>`; that was wrong, and the module was
rewritten against the real API.

### 5.4 What was built

`crates/emulator-server/src/http2_server.rs` (new, compiles, **not yet wired
in**):

* `serve_http2(stream, peer)` — h2 server loop; every request is handed to the
  *same* `route_http_request` used by the HTTP/1.1 path, so the two transports
  cannot drift apart.
* `is_http2_alpn(Option<&[u8]>)` and the `ALPN_H2` / `ALPN_HTTP11` constants.
* RFC 9113 §8.2.2 filtering of `connection` / `transfer-encoding` /
  `keep-alive` / `upgrade` — sending those over HTTP/2 is a protocol error.
* Flow-control capacity is released as the body is consumed, so the client
  never stalls waiting for a `WINDOW_UPDATE`.
* Two unit tests: the ALPN name is recognised; a QoS reply fits in one DATA
  frame (inside the initial 64 KiB window).

### 5.5 What is still missing (three small edits)

1. `crates/emulator-server/src/tls.rs:439-441` — add
   `.with_alpn_protocols(vec![b"h2".to_vec(), b"http/1.1".to_vec()])` to the
   `ServerConfig`.
2. `crates/emulator-server/src/telemetry_server.rs` — replace the "HTTP/1.1
   only" warning with an ALPN dispatch to `serve_http2`.
3. `crates/emulator-server/src/lib.rs` — `pub mod http2_server;`
   *(done this session; the module now compiles and is type-checked).*

> ⚠️ **Note for whoever wires this up:** `start_services_full_with_qos` shares
> **one** `TlsAcceptor` across the redirector, Blaze and telemetry listeners.
> Adding ALPN to that shared config also puts `h2`/`http/1.1` on the Blaze
> port. If the Blaze/ProtoSSL client ever offers an ALPN protocol that is
> *not* in our list, rustls will answer `NoApplicationProtocol` and break the
> Blaze handshake — which would be a much worse regression than the one being
> fixed. **Either confirm from the live capture that the Blaze client offers
> no ALPN (the log's ProtoHttp entries suggest the DirtySDK stack never
> does), or give the `:443` sink its own acceptor.**

---

## 6. The reference repositories are exhausted for this problem

A sweep of all five pinned reference projects was run. Result: **the HTTP
redirector reply format is not present in any of them.**

| Repository | What it actually contains | Relevance |
| :--- | :--- | :--- |
| `mxmstr/hp-server` | A **fork** of BlazeSDK plus a game server. Has **no redirector handler and no captured redirector reply frame**. Its ":42127 redirector" is just an unauthenticated Blaze session on a separate port. | **Not independent corroboration** — it is the upstream. |
| `Khysnik/BlazeSDK` | **Codec only**: 5 Go files (Fire2 framing + TDF codec, 922 lines) based on BlazeSDK v15.1.1.9.4. No TDF schema, no struct definitions, no redirector component, no QoS, no HTTP service. | Fire2/Heat2 codec corroboration only. |
| `openBlase/openBlase` | `Tdf.h` label compression; contains `BlazeProxy/gosredirector.ea.com.crt` — the **retail** redirector leaf, issued by an EA root called **"Root Agency"**, *not* DigiCert. | **The retail leaf's issuer is an EA root.** Our CA's "DigiCert Global Root CA" name is cosmetic and tells us nothing about the client's real anchors. |
| `lowlevelmetal/open-ds2-server` | A **different** Blaze generation (12-byte header, LEB128 varint). | Background only. |
| `skate6743/Skate3BlazeServer` | TDF schema model classes; `Util` command models. Its redirector work is the older **in-Blaze TDF** redirector (component `0x0005`, command `0x0001`) — a *different service* from FIFA 22's HTTP redirector on `:42230`. | Schema source of truth; **not** the reply format. |

**`qosapi` has zero occurrences in all five repositories.**

Two further consequences:

* The HP generation's transport is **TLS 1.0 with RSA/RC4** (hp-server
  ships a custom adapter for suites `0x0004`/`0x0005` because Go's
  `crypto/tls` cannot offer them), whereas FIFA 22's QoS connection is
  **modern TLS with an `h2` ALPN offer**. The generations differ in transport
  as well as in reply format, so no reference behaviour can be copied
  literally.
* The only authentic EA bytes in the reference set remain the two `preAuth`
  frames in `hp-server/test/test.go` (Blaze 18.3.0) — which are the source of
  the `QOSS.QCNF` / `QCA="qoscoordinator.gameservices.ea.com"` / `QCP=10010`
  shape quoted in §3.4.

**Conclusion: the reply format must be recovered from `FIFA22.exe` itself** —
the parser that consumes the `:42230` response, and the code that binds the
`qosapi` service.

---

## 7. The shim destroys the host name before TLS

`shim/src/hooks_winsock.cpp` — `ShouldRerouteHost` rewrites any host matching
`.ea.com` / `.ea.net` / `.dice.se` / `.xboxlive.com` to the emulator's IP
**before** the TLS ClientHello is built. So even a perfect redirector reply
yields a **zero-length SNI**: the name is gone by the time the handshake starts.

Two viable ways to keep the name alive to the SNI:

1. **Hosts-file pattern** (what `hp-server` and `Skate3BlazeServer` do): put
   the QoS host name in `C:\Windows\System32\drivers\etc\hosts` pointing at
   `127.0.0.1`, and make the shim pass that name through unmodified. The name
   then reaches the ClientHello, the SNI is correct, and the leaf's SAN
   matches.
2. **SNI-preserving shim rewrite**: keep the original name in the
   `getaddrinfo` result so the client sends it as the SNI, while the connect
   still goes to `127.0.0.1`.

Note that `getaddrinfo("")` deliberately passes through (it enumerates local
interfaces) — that behaviour is *correct* and is what exposed the bare-IP
dial, so it must not be "fixed" by rewriting empty hosts.

---

## 8. Documentation that is now stale

These need correcting; they were not corrected in this session because the
pause landed first.

| File | Section | What is wrong |
| :--- | :--- | :--- |
| `docs/POST_PING_STALL_ROOT_CAUSE.md` | §1, §7 | Claims the redirector "returns 200 OK with a **valid XML ServerInstance**" and that "the redirector HTTP flow works". The reply is an **invention**; the client honours the Blaze address and **ignores the service list**. |
| `docs/FIFA22_QOS_PROTOPING_HTTP.md` | §3.3 | The `qosapi` **port has not been located**; the literal is at best a Host-header token. |
| `docs/FIFA22_QOS_PROTOPING_HTTP.md` | §8.4 | The sample log line `host=qosapi:443` is a **guess**. |
| `docs/FIFA22_QOS_PROTOPING_HTTP.md` | §8.3 | Needs the HTTP/2 finding appended: the sink is HTTP/1.1-only, and the client negotiates `h2`. |
| `PROJECT_PROGRESS.md` | §10, "Open Item — the QoS HTTP port" | Still says the port "has not been located in the binary yet" and that the shim's `ShouldRerouteHost` "matches none of its patterns for the literal host `qosapi`". The real answer is that the client never resolves `qosapi` **at all** because the redirector reply does not carry it. |

---

## 9. Repository changes made this session

| Path | Change |
| :--- | :--- |
| `crates/emulator-server/src/http2_server.rs` | **New.** HTTP/2 server for the QoS API, written against the real `h2` 0.4 API. Compiles; 2 unit tests. Not yet dispatched to. |
| `crates/emulator-server/src/lib.rs` | `pub mod http2_server;` |
| `crates/emulator-server/Cargo.toml` | `h2 = "0.4"`, `http = "1"`, `bytes = "1"`. (`http-body-util` was added and then removed — h2 0.4 needs no body trait.) |
| `tools/align_find.py`, `tools/anchor_xrefs.py`, `tools/dump_anchor.py`, `tools/dump_qwords.py`, `tools/dump_strings.py`, `tools/dump_tdf.py`, `tools/dump_vtable.py`, `tools/fast_xrefs.py` | New RE tooling, committed. |
| `third_party/rustls-0.23.46/` | The vendored rustls fork the workspace's `[patch.crates-io]` points at. **Must be committed** or the workspace will not build for anyone else. |
| `crates/emulator-server/src/crl_server.rs`, `crates/emulator-server/tests/crl_revocation.rs`, `crates/services-http-content/src/protohttp.rs`, `crates/services-blaze/tests/auth_command_ids.rs` | New work from earlier in the day: CRL generation/serving, the ProtoHttp user-agent parser, the `logout` command-ID test. |
| `docs/FIFA22_QOS_PROTOPING_HTTP.md`, `docs/BLAZE_FIRE2_HEAT2_PROTOCOL.md` | Updated earlier in the day. |

**Deleted as scratch:** `scratch_schema_reg.txt`, `scratch_strrefs.txt`,
`scratch_svc.txt`, `tmp_preauth_frame.bin`,
`crates/services-blaze/tests/tmp_conf_dump.rs`.

---

## 10. Next session — priority order

1. **Recover the redirector reply shape from `FIFA22.exe`.** Complete the DOM
   path under `0x141FEA4E0` → bind query `0x140c90150` → `doc->[0x10]`, then
   rewrite `render_redirector_xml` / `render_service_instance_xml` to emit
   the **lowercased TDF field names** (`servicenames`, `servers`, `addresses`,
   `instancetype`, `inservice`, `hostname`, `srcport`, `dstport`, `netmask`,
   `serviceid`, `srcip`) with the right nesting. The existing tests that
   assert `<servicename>` / `<ip>` / `<port>` must be updated to the real
   names. **This is blocker A and the highest-value single change.**
2. **Make the QoS endpoint a *name* that survives to the SNI** (hosts file or
   SNI-preserving shim rewrite — §7). A redirector reply that advertises an
   IP literal will produce the same zero-length SNI.
3. **Wire the HTTP/2 path** (§5.5): the `with_alpn_protocols` call, the ALPN
   dispatch in `telemetry_server.rs`, and the separate-acceptor question.
4. **Patch all four embedded trust anchors** in `FIFA22.exe` (and
   `EAWebKit.dll`), not just DigiCert — with explicit authorisation naming
   the additional anchors (§4).
5. **Live verification** with `tools/clean_deploy_and_play.ps1` (already
   authorised). The success signal is: the h2 handshake completes, a
   `/qos/qos` request appears in the log, and `Util::postAuth` is served.
6. Then PHASE 2 (matchmaking / GameManager), PHASE 3 (UDP relay, < 5 ms
   ping), PHASE 4 (two-client end-to-end).

---

## 11. Standing constraints and authorisations

**Constraints (unchanged):**

* Do not modify `vendor/` or third-party binaries.
* Do not contact real EA endpoints.
* If a change requires bypassing Denuvo/EAAC, stop and report.
* All tests must pass (`cargo test --workspace`).
* Use at most **2** sub-agents at a time.
* The private key in `f22_server.key` must not be written into a persistent
  file without explicit authorisation naming that secret and destination.

**Authorisations granted earlier (still in force):**

* Installing the emulator CA into `Cert:\CurrentUser\Root` (user scope) —
  `CN=DigiCert Global Root CA, OU=www.digicert.com, O=DigiCert Inc, C=US`.
  Revoke with
  `certutil -user -delstore Root "DigiCert Global Root CA"`.
* The shim's in-process trust-anchor patch against `FIFA22.exe` and
  `EAWebKit.dll` — **for the anchors it already covers**. Extending it to the
  other three anchors should be re-authorised explicitly (§4).
* Running `tools/clean_deploy_and_play.ps1` for live verification.

**No new authorisations were granted in this session.**
