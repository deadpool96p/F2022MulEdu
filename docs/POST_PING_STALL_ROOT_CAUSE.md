# Root-Cause Analysis: Post-`Util::ping` 10-Second Stall

**Document Version:** 1.0.0
**Last Updated:** October 21, 2026
**Status:** Root cause **identified**; fixes staged, not yet applied to the wire path
**Symptom:** The client accepts the `Util::ping` response, then stalls for exactly
10 seconds and reports *"There was an error accessing server data."*

---

## 1. The observed behaviour

From `emulator_server.log` (6 identical cycles across repeated client launches):

```
12:29:38.179416Z [DualListener] Detected Plain TCP stream from 127.0.0.1:53928
                 (peeked 8 bytes: [00, 00, 00, 00, 00, 00, 00, 09])
12:29:38.179686Z [Blaze Server] Wire data received (16 bytes):
0000  00 00 00 00 00 00 00 09  00 02 00 00 00 00 00 00  |................|
12:29:38.179869Z [Blaze Server] Parsed 16-byte frame: Comp=0x0009, Cmd=0x0002, Seq=0, Len=0, MsgType=SubmitRequest
12:29:38.180043Z [Blaze Util] Handled ping (Seq=0) -> PingResponse(STIM=1792499378)
12:29:38.180218Z [Blaze Server] Sending response for Comp=0x0009, Cmd=0x0002, Seq=0 (26 bytes):
0000  00 00 00 0A 00 00 00 09  00 02 01 00 00 00 00 00  |................|
0010  CF 4A 6D 00 B2 BD DD D6  06 00                    |.Jm.......|
12:29:38.180404Z [Blaze Server] Flushed response to client socket successfully
12:29:48.201804Z [Blaze Server] Client UID 0 closed connection (EOF)   <-- exactly 10.02s later
```

Key observations:

* The redirector HTTP flow **works** — 200 OK, client redirected to `127.0.0.1:42128`.
* The client connects over plain TCP and sends a well-formed `Util::ping`.
* Our reply is written and flushed successfully. The socket write does not fail.
* The client then goes silent for **10.02 seconds** and closes the connection.
* Our own keep-alive timeout is 45 s, so the 10 s EOF is a **client-side watchdog**.

---

## 2. Hypotheses considered and their disposition

| # | Hypothesis | Verdict | Evidence |
| :--- | :--- | :--- | :--- |
| H1 | `PingResponse` is missing `PING`/`LATN`/`TIME` tags | **FALSE** | EA's own registration routine for `PingResponse` in the retail binary sets a member count of **1**. `STIM` alone is correct. |
| H2 | The 16-byte header bit-shift was the cause (byte 10 = `msgType << 4`) | **FALSE** | Byte 10 is the message number's high octet under Fire2. The "fix" in commit `2eaad18` was based on a misreading of the header layout. |
| H3 | The client is missing QoS/telemetry HTTP probes (`/qos/qos`, `/qos/firetype`, `/getPeerAddress`) | **FALSE for this stall** | The client makes **none** of these requests during the failing run. QoS/STUN is a Phase-3 concern, not the cause of *this* stall. |
| H4 | The `STIM` integer is encoded with the wrong varint scheme | **CONFIRMED** | See §3. |
| H5 | The 16-byte header layout is not Fire2 | **CONFIRMED** | See §4. |

---

## 3. Defect A — Heat2 varsize integer encoding

**Root cause:** `crates/blaze-tdf/src/varint.rs` implemented plain LSB-first 7-bit
LEB128. Heat2 uses a **6-bit first byte** with a sign flag in bit 6 and
continuation bytes at shifts 6, 13, 20, 27…

The two schemes disagree on **every value ≥ 0x40**, and Heat2 decodes LEB128 bytes
into a *different number* rather than failing.

### The concrete damage

`STIM` carries a Unix epoch second. For the captured run, `1792499378`
(2026-10-20 12:29:38Z):

| Encoding | Bytes | EA decodes |
| :--- | :--- | :--- |
| Heat2 (correct) | `B2 FA BA AD 0D` | `1792499378` → 2026-10-20 ✔ |
| LEB128 (shipped) | `B2 BD DD D6 06` | `896249714` → **1998-05-27** ✘ |

Our shipped ping payload was:

```
CF 4A 6D 00 B2 BD DD D6 06 00
```

The correct payload is:

```
CF 4A 6D 00 B2 FA BA AD 0D 00
```

The client receives a server timestamp **28 years in the past**, fails its
clock-drift / latency check, and the transaction never completes.

### Evidence

Three independent sources agree on the Heat2 scheme:

1. `mxmstr/hp-server` → `EncodeVarsizeInteger`
2. `Khysnik/BlazeSDK` → `DecodeVarsizeInteger`
3. `openBlase` → commented-out `Tdf::CompressInteger`

And the scheme is validated end-to-end against **authentic EA bytes**: the
475-byte offline-game-report payload in `hp-server`'s test fixtures decodes with
**475 of 475 bytes consumed**.

### Status

**Fixed.** `crates/blaze-tdf/src/varint.rs` now implements the Heat2 varsize
scheme. Pinned by `crates/blaze-tdf/tests/tdf_fire2_wire.rs` (21 exact vectors +
agreement across `-100_000..=100_000`).

---

## 4. Defect B — the 16-byte Fire2 header layout

**Root cause:** `crates/blaze-core/src/frame.rs`'s `to_bytes_16` / `from_bytes_16`
used a non-Fire2 layout. The shipped writer produced:

```
00 00 00 0A 00 00 00 09 00 02 01 00 00 00 00 00
```

Decoded under Fire2 that is:

| Field | Decoded | Client expected |
| :--- | :--- | :--- |
| `messageNumber` | `65536` | `0` |
| `messageType` | `0` (`MESSAGE`) | `1` (`REPLY`) |

**Both fields are wrong.** The client cannot match the reply to the request, and
receives a `MESSAGE` where it expects a `REPLY`.

Additionally, bytes 4–6 were being treated as an `errorCode` when Fire2 uses them
for `metadataSize`, and the Fire2 `userIndex` / `options` / `reserved` fields had
no representation at all.

### The decisive proof

The client's own request bytes are unambiguous:

```
00 00 00 00 00 00 00 09  00 02 00 00 00 00 00 00
```

* As **Fire2**: `componentID = 0x0009` (Util), `commandID = 0x0002` (ping),
  `messageNumber = 0`, `messageType = 0` (MESSAGE), `userIndex = 0`. All correct.
* As the **12-byte legacy layout**: `component = 0x0000`, `command = 0x0009` —
  not a valid Blaze component/command pair.

### The real Fire2 layout

```
 0        1        2        3        4        5        6        7
 +--------+--------+--------+--------+--------+--------+--------+--------+
 |                     payload size (u32 BE)         |  metadata size  |
 +--------+--------+--------+--------+--------+--------+--------+--------+
 |      component ID (u16 BE)       |       command ID (u16 BE)      |
 +--------+--------+--------+--------+--------+--------+--------+--------+
 |                            message number (u24 BE)              | |
 +--------+--------+--------+--------+--------+--------+--------+--------+
 |  msg type (<<5) | user index (&0x1f)   |  options  |  reserved  |
 +------------------+-----------------------+-----------+------------+
```

See `docs/BLAZE_FIRE2_HEAT2_PROTOCOL.md` for the full specification.

### Status

**Not yet applied.** The Fire2 header rewrite is deliberately separated from the
varint fix so that each change can be verified in isolation, per the
systematic-debugging workflow. The 12-byte legacy path (`to_bytes` / `from_bytes`)
must stay untouched — the redirector, the telemetry sink and all eight integration
test files depend on it.

---

## 5. Defects C & D — Heat2 type IDs and composite wire forms

Found while validating the Heat2 model against authentic EA bytes. Both are
confirmed by the EA captures and by the retail binary's own type tables.

| Defect | Shipped | Correct | Impact |
| :--- | :--- | :--- | :--- |
| **C** — `OBJECT_TYPE` / `OBJECT_ID` swapped | `ObjectId = 8`, `ObjectType = 9` | `ObjectType = 8`, `ObjectId = 9` | Object references decode as the wrong type |
| **C** — `FLOAT` / `TIMEVALUE` swapped | `TimeValue = 0x0A`, `Float = 0x0B` | `Float = 10`, `TimeValue = 11` | Floats decode as integers and vice versa |
| **D** — Union missing its member tag | `[index][type][value]` | `[index][tag(3)][type][value]` | 3-byte desync on every union-bearing payload |
| **D** — Variable wire form wrong | plain varint | `[0x01][tdfId][tag(3)][type][value][0x00]` | Parser desync on any variable |
| **D** — Object components as raw u16 BE | `u16 + u16` | `varsize + varsize` | Wrong byte count for every object id/type |

Authentic EA evidence for D:

* Union: `(active = 2, member = 'ULAV', type = TDF)`
* Variable: `(tdfId = 1085761969, member = 'EMAG', type = TDF)`

### Blast radius

`TdfType` variants are referenced **only inside `crates/blaze-tdf`** (verified by
grep). The `Union` variant is used in production code at four sites
(`services-redirector`, `services-blaze`), all of which construct
`TdfUnion::with_value(0x00, TdfValue::Struct(...))` — these will need their
active index and member tag updated to match the real schema, since the current
`0x00` index with a `HOST`/`IP`/`PORT` payload does not correspond to any real
`NetworkAddress` case (the real cases are `2 = IpPairAddress`, `3 = IpAddress`).

### Status

**Not yet applied.**

---

## 6. Header-detection heuristic

Both servers used this to decide between 12-byte and 16-byte framing:

```rust
buffer[0] == 0 && buffer[1] == 0 && buffer[4] == 0 && buffer[10] <= 0x03
```

The `buffer[10] <= 0x03` clause is **meaningless under Fire2** — byte 10 is the
high octet of the message number, not a message type. It happens to work for the
six captured frames (all have byte 10 = 0) but will misclassify any request with
message number ≥ 65536, and it is wrong in principle.

**Status:** to be replaced with a Fire2-valid check (payload/metadata size sanity
plus a valid `messageType` in the high 3 bits of byte 13).

---

## 7. What is *not* the cause

Recording these explicitly so the next investigation does not re-tread them:

* **The redirector HTTP flow.** It returns 200 OK with a valid XML `ServerInstance`
  and the client does connect to the advertised `127.0.0.1:42128`.
* **TLS.** The client negotiated plain TCP on this path successfully.
* **Missing QoS/telemetry HTTP probes.** The client never issued them during the
  failing run.
* **`PingResponse` member count.** EA registers exactly one member (`STIM`).
* **The 45-second server keep-alive.** The 10 s EOF is client-driven.
* **`trace_a.jsonl` / `trace_b.jsonl`.** These are generated by *our own* test
  harness (`HostMatchReadyTester`) and decode as 12-byte frames. They are **not**
  admissible evidence of client traffic.
* **`open-ds2-server` and `Skate3BlazeServer`.** Both implement the 12-byte
  legacy FIRE generation with a 7-bit varint — a different Blaze generation, not
  applicable to FIFA 22. Their TDF model *classes* remain useful for schema
  names; their *wire format* is not.

---

## 8. Next steps

| # | Step | State |
| :--- | :--- | :--- |
| 1 | Implement Heat2 varsize in `blaze-tdf` | **Done** |
| 2 | Add `tdf_fire2_wire.rs` conformance tests | **Done** |
| 3 | Correct Heat2 type IDs in `types.rs` | Pending |
| 4 | Correct union / variable / object wire forms in `writer.rs` + `reader.rs` | Pending |
| 5 | Rewrite `to_bytes_16` / `from_bytes_16` to Fire2 in `blaze-core` | Pending |
| 6 | Replace the 12-vs-16 detection heuristic in both servers | Pending |
| 7 | Update `services-blaze/src/util.rs` unit tests for the new layout | Pending |
| 8 | Re-run the real client; confirm `Util::preAuth` (0x0009/0x0007) arrives | Pending |
| 9 | Phase 2 — matchmaking & GameManager | Pending |
| 10 | Phase 3 — UDP relay & low ping | Pending |
| 11 | Phase 4 — two-client end-to-end verification | Pending |

Each step is a separate commit, and `cargo test --workspace` must stay green
(including the eight 12-byte integration test files) throughout.
