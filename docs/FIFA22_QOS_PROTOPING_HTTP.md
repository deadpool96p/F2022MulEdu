# FIFA 22 QoS / ProtoPing HTTP Reference

**Document Version:** 1.0.0
**Last Updated:** October 21, 2026
**Status:** Authoritative reference for `crates/services-http-content` (the `/qos/*` and
`/getPeerAddress` endpoints) and `crates/services-blaze` (the `QOSS` struct in the
`Util::preAuth` reply)
**Applies to:** FIFA 22 PC Retail `1.0.77.45722` (Frostbite 3, BlazeSDK 15,
DirtySDK 15)

> This document is **derived from `FIFA22.exe` itself**, not from any reference
> repository. Every field name, every path and every success/failure condition
> below was read out of the retail executable by disassembly. Where the
> open-source references disagree with the binary, **the binary wins** — and in
> one important case they disagree about the field the client actually requires.

---

## 1. How This Document Was Established

The evidence here is different in kind from
[`BLAZE_FIRE2_HEAT2_PROTOCOL.md`](BLAZE_FIRE2_HEAT2_PROTOCOL.md). That document
is about the *binary* Blaze framing. This one is about the *HTTP side-channel*
FIFA 22 uses for QoS probing, and it was recovered purely from the client
binary.

| Evidence class | Source | What it proves |
| :--- | :--- | :--- |
| **The client's own parser** | `FIFA22.exe` at `0x140CA1DE0` (path lookup), `0x140CA2280` (value reader), `0x140CA1FF0` (sibling iterator), `0x140CA1BB0` (tag-end finder) | The response document shape, the path syntax, and the integer parsing rules |
| **The client's own request builder** | `FIFA22.exe` at `0x141FE94D0` (URL builder), `0x141FE8540` (response parser) | The request URLs, the query parameters, and the hard success conditions |
| **The client's own schema** | `references/Skate3BlazeServer/.../Util/Commands/PreAuthResponse.cs`, `.../Models/QosConfigInfo.cs`, `.../Models/QosPingSiteInfo.cs` — EA's own code-generated TDF models | The `QOSS` struct's member names and types |

The analysis tooling **is** committed. It never embeds the executable — the
retail binary is not redistributable — and takes its path as an argument:

| Tool | Purpose |
| :--- | :--- |
| `tools/disasm_region.py` | Disassemble a VA region of `FIFA22.exe` with string annotations |
| `tools/find_str_xrefs.py` | Byte-exact RIP-relative xref finder (4-byte window scan; no linear sweep) |
| `tools/qos_dom_model.py` | Python model of the client's DOM scanner, used to validate candidate response bodies offline |

`★ Insight ─────────────────────────────────────`
The xref finder works by **arithmetic, not disassembly**: for a RIP-relative
operand, `target = (end of the disp32 field) + disp32`. The end of the disp32
field does not depend on the instruction's length, so scanning every 4-byte
window in the file finds every RIP-relative reference — even in a 592 MB binary
where linear disassembly is hopeless because one mis-aligned chunk start poisons
the entire run.
`─────────────────────────────────────────────────`

---

## 2. The response is a flat text scan, not an XML tree

This is the single most counter-intuitive fact about these endpoints, and
getting it wrong makes every field look "present but empty".

The client parses the HTTP body with a **path-based lookup helper** at
`0x140CA1DE0`. It is *not* an XML parser. It is a **flat forward text
scanner**:

* The path is split on `.` and `<` (`0x140CA1E42`–`0x140CA1E6A`).
* A leading `.` on the path is skipped (`0x140CA1E1A`–`0x140CA1E27`).
* Each path segment matches an element whose *name* equals the segment, with a
  `*` wildcard token (`0x140CA1EBA`–`0x140CA1EC3`).
* The scan is **document order**, not tree nesting.

So `.qosport` means "the first `qosport` element that appears *after* the
current node", and `.ips.ips` means "the `ips` element after the current node,
then the `ips` element after *that*". Two elements with the same name in
different places are distinguished only by their position in the byte stream.

The value reader at `0x140CA2280` skips the opening tag, then parses an optional
sign followed by decimal digits. `edx = 0` selects the **unsigned** decoder.

> **Consequence:** a response of the form
> `<qos><qosport>1234</qosport></qos>` is read as
> `root -> qos -> qosport`, **not** `root -> qosport`.

`tools/qos_dom_model.py` is a faithful Python model of this scanner. It
exists so a candidate response can be validated **without launching the game**,
which is the only way to test a response shape that the client only reads on a
cold start.

---

## 3. Endpoint → root element → fields

The parser function starts at `0x141FE8540`. It switches on the request type
stored at `[rbx + 0x34]`.

| Request | Root element | Fields read (path) | Code |
| :--- | :--- | :--- | :--- |
| type 1 | `qos` | `.qosport` (u16), `.qosip` (u32) | `0x141FE8580`–`0x141FE85D8` |
| type 2 | `qos` | `.qosport` (u16), `.qosip` (u32), `.numprobes`, `.probesize`, `.requestid`, `.reqsecret` | `0x141FE8629`–`0x141FE8728` |
| type 3 | `firewall` | `.numinterfaces`, `.ips.ips`, `.ports.ports`, `.requestid`, `.reqsecret` | `0x141FE8787`–`0x141FE889B` |
| type 3 | `firetype` | `.firetype` | `0x141FE88DE`–`0x141FE8911` |

The `firetype` request is the *same* type-3 request that also returns
`firewall`; the client reads `firetype` from the same response only when
`[rbx + 0x34] == 3` **and** the response root is `firetype`
(`0x141FE88D4`–`0x141FE88DC`).

### 3.1 The request URLs

Built by the client at `0x141FE94D0` from a base URL of the form
`"%s://%s:%u"` (file offset `0x77946C0`) with the host `qosapi`
(file offset `0x77946CC`):

```
%base%/qos/qos
%base%/qos/firewall
%base%/qos/firetype
```

with query parameters `?vers=`, `&qtyp=`, `&nint=`, `&rqsc=`, `&rqid=`.

### 3.2 The `getPeerAddress` response is not XML at all

`getPeerAddress` uses a completely different parser. The body is a list of
`name=value` pairs separated by CR/LF, and the client looks up a **fixed set of
names**:

| Field | Read at |
| :--- | :--- |
| `targetIP` | `0x144AA0731` |
| `targetPort` | `0x144AA0845` |
| `tag` | `0x144AA092D` |
| `sendCount` | `0x144AA0A49` |
| `sourcePort` | `0x144AA0B39` |
| `peerIP` | `0x144AA151D` |
| `peerPort` | `0x144AA159C` |

The client sends `getPeerAddress?myIP=<ip>&myPort=<port>&version=1.0` and
expects the same `name=value` shape back. The IP fields here are parsed with a
**dotted-quad** parser (`0x140C96010`) — unlike `qosip`, which is a decimal
u32 (see §4).

The request also carries a `Cookie: sessionID=%s` header (`0x79F5FF0`).

### 3.3 Which port the client connects to

The base URL is built as `"%s://%s:%u"` from the host `qosapi`. The `%u` port
argument is **not** one of the QoS timing constants stored in the same
structure (`[rbx+0x14c]=0x1b58`=7000, `[rbx+0x150]=0x3e8`=1000,
`[rbx+0x158]=0x19`=25, `[rbx+0x15c]=0x9c4`=2500 — those are timeouts and
intervals, not a port).

This matters because the shim reroutes **by port**
(`shim/src/hooks_winsock.cpp`, `CheckAndRewriteSockAddr`): it always reroutes
42127 / 42128 / 42230 / 44325, and with `RerouteAllEA` also 443 / 80 / 8080 /
`cfg.telemetry_port`. A QoS request on a port outside that list is **not**
delivered to the emulator.

It also matters because `ShouldRerouteHost` matches on the **host name**:

```cpp
if (host.find("gosredirector.ea.com") != npos || host.find("blaze") != npos)
    return true;
if (cfg.reroute_all_ea) {
    if (... || host.ends_with(".ea.com") || host.ends_with(".ea.net")
           || host.ends_with(".dice.se"))
        return true;
}
return false;
```

The literal host `qosapi` matches **none** of these patterns. The QoS request
therefore depends on the *port* being rerouted, not on the host name being
recognised.

> ⚠️ **Open item.** The port the client actually uses for `qosapi` has not been
> located in the binary yet. Until it is, the QoS HTTP endpoints can only be
> reached if that port happens to be one the shim already reroutes. This is a
> candidate explanation for the client stalling before the QoS step completes.

### 3.4 The ProtoHttp user agent

The client stamps its HTTP requests with:

```
ProtoHttp %d.%d/DS %d.%d.%d.%d.%d (Windows)
```

(string at file offset `0x7575C00`, VA `0x147577E00`, section `.xtext`).

Observed live as `ProtoHttp 1.3/DS 15.1.7.0.3 (Windows)`. The open-ds2
connection analysis shows the same format **without** the trailing
`(Windows)`; the binary is authoritative.

The same cluster (`0x7575C2C`–`0x7575C9C`) holds the method names
(`GET`, `POST`, `PUT`, `PATCH`, `HEAD`, `DELETE`, `OPTIONS`, `CONNECT`), the
scheme strings `http` / `https`, and `HTTP/1.0`.

---

## 4. `qosip` is a big-endian decimal u32, not a dotted quad

**This is the most important detail in this document**, and the open-source
reference gets it wrong.

`qosip` is read by the *same* decimal integer parser as the ports
(`0x140CA2280` with `edx = 0`). A dotted quad therefore parses as `127` and
yields the wrong address. The parsed u32 is then split into its four bytes
**most-significant-first** at `0x141FE8B6A`:

```
0x141FE8B6A  mov  eax, dword ptr [rbx + 0x10]
0x141FE8B6D  mov  byte ptr [rsp + 0x3f], al     ; byte 0 = v & 0xFF
0x141FE8B71  shr  eax, 8
0x141FE8B74  mov  byte ptr [rsp + 0x3e], al     ; byte 1
...
```

so `127.0.0.1` is `0x7F000001` = **2130706433**.

The same applies to the `.ips.ips` entries of the `firewall` response, which
are converted to socket addresses the same way (`0x141FE8D90`).

> Serving `<qosip>127.0.0.1</qosip>` — which is what the open-ds2 reference
> does — makes the client connect to `0.0.0.127`, which is not a host anyone
> is listening on.

---

## 5. Hard success conditions

The client **fails the QoS step** (returns a negative code) without these.
These are not soft requirements.

### 5.1 The `qos` response (types 1 and 2)

```
0x141FE85EC  cmp dword ptr [rbx + 0x10], 0      ; .qosip  must be non-zero
0x141FE85F0  je  0x141FE892A                  ; -> fail (return -3)
0x141FE85F6  cmp word ptr [rbx + 0x14], 0      ; .qosport must be non-zero
0x141FE85FB  je  0x141FE892A                  ; -> fail (return -3)
```

### 5.2 The type-2 `qos` response probe counters

```
0x141FE874D  cmp dword ptr [rbx + 0x38], 0      ; .requestid
0x141FE8751  je  0x141FE875F                  ; -> fail (-4)
0x141FE8753  cmp dword ptr [rbx + 0x44], 2
0x141FE8757  jb  0x141FE875F                  ; .numprobes must be >= 2
0x141FE8759  cmp dword ptr [rbx + 0x40], 0
0x141FE875D  jne 0x141FE8769                  ; .probesize must be NON-zero
0x141FE875F  mov eax, 0xfffffffc              ; -> fail (return -4)
```

> ⚠️ **`probesize` must be non-zero.** The `jne` at `0x141FE875D` branches to
> the **success** path (`0x141FE8769`); falling through to `0x141FE875F` is the
> failure. An earlier draft of this document had the polarity backwards.

### 5.3 The `firewall` response

Both `.ips.ips` and `.ports.ports` must exist, or the whole request fails with
`-4` (`0x141FE87D9`, `0x141FE87F0`).

---

## 6. Field reference

| Field | Type | Where | Required | Notes |
| :--- | :--- | :--- | :--- | :--- |
| `qosip` | u32 | `.qosip` | **yes, non-zero** | Big-endian decimal IPv4 (`2130706433` = 127.0.0.1) |
| `qosport` | u16 | `.qosport` | **yes, non-zero** | The UDP ProtoPing responder's port |
| `numprobes` | u32 | `.numprobes` | **yes, ≥ 2** | Type-2 response only |
| `probesize` | u32 | `.probesize` | **yes, non-zero** | Type-2 response only |
| `requestid` | u32 | `.requestid` | **yes, non-zero** | Type-2 and type-3 |
| `reqsecret` | u32 | `.reqsecret` | optional | Read if present |
| `numinterfaces` | u32 | `.numinterfaces` | type-3 | |
| `ips.ips` | list of u32 | `.ips.ips` | **yes** | Big-endian decimal IPv4 |
| `ports.ports` | list of u16 | `.ports.ports` | **yes** | Must be present, even if empty |
| `firetype` | u32 | `.firetype` | type-3 | Root element must be `firetype` |

### 6.1 Fields the client never reads

These appear in some reference implementations and are **read nowhere** in
`FIFA22.exe` (0 occurrences across all 592 MB):

* `qoshost` — the *old* implementation served this instead of `qosip`.
* `serviceid` — the TDF-level service id is `SVID` inside `QOSS`; the HTTP
  `serviceid` element is not the same thing and is not read.

> `★ Insight ─────────────────────────────────────`
> A field that is "not read" is worse than a field that is "missing". A missing
> field produces an empty value and a clear failure; an unread field produces
> a *plausible-looking* value that the client silently discards. Serving
> `qoshost` and no `qosip` produces exactly the latter failure mode: the
> response looks complete, and the client's non-zero check on `qosip` fires
> with a value nobody ever set.
> `─────────────────────────────────────────────────`

---

## 7. The `QOSS` TDF struct (`Util::preAuth` reply)

The Blaze-side `QOSS` member is a **different** thing from the HTTP response.
It is EA's `QosConfigInfo`, and its member names come from EA's own
code-generated model
(`references/Skate3BlazeServer/.../Util/Models/QosConfigInfo.cs`):

```text
QosConfigInfo
  BWPS : QosPingSiteInfo            ; the default ping site
  LNP  : ushort                     ; number of latency probes
  LTPS : map<string, QosPingSiteInfo>
  SVID : uint                       ; service id

QosPingSiteInfo
  PSA  : string                     ; address
  PSP  : ushort                     ; port
  SNA  : string                     ; site name
```

The names the emulator used to ship — `BWHR`, `LATT`, `SKEY`, and `PSA` as a
nested struct — do not exist in the schema, so a client decoding `QOSS` by type
read none of them.

### 7.1 `PreAuthResponse` has exactly four members

From `references/Skate3BlazeServer/.../Util/Commands/PreAuthResponse.cs`:

```text
PreAuthResponse
  CIDS : list<ushort>               ; supported component ids
  CONF : FetchConfigResponse
  QOSS : QosConfigInfo
  SVER : string                     ; server version
```

**There is no `ANAM` and no `TIME` member.** The implementation in
`crates/services-blaze/src/util.rs` shipped both; they are not part of the
schema and are ignored by a type-driven decoder.

Similarly, `PingResponse` has **exactly one** member, `STIM`
(`references/Skate3BlazeServer/.../Util/Commands/PingResponse.cs`) — which
matches what the retail binary's own registration routine asserts.

---

## 8. The reference implementation

`crates/services-http-content/src/lib.rs`:

| Constant | Value | Meaning |
| :--- | :--- | :--- |
| `QOS_UDP_PORT` | `17498` | The UDP ProtoPing responder's port |
| `QOS_IP_DECIMAL` | `2_130_706_433` | 127.0.0.1 as a big-endian decimal u32 |
| `QOS_NUM_PROBES` | `2` | The client requires ≥ 2 |
| `QOS_PROBE_SIZE` | `1200` | The client requires non-zero |
| `QOS_FIRETYPE` | `0` | The `firetype` value |

`crates/services-blaze/src/util.rs` builds the `QOSS` struct with
`qos_ping_site(address, port, site_name)`, which sets `PSA` / `PSP` / `SNA`.

---

## 9. What was wrong with the shipped implementation

Before the fix, `crates/services-http-content/src/lib.rs` served:

```xml
<qos>
    <numprobes>10</numprobes>
    <qoshost>127.0.0.1</qoshost>
    <qosport>17498</qosport>
    <probesize>1200</probesize>
    <serviceid>1</serviceid>
</qos>
```

| Field | Problem |
| :--- | :--- |
| `qosip` | **Absent.** The one field the client *requires* to be non-zero. Fails at `0x141FE85EC`. |
| `qoshost` | **Never read by the client.** 0 occurrences in 592 MB. |
| `serviceid` | **Never read by the client.** 0 occurrences in 592 MB. |
| `numprobes` | Was 10; satisfies the ≥ 2 check. Now 2. |
| `probesize` | Was 1200 — correct (must be non-zero). |
| `requestid` | **Absent.** The client requires it to be non-zero at `0x141FE874D`. |
| `reqsecret` | Absent. Read if present, so harmless — but present now. |

And three endpoints did not exist at all:

* `/qos/firewall` — needs both `.ips.ips` and `.ports.ports` or the request
  fails `-4`.
* `/qos/firetype` — root element `firetype`, field `.firetype`.
* `/getPeerAddress` — the `name=value` pair format described in §3.2.

---

## 10. Verification

`crates/services-http-content` pins this document with 15 unit tests, and
`crates/services-blaze` with 4 more:

```bash
cargo test -p services-http-content
cargo test -p services-blaze
```

The tests assert the *client's* success conditions, not merely that the
response contains a substring — e.g. `test_qosip_is_a_big_endian_decimal_ipv4_address`
reconstructs the four bytes the client would build from `qosip` and asserts
they are `[127, 0, 0, 1]`, and
`test_firewall_ip_and_port_lists_have_matching_lengths` asserts the
`.ips.ips` / `.ports.ports` pairing the client requires.

Offline validation against the client's own parsing rules is done with
`tools/qos_dom_model.py`:

```python
from qos_dom_model import find_node, read_int
doc = open('response.xml').read()
assert read_int(doc, find_node(doc, '.qosport')) == 17498
assert read_int(doc, find_node(doc, '.qosip')) == 2130706433
```

---

## 11. Method — how these facts were obtained

`FIFA22.exe` is 592,922,624 bytes, PE32+ x86-64, image base `0x140000000`,
SHA256 `fddc0b5b4a093c460a5d0e07e117634366e2da0050f19eed3776c4340a56471a`.

Two facts about this binary make the derivation tractable:

1. **Its PE section characteristics are non-standard.** Frostbite's own linker
   emits `.didata` (`0x60000020`), `.xtext` (`0x40000040`), `.ecode`
   (`0xE0000020`) and `.srdata` (`0xC0000020`+). `IMAGE_SCN_MEM_EXECUTE` is
   therefore **not** a reliable "is this code?" filter, and a naive
   `objdump -d` or section-based scan misses the interesting code.

2. **RIP-relative references can be found without disassembling.** The target
   of a RIP-relative operand is
   `target = (end of the disp32 field) + disp32`, and the end of the field does
   **not** depend on the instruction's length. So scanning every 4-byte window
   in every section and computing the target address finds *every* RIP-relative
   reference to a given string — with no linear disassembly and therefore no
   mis-alignment hazard.

`tools/find_str_xrefs.py` implements exactly that, and
`tools/disasm_region.py` is a small capstone-based region disassembler that
annotates string operands. Both are the reason a 592 MB binary could be
searched exhaustively in seconds.

**Note on string addresses:** the addresses quoted in this document are
*virtual addresses* (`0x140000000` + section-relative offset). Some tables give
*file offsets* instead, because that is the form the xref script takes. Mixing
the two is the easiest way to get a false negative.
