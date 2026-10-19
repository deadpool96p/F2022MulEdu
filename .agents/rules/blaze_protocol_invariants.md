# Blaze Protocol & Frostbite Identity Invariants

1. **Fire2 Frame Detection**:
   - Always evaluate incoming frame headers using structural validation (`buffer[10] <= 3` for valid `MsgType` enum) rather than whitelisting component IDs. Modern Frostbite clients batch game-specific component RPCs (e.g. `0x08BA`) on the same stream.

2. **Ping Keepalive Response**:
   - In FIFA 22 BlazeSDK (15.1.1.10.3), `Blaze::Util::PingResponse` defines 0 fields in its Heat2 schema. Sending unexpected fields (e.g. `STIM`) triggers `BLAZE_ERR_TDF_TAG_NOT_FOUND` in `Heat2Decoder`. Ping responses must be encoded as an empty struct (`TdfStruct::new()`, length 0).
   - **FIXED (2026-10-19):** `crates/services-blaze/src/util.rs::handle_ping` was emitting `STIM` (Unix epoch), violating this invariant. Root cause of the 10s watchdog EOF on port 42128. Now returns an empty struct. Regression test `util::tests::test_ping_response_is_empty_struct_no_stim` asserts 0 payload fields.
   - **FIXED (2026-10-19):** `crates/services-blaze/src/router.rs` PerfComponent `0x08BA` handler was also emitting `STIM` for every command. Since `0x08BA/0x0005` is batched with `Util::ping` in one 32-byte segment (synchronous perf probe, not an independent RPC), it now returns an empty struct per the same Heat2 schema invariant. **UNVERIFIED HYPOTHESIS** — no Ghidra-confirmed `0x08BA/0x0005` schema exists yet (gap documented in `docs/disassembly_and_engine_multiplayer_plan.md` Phase 3). If the real schema requires fields, capture the wire bytes with `tools/dissector/blaze.lua` in Wireshark and replace the empty response with the exact tags.

3. **Identity Federation**:
   - When emulating offline/LAN multiplayer with Steam integration, ensure the local platform emulator configuration (e.g. `anadius.cfg`) and the Blaze `SessionInfo` struct (`DSNM`, `PERSONA`, `UID`, `PID`) reflect the identical player persona name (`DEad>_<POol`) to satisfy the game client's identity check and eliminate offline reconnect prompts.
