# EA Blaze "Fire2" Framing & "Heat2" TDF Codec — FIFA 22 Protocol Reference

**Document Version:** 2.0.0
**Last Updated:** October 21, 2026
**Status:** Authoritative working reference for `crates/blaze-core` and `crates/blaze-tdf`
**Applies to:** FIFA 22 PC Retail `1.0.77.45722` (Frostbite 3, BlazeSDK 15, DirtySDK 15)

> This document replaces the earlier assumption that FIFA 22 speaks the 12-byte
> "legacy FIRE" framing. **It does not.** FIFA 22 speaks **Fire2** (16-byte header)
> with a **Heat2** payload codec. Several older Blaze generations (and two of the
> reference repos consulted here) use the 12-byte header with a plain 7-bit LEB128
> varint; applying those to FIFA 22 produces frames that *decode without error but
> with the wrong values* — the single most dangerous class of protocol bug.

---

## 1. How This Document Was Established

Every claim below is backed by at least one of:

| Evidence class | Source | What it proves |
| :--- | :--- | :--- |
| **Authentic EA captures** | Real client payloads embedded in `hp-server`'s test fixtures (e.g. the 475-byte offline game report) | Ground truth for how EA actually encodes on the wire |
| **The client's own bytes** | `emulator_server.log` — 6 captured `Util::ping` requests from the real FIFA 22 client | Confirms the client emits Fire2 and what its message numbers look like |
| **The retail binary** | `FIFA22.exe` — its own TDF registration routines and schema tables | Member counts, type IDs, and the exact member names of each response struct |
| **Corroboration** | [`openBlase`](https://github.com/openBlase/openBlase) `Tdf/Tdf.cpp` (commented-out `CompressInteger`) | Corroborates the 6-bit-first-byte varsize scheme |
| **Schema cross-check** | [`skate6743/Skate3BlazeServer`](https://github.com/skate6743/Skate3BlazeServer) TDF model classes | Union case numbers and member tag names |

> ⚠️ **Correction (October 21, 2026).** An earlier version of this document
> claimed that `hp-server` and `Khysnik/BlazeSDK` were "two independent
> clean-room implementations" and cited their agreement as the primary
> evidence. **That was wrong.** `hp-server` is a **fork of BlazeSDK**: its git
> history contains BlazeSDK's commits verbatim (`a79b3d4`, `cd401d2`,
> `ff31b33`, `111288b`, `3d815d4`, `4fbb2c7`, `2d88c44`, `fd17417`, …), and
> their `Fire2.go` / `Fire2Encoder.go` differ only by one added exported
> helper. `Fire2Types.go` is byte-identical. A fork and its parent are one
> source, not two — treating them as independent is exactly how a shared
> misreading survives unnoticed.
>
> The evidence standard below is therefore **authentic EA bytes or the
> client's own bytes**, and the corroborating sources are explicitly labelled
> as corroboration rather than as independent confirmation. The 475-byte
> offline-game-report payload decodes with **475 of 475 bytes consumed** — not
> one byte of slack — which is the decisive evidence, and it does not depend on
> the fork relationship at all.

### 1.0 Evidence standard

Every protocol claim in this repository is backed by at least one of:

1. **Authentic EA bytes** — the 475-byte offline-game-report payload in
   `hp-server`'s fixtures decodes with 475 of 475 bytes consumed.
2. **The client's own captured bytes** — `emulator_server.log`.
3. **The retail binary's own code** — e.g. EA's registration routine for
   `PingResponse` sets a member count of 1, which is why `STIM` alone is
   correct.

Corroboration (but *not* independent confirmation) comes from `hp-server` /
`BlazeSDK` (one codebase, forked) and `openBlase`'s commented-out
`Tdf::CompressInteger`.

> **See also:** [`FIFA22_QOS_PROTOPING_HTTP.md`](FIFA22_QOS_PROTOPING_HTTP.md) —
> the `/qos/*` HTTP endpoints, recovered from the binary alone, where the
> open-source references are demonstrably wrong about the field the client
> actually requires.

### 1.1 Reference repositories

Fetched by `scripts/fetch-references.ps1`, which pins every repo to a known commit:

| Repository | Pinned commit | Role |
| :--- | :--- | :--- |
| [`mxmstr/hp-server`](https://github.com/mxmstr/hp-server) | `59f6468a8b358743962e0acf5d90e42d904e9f1a` | **Primary.** Fire2 header + Heat2 codec + **authentic EA capture fixtures** (the decisive evidence) |
| [`Khysnik/BlazeSDK`](https://github.com/Khysnik/BlazeSDK) | `a79b3d4a46a394c7bb256bc702bff3c72fd99366` | **The upstream of `hp-server`, not an independent source.** Same wire format; `Fire2Types.go` is byte-identical and the other files differ by one exported helper. Cite as corroboration only. |
| [`openBlase/openBlase`](https://github.com/openBlase/openBlase) | `ca33729e35be4be90a6e5df1e50322df9cf2e1b9` | Heat2 label compression + `CompressInteger` corroboration. Its `Tdf.h` enum is an *older, different* TDF type set — do not copy type IDs from it. |
| [`lowlevelmetal/open-ds2-server`](https://github.com/lowlevelmetal/open-ds2-server) | `7702bbf348813dfce8be62bcfacd565009a34a28` | **Different Blaze generation** — 12-byte header, LEB128 varint. *Not applicable to FIFA 22*; useful only for the connection-analysis doc |
| [`skate6743/Skate3BlazeServer`](https://github.com/skate6743/Skate3BlazeServer) | `f248282ce91f856cbeb2f623adc966df0adfe0e3` | TDF schema model classes → union case numbers and member tag names. **Its `Util` command models are EA's own** and are the source of truth for `PreAuthResponse` / `PingResponse` / `QosConfigInfo` member lists. |

---

## 2. Fire2 Frame Header (16 bytes)

```
 0        1        2        3        4        5        6        7
 +--------+--------+--------+--------+--------+--------+--------+--------+
 |                     payload size (u32 BE)         |  metadata size  |
 +--------+--------+--------+--------+--------+--------+--------+--------+
 |      component ID (u16 BE)       |       command ID (u16 BE)      |
 +--------+--------+--------+--------+--------+--------+--------+--------+
 |                            message number (u24 BE)              | |
 +--------+--------+--------+--------+--------+--------+--------+--------+
 |  msg type (<<5) | user index (&0x1f)   |    zero   |    zero    |
 +------------------+-----------------------+-----------+------------+
```

| Offset | Size | Field | Notes |
| :--- | :--- | :--- | :--- |
| `0..4` | 4 | `payloadSize` | Big-endian u32. Length of the **payload** section only — metadata is *excluded* |
| `4..6` | 2 | `metadataSize` | Big-endian u16. Length of the **metadata** section. **This is not an error code** |
| `6..8` | 2 | `componentID` | Big-endian u16 |
| `8..10` | 2 | `commandID` | Big-endian u16 |
| `10..13` | 3 | `messageNumber` | Big-endian **u24** |
| `13` | 1 | `messageType` | High 3 bits: `(byte >> 5)` |
| `14` | 1 | *(no field)* | **Always zero.** Not an options byte. |
| `15` | 1 | *(no field)* | **Always zero.** |

> ⚠️ **Correction (October 21, 2026).** An earlier version of this document
> gave byte 14 as an `options` byte with `OPTION_IMMEDIATE = 0x01`. **There is
> no options byte in FIFA 22's Fire2.** The serializer writes a zeroed 16-bit
> word at bytes 14–15 as a single instruction:
>
> ```
> 0x14490604E  xor  esi, esi              ; esi = 0
> ...
> 0x144906186  mov  word ptr [rbx + 0xe], si   ; bytes 14..15 = 0x0000
> ```
>
> An exhaustive census of every store in the Fire2 codec region
> (`0x144905000`–`0x144907600`) found `0x144906186` to be the **only**
> instruction that writes header bytes 14 or 15 anywhere in the binary, and
> the deserializer never reads them. The `Header.Options` field in the Go
> reference is a Go-only invention.
>
> The asymmetry is a trap: if the server sets byte 14 to a non-zero value,
> FIFA 22's parser ignores it, but the *server's* parser will read it as an
> options field that FIFA never sent. `crates/blaze-core` still carries an
> `options` field for API symmetry — it must always be written as 0.

### 2.1 Message types

The type enum is `0..5`, normalised by a switch at `0x1449060FF`–`0x14490613C`
with an identity mapping for 0–5. The deserializer branches on 4 and 5 for
the keepalive pair (`0x14490652F`, `0x144906572`).

| Value | Name |
| :--- | :--- |
| 0 | `MESSAGE` (client → server request) |
| 1 | `REPLY` (server → client response) |
| 2 | `NOTIFICATION` (server → client, unsolicited) |
| 3 | `ERROR_REPLY` |
| 4 | `PING` |
| 5 | `PING_REPLY` |

#### The keepalive pair

A `PING` is a **transport-level** frame, not an RPC request. It carries no
payload and no metadata, and it must be answered with a `PING_REPLY` — also
with no payload — echoing the request's 24-bit message number and 5-bit user
index.

It must **not** be answered with an ordinary `REPLY`. The client reads byte 13
as `messageType << 5 | userIndex`, so a `REPLY` (type 1) and a `PING_REPLY`
(type 5) are distinguishable, and the client treats anything other than a
`PING_REPLY` as an unsatisfied keepalive. A live capture shows the client
sending a `PING` frame, the server replying with a `REPLY`, and the client
dropping the connection and reconnecting three seconds later.

`crates/emulator-server/src/blaze_server.rs` therefore intercepts message type
4 at the transport layer, before the router sees it, and writes a
`PING_REPLY` built by `BlazePacket::ping_reply`.

### 2.2 Frame body order

```
[ 16-byte header ] [ metadata section (TDF struct) ] [ payload (TDF struct) ]
```

The metadata section is present only when `metadataSize > 0`. It is an ordinary
Heat2 TDF struct — same codec as the payload — and it is written **first**, with
its length back-patched into bytes 4–5.

#### `payloadSize` counts the payload only

The serializer computes it as `cursor − metadataSize − 16` and back-patches it
big-endian into bytes 0–3:

```
0x1449061E9  call [rax + 0x20]            ; write the PAYLOAD object
0x144906200  movzx eax, byte ptr [rbx + 4]
0x144906204  shl  ax, 8
0x14490620B  movzx eax, byte ptr [rbx + 5]
0x14490620F  or   rcx, rax                 ; rcx = metadataSize
0x144906212  mov  rax, qword ptr [rdi + 0x10]   ; write cursor
0x144906216  sub  rax, rcx                ; - metadataSize
0x14490621E  sub  rax, rbx
0x144906221  sub  rax, 0x10               ; - 16-byte header  => PAYLOAD SIZE
0x144906237  mov  byte ptr [rbx + 3], al
0x14490623D  mov  byte ptr [rbx], cl      ; bytes 0..3 = payloadSize (BE u32)
```

So the total frame length on the wire is
`payloadSize + metadataSize + 16`.

#### The metadata struct has **nine** members in FIFA 22

The member-registration function at `0x1402C49C0` registers nine members, each
storing a codec-factory pointer, a member-name string pointer, a 32-bit dword
(high 24 bits = packed tag, low byte = wire type), a member offset and a size:

| Tag | Member | Wire type | C++ type |
| :--- | :--- | :--- | :--- |
| `CNTX` | `context` | `0x00` INTEGER | `uint64_t` |
| `ERRC` | `errorCode` | `0x00` INTEGER | `int32_t` |
| `FLAG` | `flags` | `0x00` INTEGER | `Blaze::Fire2Metadata::Flags` |
| `LCTX` | `logContext` | `0x01` STRING | |
| `MADR` | `movedToAddr` | `0x00` INTEGER | `uint16_t` |
| `MIID` | `movedTo` | `0x00` INTEGER | `uint32_t` |
| `NAME` | `movedToHostName` | `0x00` INTEGER | `uint32_t` |
| `SIDN` | `sliverIdentity` | `0x00` INTEGER | `uint32_t` |
| `SKEY` | `sessionKey` | `0x00` INTEGER | `uint32_t` |

> The Go reference's `BuildMetadata` emits only `CNTX` and `ERRC`. That is
> *sufficient* to interoperate (a tag-keyed lookup finds them) but it is
> **incomplete**: a decoder that assumes exactly two members will break on the
> seven the client may also send. Decode by tag, and tolerate unknown tags.

The error code lives in the **metadata** (`ERRC`), not in the header — the
16-byte header has no error field at all.

### 2.3 Proof that FIFA 22 uses Fire2

The client's very first request, captured in `emulator_server.log`:

```
00 00 00 00 00 00 00 09  00 02 00 00 00 00 00 00
```

Decoded as Fire2:

| Field | Value | Meaning |
| :--- | :--- | :--- |
| `payloadSize` | 0 | Ping has no request payload |
| `metadataSize` | 0 | No metadata |
| `componentID` | `0x0009` | **Util** — correct, this is `Util::ping` |
| `commandID` | `0x0002` | **`ping`** — correct |
| `messageNumber` | 0 | First message in the session |
| `messageType` | 0 (`MESSAGE`) | Client → server request |
| `userIndex` | 0 | |
| bytes 14–15 | 0 / 0 | Always zero — not an options field |

Decoded as the legacy 12-byte header, the same bytes yield `component = 0x0000`,
which is not a valid Blaze component. **Fire2 is correct; the 12-byte layout is not.**

The six captured client requests carry message numbers 0, 1, 2, 3, 4, 5 — stored in
byte 12, with bytes 10–11 as the high and middle octets. The client's requests
always carry `messageType = 0` (`MESSAGE`) and `userIndex = 0`.

---

## 3. Heat2 TDF Codec

### 3.1 Tag compression

**Four** ASCII characters pack into three bytes, 6 bits per character:

```
packed |= uint32((char - 0x20) & 0x3F) << (18 - 6*i)   for i in 0..4
```

Lowercase is uppercased first. The four 6-bit groups occupy bits 18–23, 12–17,
6–11 and 0–5 of the 24-bit tag. A **zero** 6-bit group terminates the name, so
tags are 1 to 4 characters; a 4-character tag uses all 24 bits and a 3-character
tag leaves the low group at zero.

Verified byte-exact against authentic EA captures:

| Tag | Packed bytes | Seen in |
| :--- | :--- | :--- |
| `GID` | `9E 99 00` | EA capture (`hp-server` test fixture) |
| `PCAP` | `C2 38 70` | EA capture (`hp-server` test fixture) |
| `STIM` | `CF 4A 6D` | Our `Util::ping` reply, matching EA's |

Verified byte-exact against the **retail binary's own member-registration
table** — the nine `Fire2Metadata` members at `0x1402C49C0` each store a 32-bit
dword whose high 24 bits are the packed tag and whose low byte is the wire type.
All nine decode back to their member names:

| Tag | Packed bytes | Member | Wire type |
| :--- | :--- | :--- | :--- |
| `CNTX` | `8E ED 38` | `context` | `0x00` INTEGER |
| `ERRC` | `97 2C A3` | `errorCode` | `0x00` INTEGER |
| `FLAG` | `9A C8 67` | `flags` | `0x00` INTEGER |
| `LCTX` | `B2 3D 38` | `logContext` | `0x01` STRING |
| `MADR` | `B6 19 32` | `movedToAddr` | `0x00` INTEGER |
| `MIID` | `B6 9A 64` | `movedTo` | `0x00` INTEGER |
| `NAME` | `BA 1B 65` | `movedToHostName` | `0x00` INTEGER |
| `SIDN` | `CE 99 2E` | `sliverIdentity` | `0x00` INTEGER |
| `SKEY` | `CE B9 79` | `sessionKey` | `0x00` INTEGER |

> ⚠️ **Correction (October 21, 2026).** An earlier version of this document
> gave the *shift schedule* as `26 - 6*i`. That is wrong — the correct
> schedule is `18 - 6*i`, and the tag is **four** characters wide, not three.
> The mistake was caught by decoding the binary's nine `Fire2Metadata` tag
> dwords: they yield `CNTX`/`ERRC`/`FLAG`/… only under the 4-character,
> 18/12/6/0 schedule. EA's own decoder
> (`references/BlazeSDK/Fire2Decoder.go`) confirms it:
>
> ```go
> func DecodeTag(buffer []byte) string {
>     tag := uint32(buffer[0])<<16 | uint32(buffer[1])<<8 | uint32(buffer[2])
>     var buf [4]byte
>     size := 4
>     for i := 3; i >= 0; i-- {   // <-- four characters, LSB group first
>         sixbits := tag & 0x3f
>         if sixbits != 0 { buf[i] = byte(sixbits + 32) } else { buf[i] = 0; size = i }
>         tag >>= 6
>     }
>     return string(buf[:size])
> }
> ```
>
> `crates/blaze-tdf/src/tag.rs` was already correct on this point
> (`p0 << 18 | p1 << 12 | p2 << 6 | p3`) — it was the *document* that was
> wrong.

### 3.2 Integer encoding — "varsize"

**This is the single most important detail in this document.**

Heat2 integers are *not* LEB128. The first byte carries only **6** value bits:

```
first byte:  bit 7 = continuation, bit 6 = negative, bits 0-5 = value
continuation bytes: bit 7 = continuation, bits 0-6 = value
continuation value bits sit at shifts 6, 13, 20, 27, ...
```

Reference implementation (from `hp-server` / `BlazeSDK`; corroborated by
`openBlase`'s commented-out `Tdf::CompressInteger`):

```go
func EncodeVarsizeInteger(value int64) []byte {
    negative := value < 0
    remaining := uint64(value)
    if negative {
        remaining = uint64(-value)
    }
    firstByte := byte(remaining & 0x3f)
    remaining >>= 6
    if negative {
        firstByte |= 0x40
    }
    if remaining > 0 {
        firstByte |= 0x80
    }
    out := []byte{firstByte}
    for remaining > 0 {
        nextByte := byte(remaining & 0x7f)
        remaining >>= 7
        if remaining > 0 {
            nextByte |= 0x80
        }
        out = append(out, nextByte)
    }
    return out
}
```

`openBlase`'s commented-out `Tdf::CompressInteger` implements the identical scheme,
corroborating it.

The retail binary's own encoder confirms it byte-for-byte
(`FUN_14496BAA0` at `0x14496BAA0`):

```
0x14496BAD4  movzx eax, dl
0x14496BAD7  and  al, 0x3f        ; positive: 6 value bits
0x14496BAD9  or   al, 0x80        ; + continuation flag
0x14496BAB0  sar  rdx, 6          ; first continuation shift = 6
0x14496BAF6  or   cl, 0x80        ; continuation bytes carry 7 bits + 0x80
0x14496BAF9  sar  rdx, 7          ; subsequent shifts = 7,7,7 -> 6,13,20,27
0x14496BB0D  and  byte ptr [rax + r9], 0x7f  ; clear 0x80 on the LAST byte
```

and the negative path sets `0x40` (the sign flag) on the first byte:

```
0x14496BACE  movzx eax, dl
0x14496BAD0  or   al, 0xc0        ; 0x40 = negative, 0x80 = continuation
```

The sign is applied to the *magnitude*; `0x40` is the only sign carrier. Value
`0` encodes as a single `0x00` byte.

#### Why this matters — the `STIM` bug

`Util::ping`'s `STIM` (server time) field carries a Unix epoch second. The value
`1792499378` (2026-10-20 12:29:38Z) encodes as:

| Encoding | Bytes | What EA decodes it as |
| :--- | :--- | :--- |
| Heat2 (correct) | `B2 FA BA AD 0D` | `1792499378` — 2026-10-20 ✔ |
| LEB128 (wrong) | `B2 BD DD D6 06` | `896249714` — **1998-05-27** ✘ |

Heat2 decodes LEB128 bytes **silently into a different number** — no error, no
disconnect, just a timestamp 28 years in the past. The client reads a server that
appears to be running circa 1998, fails its clock-drift check, and eventually drops
the connection.

Our pre-fix ping payload was:

```
CF 4A 6D 00 B2 BD DD D6 06 00
   tag   ^int  ^^^^^^^^^^^^^  ^root terminator
```

The correct payload is:

```
CF 4A 6D 00 B2 FA BA AD 0D 00
```

#### Value ranges

| Value | Bytes | Note |
| :--- | :--- | :--- |
| `0` | `00` | |
| `1` | `01` | |
| `62` | `3E` | Last single-byte value |
| `63` | `3F` | |
| `64` | `80 01` | First multi-byte value |
| `127` | `BF 01` | |
| `16383` | `BF FF 01` | |
| `16384` | `80 80 02` | |
| `-1` | `41` | Sign flag in bit 6 |
| `-64` | `C0 01` | |
| `-1000` | `E8 0F` | |

### 3.3 Wire type IDs

| ID | Name | ID | Name |
| :--- | :--- | :--- | :--- |
| 0 | `INTEGER` | 7 | `VARIABLE` |
| 1 | `STRING` | 8 | **`OBJECT_TYPE`** |
| 2 | `BLOB` | 9 | **`OBJECT_ID`** |
| 3 | `TDF` (struct) | 10 | **`FLOAT`** |
| 4 | `LIST` | 11 | **`TIMEVALUE`** |
| 5 | `MAP` | 12 | `GENERIC` |
| 6 | `UNION` | 13 | *unknown/default* |

> ⚠️ `OBJECT_TYPE` is **8** and `OBJECT_ID` is **9** — not the other way round.
> `FLOAT` is **10** and `TIMEVALUE` is **11**.

#### Two type namespaces, and the translation between them

The **internal TDF type id** used by the C++ reflection layer (`0x00`–`0x17`)
is **not** the wire type byte (`0x00`–`0x0D`). `FUN_14496BB30` at
`0x14496BB30` translates internal → wire via a 24-entry jump table at
`0x14496BBB4`:

| Internal id | → wire byte | | Internal id | → wire byte |
| :--- | :--- | :--- | :--- | :--- |
| `0x00` | `0x0D` (default) | | `0x09` | `0x02` BLOB |
| `0x01` | `0x0C` GENERIC | | `0x0A` | `0x06` UNION / `0x03` TDF |
| `0x02` | `0x05` MAP / `0x03` TDF | | `0x0B` | `0x03` TDF |
| `0x03` | `0x04` LIST / `0x03` TDF | | `0x0C` | `0x08` OBJECT_TYPE |
| `0x04` | `0x0A` FLOAT | | `0x0D` | `0x09` OBJECT_ID |
| `0x05` | `0x00` INTEGER | | `0x0E` | `0x0B` TIMEVALUE |
| `0x06` | `0x01` STRING | | `0x0F`–`0x17` | `0x00` INTEGER |
| `0x07` | `0x07` VARIABLE | | `> 0x17` | `0x0D` (default) |

The `MAP`/`UNION`/`LIST` → `0x03` fallback is the **TDF-vs-container**
distinction: a map, union or list whose value is really a nested TDF struct is
written with the plain `0x03` struct wire type.

The encoder's own 24-case dispatch in `FUN_14496AC20` at `0x14496AC20`
(jump table at `0x14496B9A0`) agrees with the mapper exactly, which is why the
table above is confirmed from two independent binary sources.

> ⚠️ **The `0x0D` default code matters.** The Go reference uses `0xFF` as its
> out-of-range sentinel; the binary uses **`0x0D`**. And `0x0C` `GENERIC` is
> missing from `crates/blaze-tdf`'s `TdfType` — see §7.

### 3.4 Composite wire forms

These are where a "close enough" implementation silently breaks.

#### Struct (type 3)

```
[ tag (3) ] [ 0x03 ] [ member fields... ] [ 0x00 terminator ]
```

#### String (type 1)

```
[ tag (3) ] [ 0x01 ] [ varsize length ] [ UTF-8 bytes ] [ 0x00 ]
```

The length **includes** the NUL terminator. A 200-character string has length 201
→ `89 03` in Heat2 (vs `C9 01` in LEB128).

#### List (type 4)

```
[ tag (3) ] [ 0x04 ] [ element type byte ] [ varsize count ] [ elements... ]
```

#### Map (type 5)

```
[ tag (3) ] [ 0x05 ] [ key type byte ] [ value type byte ] [ varsize count ] [ pairs... ]
```

EA sets the map's *value* type to the **leaf** element type, not the container type.

#### Union (type 6) — ⚠️ carries its own member tag

```
[ tag (3) ] [ 0x06 ] [ active member index ] [ member tag (3) ] [ member type ] [ value ]
```

The active member's **own 3-byte tag is present between the index and the type
byte**. An implementation that writes only `[index][type][value]` is missing 3
bytes and desynchronises the parser.

Empty unions use index `0xFF` (or `0x7F`); no member follows.

Confirmed on authentic EA bytes: `(active = 2, member = 'ULAV', type = TDF)`.

#### Variable (type 7) — ⚠️ present flag, TDF id, member tag, terminator

```
[ tag (3) ] [ 0x07 ] [ 0x01 present ] [ varsize tdfId ] [ member tag (3) ] [ member type ] [ value ] [ 0x00 terminator ]
```

A not-present variable is just `[0x07][0x00]`.

Confirmed on authentic EA bytes: `(tdfId = 1085761969, member = 'EMAG', type = TDF)`.

#### ObjectType (type 8) / ObjectId (type 9) — ⚠️ varsize, not raw u16

```
OBJECT_TYPE: [ varsize(component) ] [ varsize(type) ]
OBJECT_ID:   [ varsize(component) ] [ varsize(type) ] [ varsize(id) ]
```

Components are **not** written as big-endian u16 pairs.

#### Float (type 10) / TimeValue (type 11)

```
FLOAT:     [ 4 bytes, IEEE 754 big-endian ]
TIMEVALUE: [ varsize integer, microseconds ]
```

### 3.5 Root struct terminator

The **root** payload of a Fire2 frame is a TDF struct **without** a trailing `0x00`
terminator — the frame length delimits it. Nested structs *do* carry their own
terminator.

Verified against an authentic EA capture: the 13-byte payload
`9E 99 00 00 01 C2 38 70 04 00 02 02 00` decodes to `{GID: 1, PCAP: [2, 0]}` and
consumes exactly 13 bytes.

> Note: `crates/blaze-tdf` currently appends a root terminator. This is a
> discrepancy to resolve — the 12-byte legacy path depends on the current
> behaviour, so it is tracked as a separate, deliberate change rather than folded
> into the Fire2 header work.

---

## 4. Component & Command IDs

### 4.1 Components

| ID | Component | ID | Component |
| :--- | :--- | :--- | :--- |
| 1 | Authentication | 15 | Messaging |
| 3 | Example | 25 | AssociationLists |
| 4 | GameManager | 27 | GpsContentController |
| 5 | Redirector | 28 | GameReporting |
| 7 | Stats | 31 | ByteVault |
| 9 | **Util** | 33 | Achievements |
| 10 | CensusData | 1025 | XBLSystemConfigs |
| 11 | Clubs | 1031 | Friends |
| — | — | 7802 | UserSessions |
| 802 | Packs | 803 | Inventory |
| 805 | PvzGw | | |

### 4.2 Util commands (component 0x0009)

From EA's own `UtilMessage` enum
(`references/Skate3BlazeServer/.../Blaze/MessageLists/UtilMessages.cs`):

| Command | Name |
| :--- | :--- |
| `0x0001` | `fetchClientConfig` |
| `0x0002` | **`ping`** |
| `0x0003` | `setClientData` |
| `0x0004` | `localizeStrings` |
| `0x0005` | `getTelemetryServer` |
| `0x0006` | `getTickerServer` |
| `0x0007` | **`preAuth`** |
| `0x0008` | **`postAuth`** |
| `0x000A` | `userSettingsLoad` |
| `0x000B` | `userSettingsSave` |
| `0x000C` | `userSettingsLoadAll` |
| `0x000D` | `userSettingsLoadAllForUserId` |
| `0x0014` | `filterForProfanity` |
| `0x0015` | `fetchQosConfig` |
| `0x0016` | `setClientMetrics` |
| `0x0017` | `setConnectionState` |

> **This table was wrong until the authentic EA `preAuth` reply was decoded.**
> It used to place `preAuth` at `0x000B` and `userSettingsSave` at `0x0007`.
> Because a Blaze server dispatches on the numeric command ID alone, the
> mistake was silent: the emulator answered the FIFA 22 client's opening
> request — `Comp=0x0009 Cmd=0x0007`, a 230-byte client-identity payload —
> with an empty `userSettingsSave` reply, and the client never received a
> `preAuth` response. It then fell into a three-second reconnect loop.
>
> Three independent sources now agree that `0x0007` is `preAuth`:
>
> 1. EA's `UtilMessage` enum, quoted above.
> 2. EA's own `PreAuthHandler`
>    (`references/Skate3BlazeServer/.../Util/Handlers/PreAuthHandler.cs`),
>    which builds its response with the literal header bytes
>    `0, 0, 0, 9, 0, 7, 0, 0, 0, 0, 0, 0` — component `0x0009`, command
>    `0x0007`.
> 3. The two authentic EA `preAuth` replies, whose payloads decode to a
>    `PreAuthResponse`-shaped struct carrying `CIDS`, `CONF`, `QOSS` and
>    `SVER`. The fixtures live in
>    `crates/services-blaze/tests/fixtures/`, and
>    `crates/services-blaze/tests/preauth_matches_ea.rs` pins the
>    emulator's reply to their shape.
>
> See §4.4 for the `preAuth` reply's members.

### 4.3 Schema field names (from the FIFA 22 binary's own schema tables)

Extracted from the retail executable and cross-checked against the reference
repos' TDF model classes. Only the fields this project actually emits are listed.

**`Util::PingResponse`** — exactly **one** member:

| Tag | Type | Meaning |
| :--- | :--- | :--- |
| `STIM` | INTEGER | Server time, Unix epoch seconds |

> EA's own registration routine for `PingResponse` sets a member count of **1**.
> The earlier hypothesis that `PING`/`LATN`/`TIME` tags were missing was
> **false** — `STIM` alone is correct, and the real defect was the varint encoding.

**`Util::PreAuthResponse`** — **four** members in EA's own code-generated
model, **fourteen** on the wire:

| Tag | Type | Meaning |
| :--- | :--- | :--- |
| `CIDS` | LIST of ushort | Supported component ids |
| `CONF` | `FetchConfigResponse` | Server configuration |
| `QOSS` | `QosConfigInfo` | QoS ping-site configuration |
| `SVER` | STRING | Server version |

> ⚠️ **Correction (October 21, 2026).** An earlier version of this document
> listed `ANAM`, `TIME`, `serverVersion`, `telemetryServer`, `tickerServer`,
> `underageSupported`, `entitlementSource`, `registrationSource`,
> `legalDocGameIdentifier` and `suspendTime` as `PreAuthResponse` members.
> **None of them exist.** EA's own code-generated model
> (`references/Skate3BlazeServer/.../Util/Commands/PreAuthResponse.cs`) has
> exactly the four members above, and the retail binary's registration routine
> agrees. `ANAM` and `TIME` were being emitted by
> `crates/services-blaze/src/util.rs`; a type-driven decoder ignores them, so
> they are dead weight on the wire.
>
> See [`FIFA22_QOS_PROTOPING_HTTP.md`](FIFA22_QOS_PROTOPING_HTTP.md) §7 for
> the `QOSS` member list and the `QosConfigInfo` / `QosPingSiteInfo` schemas.
>
> ⚠️ **Correction (October 22, 2026).** The *model* has four members, but
> the *wire* has more — see §4.4. The four-member model is the open-source
> reconstruction of one game's schema; it is not what a retail server sends.

### 4.4 The `preAuth` reply on the wire

The two authentic EA `preAuth` replies — Blaze 15.1.1.4.6 and 18.3.0 — decode
to these root members:

| Tag | Type | Meaning |
| :--- | :--- | :--- |
| `ASRC` | STRING | asset source id |
| `CIDS` | LIST of uint | component ids the server implements |
| `CLID` | STRING | server id |
| `CONF` | STRUCT | `FetchConfigResponse` — see below |
| `ESRC` | STRING | |
| `INST` | STRING | product id |
| `MAID` | uint | master account id |
| `MINR` | uint | |
| `NASP` | STRING | authentication namespace |
| `PILD` | STRING | |
| `PLAT` | STRING | platform |
| `QOSS` | STRUCT | `QosConfigInfo` |
| `RSRC` | STRING | |
| `SVER` | STRING | server version, e.g. `Blaze 18.3.0 (CL# 2115104)` |

The 18.3.0 reply adds three members that belong to that game's schema
(`HPLT`, `LQTK`, `RELT`); the fourteen above are the ones both replies agree
on, and the ones the emulator sends.

Three details in that structure are easy to get wrong, and each one was:

**`CONF` is a struct wrapping a map, not a bare map.** EA's
`FetchConfigResponse` has a single member, `CONF`, of type
`map<string, string>`. Both authentic replies decode as `CONF (struct)`
containing `CONF (map)`. The emulator used to send a bare map.

**`QOSS` is a struct of its own**, holding `BWPS` (a `QosPingSiteInfo`),
`LNP`, `LTPS` (a map of alias → `QosPingSiteInfo`) and `SVID`. Each
`QosPingSiteInfo` carries `PSA` (address), `PSP` (port) and `SNA` (site
name).

**Members are emitted in alphabetical tag order.** The authentic replies are
sorted (`ASRC`, `CIDS`, `CLID`, `CONF`, …). The Rust `TdfStruct` is a
`BTreeMap`, so the writer emits them in that order for free.

The fixtures are in `crates/services-blaze/tests/fixtures/`, and
`crates/services-blaze/tests/preauth_matches_ea.rs` asserts that the
emulator's reply carries exactly the fourteen members above, that its `CONF`
has EA's shape, and that its `QOSS` points at the emulator's own responder
rather than at a real EA host.

**`Util::PreAuthRequest`**: `clientVersion`, `clientType`, `locale`,
`platformId`, `productId`, `blazeVersion`, `clientBuild`, `macAddress`

**`Util::PostAuthResponse`**: `TELE` (`ADRS`, `PORT`, `SKEY`), `TICK` (`ADRS`, `PORT`, `SKEY`)

**`Util::FetchConfigResponse`**: `CONF` (map)

**`UserSessions::NetworkAddress` (union)** — case numbers from the Skate3 TDF models:

| Case | Member | Type |
| :--- | :--- | :--- |
| 2 | `IpPairAddress` | struct |
| 3 | `IpAddress` | struct (dirtycast servers) |

**`UserSessions::IpPairAddress`**: `EXIP` (external IP), `INIP` (internal IP)

**`UserSessions::IpAddress`**: `IP`, `PORT`, `HOST`

**`UserSessions::UserSessionExtendedData`**: `ADDR` (union), `BPS`, `CTY`, `HWFL`,
`UID`, `QOS` (`NATT`, `BWIN`, `BWOT`, `DBW`, `UBW`)

**`UserSessions::UserData`**: `AID`, `ALST`, `EXID`, `ID`, `NAME`, `PID`, `DSNM`

**`GameManager::GameSetup`**: `GAME` (`GID`, `GAM`, `ATTR`, `CAP`, `STAT`, `TOP`,
`NQOS`, `MODE`, `SLOT`, `PROS`), `PROS` (list of `PROS`)

**`GameManager::PlayerData`**: `PID`, `PLID`, `NAME`, `GID`, `SLOT`, `STATE`,
`ADDR`, `NQOS`, `TEAM`, `ATTR`

---

## 5. Known pitfalls

| Pitfall | Consequence |
| :--- | :--- |
| Using LEB128 for Heat2 integers | Silent wrong values; `STIM` becomes a 1998 timestamp |
| Assuming the 16-byte header's bytes 4–6 are an error code | Metadata length misread as error code; frame desynchronises |
| Putting `msgType` in byte 10 | Byte 10 is the message number's high octet; the client never matches the reply |
| Omitting the union's 3-byte member tag | 3-byte desync on every union-bearing payload |
| Omitting the variable's present flag / tdf id / terminator | Parser desync on any payload containing a variable |
| Writing object components as raw u16 BE | Wrong byte count for every object id/type |
| Swapping `OBJECT_TYPE` and `OBJECT_ID` | Object references decode as the wrong type |
| Detecting Fire2 with `buffer[10] <= 0x03` | Byte 10 is a message-number octet; the heuristic is meaningless under Fire2 and misclassifies frames |

---

## 6. Regenerating the schema string dump

The field-name tables in §4.3 were derived by extracting printable strings from the
retail executable and filtering for Blaze schema identifiers. The raw dump is
**not committed** (it is extracted content from a proprietary binary); it is
regenerated locally with:

```powershell
# Extracts schema strings from the retail executable into blaze_schema_strings.txt
# (script intentionally not committed; requires the local game install)
```

The filtering criteria are:

1. Address-sorted printable runs of ≥ 4 characters from the data sections.
2. Matches on `Blaze::<Component>::<Type>` and bare schema identifiers.
3. Cross-referenced against the reference repositories' TDF model classes
   (`references/Skate3BlazeServer/.../Models/`) for union case numbers and member
   tag names.

---

## 7. Verification

`crates/blaze-tdf/tests/tdf_fire2_wire.rs` pins this document's claims as
executable tests:

- 21 exact varsize vectors transcribed from the reference implementation
- agreement with the reference across `-100_000..=100_000`
- the exact EA `STIM` payload bytes
- string length encoding including the NUL terminator

```bash
cargo test -p blaze-tdf --test tdf_fire2_wire
```
