# Blaze Protocol & Frostbite Identity Invariants

> **Note (2026-10-21):** Rules 1 and 2 were **falsified** during the post-`ping`
> stall investigation and have been corrected below. The detailed protocol
> specification now lives in
> [`docs/BLAZE_FIRE2_HEAT2_PROTOCOL.md`](../docs/BLAZE_FIRE2_HEAT2_PROTOCOL.md),
> and the investigation record is in
> [`docs/POST_PING_STALL_ROOT_CAUSE.md`](../docs/POST_PING_STALL_ROOT_CAUSE.md).
> Read those before changing anything in `blaze-core` or `blaze-tdf`.

1. **Fire2 Frame Detection**:
   - FIFA 22 speaks **Fire2**: a 16-byte header, *not* the 12-byte legacy FIRE
     header. Do **not** validate frame headers using `buffer[10] <= 3` — under
     Fire2, byte 10 is the **high octet of the 24-bit message number**, and
     byte 13 holds `(messageType << 5) | userIndex`. The old heuristic is
     meaningless under Fire2 and will misclassify any request whose message
     number is ≥ 65536.
   - The client's own bytes settle it: `00 00 00 00 00 00 00 09 00 02 …`
     decodes under Fire2 as `component = 0x0009` (Util), `command = 0x0002`
     (ping) — both correct. Under the 12-byte layout it decodes as
     `component = 0x0000`, which is not a valid Blaze component.
   - Modern Frostbite clients batch game-specific component RPCs (e.g. `0x08BA`)
     on the same stream, so component IDs must never be whitelisted.

2. **Ping Keepalive Response**:
   - `Blaze::Util::PingResponse` (Comp=0x0009, Cmd=0x0002) MUST include `STIM`
     (Server Time, Unix epoch seconds) — and **exactly one member**. EA's own
     registration routine in the retail binary sets a member count of 1, so the
     earlier theory that `PING`/`LATN`/`TIME` tags were missing was wrong.
   - The real defect was **the integer encoding**, not the member list. Heat2
     integers are *varsize* encoded with a **6-bit first byte** (bit 6 = sign,
     bit 7 = continuation; continuation bytes at shifts 6, 13, 20, 27…). Plain
     7-bit LEB128 silently decodes to a *different number*. Encoding `STIM` with
     LEB128 made the client read `896249714` (1998-05-27) instead of
     `1792499378` (2026-10-20) — a server 28 years in the past — which trips
     its clock-drift check and the 10-second watchdog.
   - **Invariant:** every integer on the wire must use the Heat2 varsize scheme.
     `crates/blaze-tdf/tests/tdf_fire2_wire.rs` pins this.

3. **Identity Federation**:
   - When emulating offline/LAN multiplayer with Steam integration, ensure the
     local platform emulator configuration (e.g. `anadius.cfg`) and the Blaze
     `SessionInfo` struct (`DSNM`, `PERSONA`, `UID`, `PID`) reflect the
     identical player persona name (`DEad>_<POol`) to satisfy the game client's
     identity check and eliminate offline reconnect prompts.

4. **Heat2 Type IDs and Composite Wire Forms**:
   - Type IDs: `0` INTEGER, `1` STRING, `2` BLOB, `3` TDF, `4` LIST, `5` MAP,
     `6` UNION, `7` VARIABLE, **`8` OBJECT_TYPE**, **`9` OBJECT_ID**,
     **`10` FLOAT**, **`11` TIMEVALUE**, `12` GENERIC.
   - **UNION** carries its own member tag: `[index][tag(3)][type][value]`.
     Omitting the tag desynchronises the parser by 3 bytes.
   - **VARIABLE** is `[0x01][tdfId][tag(3)][type][value][0x00]`; a not-present
     variable is just `[0x00]`.
   - **OBJECT_TYPE / OBJECT_ID** use `varsize(component) + varsize(type)`
     (+ `varsize(id)`), **not** raw big-endian u16 pairs.
   - The root payload of a Fire2 frame has **no** trailing `0x00` terminator;
     nested structs do.

5. **Do Not Re-derive Protocol From the Wrong Generation**:
   - `open-ds2-server` and `Skate3BlazeServer` implement the **12-byte legacy
     FIRE** generation with a 7-bit LEB128 varint. Their *TDF model classes* are
     a good source of schema names and union case numbers; their *wire format*
     is **not** applicable to FIFA 22. `openBlase` is the exception — its
     `Tdf::CompressInteger` corroborates the Heat2 varsize scheme.
   - Any protocol claim must be backed by one of: authentic EA bytes, the
     client's own captured bytes, or the retail binary's own code (its TDF
     registration routines and schema tables). See
     `docs/BLAZE_FIRE2_HEAT2_PROTOCOL.md` §1.
   - **Do not cite `hp-server` and `BlazeSDK` as two independent sources.**
     `hp-server` is a *fork* of BlazeSDK — its history contains BlazeSDK's
     commits verbatim and `Fire2Types.go` is byte-identical between them.
     A fork and its parent are one source. Cite them as corroboration, and
     cite authentic EA bytes or the binary as the actual evidence.

6. **Reference Repositories Are Not Vendored**:
   - The five reference repositories live outside version control and are
     fetched by `scripts/fetch-references.ps1`, which pins each to a known
     commit. Do not commit them, and do not modify their contents.
