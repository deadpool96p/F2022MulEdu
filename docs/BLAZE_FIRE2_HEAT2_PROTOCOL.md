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
| **Two independent implementations** | [`mxmstr/hp-server`](https://github.com/mxmstr/hp-server) and [`Khysnik/BlazeSDK`](https://github.com/Khysnik/BlazeSDK) | Byte-exact agreement on header layout, varint scheme, type IDs, and every composite wire form |
| **Authentic EA captures** | Real client payloads embedded in `hp-server`'s test fixtures (e.g. the 475-byte offline game report) | Ground truth for how EA actually encodes on the wire |
| **The client's own bytes** | `emulator_server.log` — 6 captured `Util::ping` requests from the real FIFA 22 client | Confirms the client emits Fire2 and what its message numbers look like |
| **Corroboration** | [`openBlase`](https://github.com/openBlase/openBlase) `Tdf/Tdf.cpp` (commented-out `CompressInteger`) | Independently confirms the 6-bit-first-byte varsize scheme |
| **Schema cross-check** | [`skate6743/Skate3BlazeServer`](https://github.com/skate6743/Skate3BlazeServer) TDF model classes | Union case numbers and member tag names |

Two implementations agreeing is strong evidence; two implementations agreeing **and**
decoding authentic EA bytes to the exact byte count is decisive. The 475-byte
offline-game-report payload referenced above decodes with **475 of 475 bytes
consumed** — not one byte of slack.

### 1.1 Reference repositories

Fetched by `scripts/fetch-references.ps1`, which pins every repo to a known commit:

| Repository | Pinned commit | Role |
| :--- | :--- | :--- |
| [`mxmstr/hp-server`](https://github.com/mxmstr/hp-server) | `59f6468a8b358743962e0acf5d90e42d904e9f1a` | **Primary.** Fire2 header + Heat2 codec + authentic EA capture fixtures |
| [`Khysnik/BlazeSDK`](https://github.com/Khysnik/BlazeSDK) | `a79b3d4a46a394c7bb256bc702bff3c72fd99366` | **Independent confirmation.** A second clean-room Fire2/Heat2 implementation |
| [`openBlase/openBlase`](https://github.com/openBlase/openBlase) | `ca33729e35be4be90a6e5df1e50322df9cf2e1b9` | Heat2 label compression + `CompressInteger` corroboration |
| [`lowlevelmetal/open-ds2-server`](https://github.com/lowlevelmetal/open-ds2-server) | `7702bbf348813dfce8be62bcfacd565009a34a28` | **Different Blaze generation** — 12-byte header, LEB128 varint. *Not applicable to FIFA 22*; useful only for the connection-analysis doc |
| [`skate6743/Skate3BlazeServer`](https://github.com/skate6743/Skate3BlazeServer) | `f248282ce91f856cbeb2f623adc966df0adfe0e3` | TDF schema model classes → union case numbers and member tag names |

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
 |  msg type (<<5) | user index (&0x1f)   |  options  |  reserved  |
 +------------------+-----------------------+-----------+------------+
```

| Offset | Size | Field | Notes |
| :--- | :--- | :--- | :--- |
| `0..4` | 4 | `payloadSize` | Big-endian u32. Length of the **payload** section only |
| `4..6` | 2 | `metadataSize` | Big-endian u16. Length of the **metadata** section. **This is not an error code** |
| `6..8` | 2 | `componentID` | Big-endian u16 |
| `8..10` | 2 | `commandID` | Big-endian u16 |
| `10..13` | 3 | `messageNumber` | Big-endian **u24** |
| `13` | 1 | `messageType` | High 3 bits: `(byte >> 5)` |
| `13` | – | `userIndex` | Low 5 bits: `byte & 0x1F` |
| `14` | 1 | `options` | `OPTION_IMMEDIATE = 0x01` |
| `15` | 1 | `reserved` | Always 0 |

### 2.1 Message types

| Value | Name |
| :--- | :--- |
| 0 | `MESSAGE` (client → server request) |
| 1 | `REPLY` (server → client response) |
| 2 | `NOTIFICATION` (server → client, unsolicited) |
| 3 | `ERROR_REPLY` |
| 4 | `PING` |
| 5 | `PING_REPLY` |

### 2.2 Frame body order

```
[ 16-byte header ] [ metadata section (TDF struct) ] [ payload (TDF struct) ]
```

The metadata section is present only when `metadataSize > 0`. It is a TDF struct
containing `CNTX` (context) and, for error replies, `ERRC` (error code). The
error code lives in the **metadata**, not in the header.

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
| `options` / `reserved` | 0 / 0 | |

Decoded as the legacy 12-byte header, the same bytes yield `component = 0x0000`,
which is not a valid Blaze component. **Fire2 is correct; the 12-byte layout is not.**

The six captured client requests carry message numbers 0, 1, 2, 3, 4, 5 — stored in
byte 12, with bytes 10–11 as the high and middle octets. The client's requests
always carry `messageType = 0` (`MESSAGE`) and `userIndex = 0`.

---

## 3. Heat2 TDF Codec

### 3.1 Tag compression

Four ASCII characters pack into three bytes, 6 bits per character:

```
packed |= uint32((char - 0x20) & 0x3F) << (26 - 6*i)   for i in 0..4
```

Lowercase is uppercased first. The first byte carries bits 26–31, the second bits
20–25, the third bits 14–19. A zero 6-bit group means "space" and terminates the
name.

Verified byte-exact against authentic EA captures:

| Tag | Packed bytes | Seen in |
| :--- | :--- | :--- |
| `GID` | `9E 99 00` | EA capture (`hp-server` test fixture) |
| `PCAP` | `C2 38 70` | EA capture (`hp-server` test fixture) |
| `STIM` | `CF 4A 6D` | Our `Util::ping` reply, matching EA's |

### 3.2 Integer encoding — "varsize"

**This is the single most important detail in this document.**

Heat2 integers are *not* LEB128. The first byte carries only **6** value bits:

```
first byte:  bit 7 = continuation, bit 6 = negative, bits 0-5 = value
continuation bytes: bit 7 = continuation, bits 0-6 = value
continuation value bits sit at shifts 6, 13, 20, 27, ...
```

Reference implementation (identical in both `hp-server` and `BlazeSDK`):

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
independently confirming it.

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
| 6 | `UNION` | | |

> ⚠️ `OBJECT_TYPE` is **8** and `OBJECT_ID` is **9** — not the other way round.
> `FLOAT` is **10** and `TIMEVALUE` is **11**.

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

| Command | Name |
| :--- | :--- |
| `0x0001` | `fetchClientConfig` |
| `0x0002` | **`ping`** |
| `0x0003` | `setClientData` |
| `0x0007` | `userSettingsSave` |
| `0x0008` | `userSettingsLoad` |
| `0x000A` | `userSettingsLoadAll` |
| `0x000B` | **`preAuth`** |
| `0x000C` | **`postAuth`** |

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

**`Util::PreAuthResponse`**: `ANAM`, `CIDS`, `QOSS`, `SVER`, `TIME`,
`serverVersion`, `telemetryServer`, `tickerServer`, `underageSupported`,
`entitlementSource`, `registrationSource`, `legalDocGameIdentifier`, `suspendTime`

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
