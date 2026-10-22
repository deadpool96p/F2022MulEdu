# FIFA 22 QoS / ProtoPing HTTP Reference

**Document Version:** 1.2.0
**Last Updated:** October 22, 2026
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

The three path formats and every query-parameter literal were read out of
the string pool (file offsets in `FIFA22.exe`):

| Endpoint | Path format | Offset |
| :--- | :--- | :--- |
| `/qos/qos` | `%s/qos/qos` | `0x7794760` |
| `/qos/firewall` | `%s/qos/firewall` | `0x7794770` |
| `/qos/firetype` | `%s/qos/firetype` | `0x7794788` |

| Query parameter | Offset | Used by |
| :--- | :--- | :--- |
| `?vers=` | `0x7794754` | all three |
| `&qtyp=` | `0x7794780` | `/qos/qos` — selects request type 1 or 2 |
| `&nint=` | `0x7794798` | `/qos/firewall` — the interface count |
| `&rqid=` | `0x77947A8` | `/qos/firetype` — the request id |
| `&rqsc=` | `0x77947A0` | `/qos/firetype` — the request secret |
| `&inip=` | `0x77946B8` | `/qos/firetype` — the client's own IP |
| `&inpt=` | `0x77946B0` | `/qos/firetype` — the client's own port |

So the requests look like:

```
%base%/qos/qos?vers=<v>&qtyp=<1|2>
%base%/qos/firewall?vers=<v>&nint=<n>
%base%/qos/firetype?vers=<v>&rqid=<id>&rqsc=<secret>&inip=<ip>&inpt=<port>
```

> ⚠️ **Correction (October 22, 2026).** An earlier draft of this document
> listed only `?vers=`, `&qtyp=`, `&nint=`, `&rqsc=` and `&rqid=`, and
> omitted `&inip=` and `&inpt=`. Both are present in the binary.
>
> The *order* of the parameters within a URL is not established — the
> literals are appended to the path one at a time, and the order in which
> the appends happen was not traced. The set is what is proven.

`rqid`/`rqsc` are the request id and secret; the response carries the same
two values back as `<requestid>` and `<reqsecret>`, and the client reads
those two names from the response (offsets `0x77946E8` and `0x77946D8`).
The client **rejects** a response whose `requestid` is zero
(`0x141FE874D`), so a server must never answer with `0`.

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

The client's request URL is built from the format string at file offset
`0x79F6010`:

```
http://%s:%d/getPeerAddress?myIP=%s&myPort=%d&version=1.0
```

Note the scheme: this URL is **plain `http`**, not `https`, and it is
`host:port` — unlike the three `/qos/*` URLs, which go through the
`"%s://%s:%u"` base and are therefore `https`.

A second, longer variant of the query exists at file offset `0x79F5F28`:

```
myIP=%s&myPort=%d&version=1.0&status=%s&gameFeatureID=%s
```

and `&gameID=` at `0x79F6050` is appended to it. The client sends
`Cookie: sessionID=%s` (`0x79F5FF0` and `0x79F6060`) and
`Content-Type: application/x-www-form-urlencoded` (`0x79F6076`).

The IP fields here are parsed with a **dotted-quad** parser (`0x140C96010`) —
unlike `qosip`, which is a decimal u32 (see §4).

> Note on the names: `sourceIP`, `sourcePort`, `tag` and `sendCount` appear
> **twice** in the client's string pool — once as bare lookup names
> (`0x79F5F68` `sourcePort`, `0x79F5FC0` `sendCount`, `0x79F5FB0`
> `targetPort`, `0x79F5FD0` `peerPort`, `0x79F5FE0` `targetIP`, `0x79F6008`
> `peerIP`) and once as `name=value` **request** format strings
> (`0x79F5F78` `sourceIP=%s`, `0x79F5F85` `sourcePort=%d`, `0x79F5F94`
> `tag=%s`, `0x79F5F9C` `sendCount=%d`). The bare `tag` and `sourceIP`
> used as response lookup keys were confirmed by resolving the
> RIP-relative references in the client's `getPeerAddress` parser
> (`0x144AA092D` -> `tag`, and the six others in the table above).

#### 3.2.1 Four of the request parameters arrive in the *body*

The four `name=value` request format strings above are **not** part of
either URL format the client builds (`0x79F6010` and `0x79F5F28`); those
URLs carry only `myIP`, `myPort`, `version`, and optionally `status`,
`gameFeatureID` and `gameID`. The client sends
`Content-Type: application/x-www-form-urlencoded` (`0x79F6076`), so
`sourceIP`, `sourcePort`, `tag` and `sendCount` arrive as an
`application/x-www-form-urlencoded` **body**.

**FACT.** The sink therefore reads request parameters from the query
string *and* from the form body
(`HttpRequest::form_pairs` / `HttpRequest::param` in
`crates/services-http-content/src/lib.rs`). The body is only parsed as
form data when the `Content-Type` says it is form data, so a
`text/plain` body is never mis-parsed.

This also means the method is **not** necessarily a GET for this
endpoint: a request that carries a form body is a POST. The emulator
answers both (§3.5).

#### 3.2.2 What the response values mean

| Field | Value the emulator sends | Why |
| :--- | :--- | :--- |
| `sourceIP` | echoed from `myIP` / `sourceIP` | The client's own address |
| `sourcePort` | echoed from `myPort` / `sourcePort` | The client's own port |
| `tag` | echoed from the request's `tag` | The client's correlation tag |
| `sendCount` | echoed from the request's `sendCount` | The client's own counter |
| `targetIP`, `peerIP` | `127.0.0.1` | The peer is the emulator itself |
| `targetPort`, `peerPort` | `3659` | The ProtoTunnel relay port |

**ASSUMPTION.** The *values* of `targetIP` / `targetPort` / `peerIP` /
`peerPort` are not stated by the client. The emulator hosts *both*
players, so the peer is the emulator; the address it must be reached at
is the one the Blaze session already publishes for a player
(`127.0.0.1:3659`, the ProtoTunnel relay that `crates/emulator-server`
binds and that `services-blaze` reports as the session port).

Note that this is **not** `QOS_UDP_PORT` (17498): that port carries the
ProtoPing latency probes, not peer traffic. The two must not be
conflated — the client learns 17498 from `/qos/qos` (§4) and 3659 from
the Blaze session and from `/getPeerAddress`.

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

### 3.4 The ProtoHttp user agent and request format

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

#### 3.4.1 The request ProtoHttp actually puts on the wire

The full request template lives in the same cluster (file offsets in
`FIFA22.exe`):

| Template | Offset |
| :--- | :--- |
| `%s %s%s HTTP/1.1` — the request line | `0x7575DC8` |
| `Host: %s:%d` | `0x7575D98` |
| `Accept: */*` | `0x7575D18` |
| `User-Agent: %s` | `0x7575D50` |
| `Content-Length: %qd` | `0x7575D68` |
| `Connection: Close` | `0x7575D80` |

Captured live from the client (`emulator_server.log`, the
`/redirector/getServerInstance` request):

```http
POST /redirector/getServerInstance HTTP/1.1
Host: spring18.gosredirector.ea.com:42230
Content-Length: 707
Connection: Close
User-Agent: ProtoHttp 1.3/DS 15.1.7.0.3 (Windows)
Content-Type: application/xml
```

So a QoS probe arrives as:

```http
GET /qos/qos?vers=..&qtyp=.. HTTP/1.1
Host: qosapi:<port>
Accept: */*
Connection: Close
User-Agent: ProtoHttp 1.3/DS 15.1.7.0.3 (Windows)
```

> ⚠️ The `Host` header is **not** the address the client connected to.
> The client connects to a bare IP and sends `Host: qosapi:<port>`. A
> server must therefore route on the **path**, never on `Host`.

#### 3.4.2 What a ProtoHttp client needs back

ProtoHttp is a plain HTTP/1.1 client. Its response parser only recognises
these header names (file offsets `0x7575CC0`–`0x7575D08`):

```
connection   transfer-encoding   chunked   content-length   last-modified   location
```

So a response to a ProtoHttp client must:

* be framed by an **exact `Content-Length`** — never chunked, and
* **close the connection**, because the client sends `Connection: Close`.

A response that is chunked, or that omits `Content-Length`, hangs the
client waiting for a body that never arrives.

#### 3.4.3 The HTTP method

**FACT.** The client selects the method from a *string pool* at file
offset `0x6DFBA50` (VA `0x146DFDC50`) holding, at 16-byte stride:

```
HEAD   GET   POST   PUT   DELETE   OPTIONS   PATCH   CONNECT
```

The pool is referenced from the ProtoHttp library itself
(`0x140C739C5`), i.e. the method is chosen by an enum index stored on the
request object, not by a literal in the QoS module. The QoS module
contains **no** reference to the strings `GET` or `POST`.

**FACT (independent corroboration).** The open-ds2 server analysis states
the method explicitly:

* `references/open-ds2-server/docs/03-main-executable-analysis.md` line 630:
  `HTTP GET: /qos/qos, /qos/firetype, /qos/firewall`
* `references/open-ds2-server/server/include/network/qos_server.hpp` line 21:
  `GET /qos/qos?vers=<version>&qtyp=<type>`

**ASSUMPTION.** The method for the FIFA 22 QoS probes is therefore taken to
be **GET**, on the strength of the open-ds2 corroboration plus the fact
that a query-only endpoint with no body is a GET in every other
implementation. This was **not** proven from a disassembly of
`FIFA22.exe`: the method index is set on the request object somewhere
earlier in the call chain than the URL builder at `0x141FE94D0`, and that
write was not located.

Because the method was not proven, the emulator's handlers are
**method-agnostic** — `/qos/qos`, `/qos/firewall`, `/qos/firetype` and
`/getPeerAddress` answer `GET` and `POST` alike. The method therefore
cannot be a source of failure.

### 3.5 The endpoint contract

What `crates/services-http-content` serves. `Content-Type` is the only
header that differs between the endpoints; `Content-Length` is always the
exact body length and the connection always closes.

| Path | Method | Status | `Content-Type` | Body |
| :--- | :--- | :--- | :--- | :--- |
| `/qos/qos` | GET (POST also accepted) | `200 OK` | `application/xml` | `<qos>` with `qosip`, `qosport`, `numprobes`, `probesize`, `requestid`, `reqsecret` |
| `/qos/firewall` | GET (POST also accepted) | `200 OK` | `application/xml` | `<firewall>` with `numinterfaces`, `ips/ips`, `ports/ports`, `requestid`, `reqsecret` |
| `/qos/firetype` | GET (POST also accepted) | `200 OK` | `application/xml` | `<firetype>` with `firetype` |
| `/getPeerAddress` | GET (POST also accepted) | `200 OK` | `text/plain` | `name=value` lines — **not** XML |

Request parameters are read from the query string **and** from an
`application/x-www-form-urlencoded` body (§3.2.1), so `rqid`, `rqsc`,
`myIP`, `myPort`, `sourceIP`, `sourcePort`, `tag` and `sendCount` are
honoured wherever the client puts them.

**ASSUMPTION.** The `Content-Type` values are not read by the client: the
QoS parser is a flat text scanner over the body (§2) and the
`getPeerAddress` parser splits on `=` and CR/LF. The values are set
because they describe the bodies correctly, not because the client
demands them. `text/plain` for `/getPeerAddress` is the one that matters
cosmetically — the body genuinely is not XML, and serving it as
`application/xml` was a defect.

**ASSUMPTION.** The `200 OK` status. The client treats a non-2xx
ProtoHttp response as a transport error, so `200` is the only safe
choice, but the exact status the retail QoS service returns was not
observed.

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

### 7.1 `PreAuthResponse` has four members *in the model*

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

> ⚠️ **Correction (October 22, 2026).** "Exactly four members" is true of the
> *open-source model*, not of the *wire*. Two authentic EA `preAuth` replies
> captured off a live Blaze server (`crates/services-blaze/tests/fixtures/`)
> carry **fourteen** root members:
>
> ```text
> ASRC  CIDS  CLID  CONF  ESRC  INST  MAID  MINR  NASP  PILD  PLAT  QOSS  RSRC  SVER
> ```
>
> A newer server adds `HPLT`, `LQTK` and `RELT` (game-specific). The four the
> model names are a subset — the model was generated for a different game
> build and simply does not describe every member the retail server sends.
> The emulator must send the full set, because the client decodes the reply
> by the *wire* schema, not by the model. See
> `docs/BLAZE_FIRE2_HEAT2_PROTOCOL.md` §4.4.
>
> Two shape details also only the wire reveals: `CONF` is a **struct wrapping
> the map**, not a bare map, and the root struct carries **no trailing
> terminator** (nested structs do).

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
| `PEER_RELAY_UDP_PORT` | `3659` | The ProtoTunnel relay's port (the peer address) |
| `PEER_ADDRESS` | `127.0.0.1` | The peer address, in dotted-quad form |

`QOS_UDP_PORT` is not a free choice: the emulator already binds a UDP
socket on `:17498` for the ProtoPing responder
(`crates/emulator-server/src/lib.rs`), and `QOS_IP_DECIMAL` is
`127.0.0.1` — the address the client reaches the emulator on. The client
dials exactly the `(ip, port)` pair this response advertises.

`PEER_RELAY_UDP_PORT` is the same kind of constraint: the emulator binds
the ProtoTunnel relay on `:3659`, and the Blaze session publishes the
same port as the player's network address. It is the *peer* port, not
the QoS probe port — see §3.2.2.

`crates/services-blaze/src/util.rs` builds the `QOSS` struct with
`qos_ping_site(address, port, site_name)`, which sets `PSA` / `PSP` / `SNA`.

### 8.1 The request id and secret are echoed

The client sends `rqid` and `rqsc` as query parameters (§3.1) and reads
`<requestid>` and `<reqsecret>` back from the response. The emulator
therefore echoes them:

| Request | Response |
| :--- | :--- |
| `?rqid=42&rqsc=7` | `<requestid>42</requestid>` `<reqsecret>7</reqsecret>` |
| *(absent)* | `<requestid>1</requestid>` `<reqsecret>0</reqsecret>` |
| `?rqid=0` | `<requestid>1</requestid>` — **never zero** |

The last row matters: the client aborts the QoS step with `-4` when
`requestid` is zero (`0x141FE874D`), so a client that sends no id — or a
zero one — must still be given a non-zero id.

### 8.2 ProtoHttp user-agent handling

`crates/services-http-content/src/protohttp.rs` parses the user agent and
exposes it as `ProtoHttpUserAgent`:

```rust
let ua = req.proto_http_user_agent();   // Option<ProtoHttpUserAgent>
```

* `http_version` — the ProtoHttp library version (`1.3`).
* `ds_version` — the DirtySDK version (`15.1.7.0.3`).
* `platform` — the parenthesised token (`Some("Windows")`), optional.

A request that carries a ProtoHttp user agent is answered with a
ProtoHttp-shaped response (§3.4.2): an exact `Content-Length`, no chunked
framing, and `Connection: Close`. The response also carries a
`User-Agent` header of its own — **ASSUMPTION**: the client does not read
a `User-Agent` *response* header, so this is cosmetic, and it exists only
so the ProtoHttp path is visible on the wire.

### 8.3 The sink speaks HTTP/1.1 only

`crates/services-http-content` implements HTTP/1.1. The client offers
`h2` in its ALPN extension, and the emulator's TLS acceptor advertises
**no** ALPN protocol at all, so the offer is declined and ProtoHttp falls
back to HTTP/1.1 — which the client accepts.

`parse_http_request` detects the HTTP/2 connection preface
(`PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n`) and refuses it, because the preface's
first line would otherwise parse as an HTTP/1.1 request line (`PRI * HTTP/2.0`)
and be silently mis-routed to the fast-path fallback. The transport logs
the preface at `warn` level so it cannot pass unnoticed.

### 8.4 What the sink logs

Every request is logged at `info` with the full request line, the path
*including* its query string, the `Host` header, the `User-Agent`, the
`Content-Type`, whether the caller is ProtoHttp, and the ALPN (when the
transport supplied it):

```
[HTTP Sink] request: GET /qos/qos?vers=1&qtyp=2 | host=qosapi:443 |
  ua=ProtoHttp 1.3/DS 15.1.7.0.3 (Windows) | ctype=<none> | protohttp=true | alpn=<tls-layer>
```

An unmapped path is fast-pathed with `200 OK` and
`{"status":"ok","code":0,"server_status":0}` so the client never stalls
waiting for a response.

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

### 9.1 What the second pass fixed (October 22, 2026)

| Defect | Consequence | Fix |
| :--- | :--- | :--- |
| `/getPeerAddress` was served as `application/xml` | The body is `name=value` lines, not XML | Served as `text/plain` |
| `requestid` was the constant `1` | The client's own `rqid` was ignored | `rqid`/`rqsc` are echoed; the constant is only the fallback |
| The ProtoHttp user agent was not recognised | A ProtoHttp client got the generic response shape | `ProtoHttpUserAgent` parsing; ProtoHttp requests get `Connection: Close` and an exact `Content-Length` |
| The request line, `Host` and `User-Agent` were not logged | An unmapped endpoint logged only `Fast-pathing unmapped endpoint: /genericEvents` | Every request is logged at `info` with the full request line, path + query, `Host`, `User-Agent`, `Content-Type`, the ProtoHttp flag and the ALPN |
| The HTTP/2 connection preface was parsed as a request line | An h2 connection was silently fast-pathed and answered with HTTP/1.1 | The preface is detected and refused; the transport logs it at `warn` |

### 9.2 What the third pass fixed (October 22, 2026)

| Defect | Consequence | Fix |
| :--- | :--- | :--- |
| Only the query string was read for request parameters | `sourceIP`, `sourcePort`, `tag` and `sendCount` — which the client sends in an `application/x-www-form-urlencoded` **body** (§3.2.1) — were silently ignored | `HttpRequest::form_pairs` / `HttpRequest::param` read the body as well; a body is only parsed as form data when the `Content-Type` says so |
| The `getPeerAddress` response hard-coded `tag=emulator` and `sendCount=1` | The client's own correlation values did not survive the round trip | Both are echoed from the request |
| The `getPeerAddress` peer address pointed at the **QoS** port (17498) | 17498 is the ProtoPing responder; peer traffic goes over the ProtoTunnel relay | The peer address is now `127.0.0.1:3659`, the relay port the emulator binds and the Blaze session publishes (§3.2.2) |
| The QoS request id was read from the query string only | A client that puts `rqid` in a form body lost its correlation | `rqid`/`rqsc` are read through `HttpRequest::param` |

---

## 10. Verification

`crates/services-http-content` pins this document with 38 unit tests, and
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

The ProtoHttp and request-shape facts are pinned the same way:

| Test | Asserts |
| :--- | :--- |
| `test_proto_http_user_agent_is_recognised` | The live user agent parses into `1.3` / `15.1.7.0.3` / `Windows` |
| `test_proto_http_requests_get_a_proto_http_shaped_response` | Exact `Content-Length`, `Connection: Close`, no `Transfer-Encoding` |
| `test_non_proto_http_user_agent_is_not_recognised` | A browser user agent is not mistaken for ProtoHttp |
| `test_host_header_does_not_affect_routing` | `Host: accounts.ea.com` still reaches `/qos/qos` |
| `test_qos_endpoints_accept_both_get_and_post` | Every QoS path answers `GET` and `POST` |
| `test_qos_response_echoes_the_client_request_id` | `rqid`/`rqsc` come back as `requestid`/`reqsecret` |
| `test_qos_request_id_is_never_zero` | `requestid` is never `0`, even for `?rqid=0` |
| `test_get_peer_address_is_served_as_text_plain` | The `name=value` body is not served as XML |
| `test_get_peer_address_carries_every_name_the_client_looks_up` | All seven lookup names are present |
| `test_get_peer_address_echoes_the_requested_address` | `sourceIP`/`sourcePort` echo the request; the peer is `127.0.0.1` |
| `test_get_peer_address_reads_the_form_body` | `sourceIP`/`sourcePort`/`tag`/`sendCount` are read from the form body and echoed |
| `test_a_non_form_body_is_not_parsed_as_form_data` | A `text/plain` body is never mis-parsed as form data |
| `test_the_advertised_peer_port_is_a_port_the_emulator_binds` | The peer port is `3659`, the relay the emulator binds |
| `test_the_advertised_qos_port_is_the_port_the_emulator_binds` | `qosport` is `17498` |
| `test_no_response_points_at_a_real_ea_host` | No response body contains an EA URL or hostname |
| `test_http2_preface_is_detected_and_not_parsed_as_http11` | The h2 preface is never mis-routed |

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
