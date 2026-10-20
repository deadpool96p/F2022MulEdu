# Blaze Protocol & Frostbite Identity Invariants

1. **Fire2 Frame Detection**:
   - Always evaluate incoming frame headers using structural validation (`buffer[10] <= 3` for valid `MsgType` enum) rather than whitelisting component IDs. Modern Frostbite clients batch game-specific component RPCs (e.g. `0x08BA`) on the same stream.

2. **Ping Keepalive Response**:
   - In Blaze protocol / DirtySDK, `Blaze::Util::PingResponse` (Comp=0x0009, Cmd=0x0002) MUST include `STIM` (Server Time, Unix epoch seconds). DirtySDK evaluates `STIM` to synchronize clock drift and compute latency round-trips. If `STIM` is omitted (0-byte payload), DirtySDK treats the ping transaction as incomplete and drops the socket after 10 seconds.
   - The earlier 10-second watchdog disconnect was actually caused by the 16-byte header bit-shift bug (Byte 10 was serialized as `0x10` instead of unshifted `0x01` `RecordResponse`). Once Byte 10 is correctly packed as `0x01`, `PingResponse` with `STIM` is properly accepted by DirtySDK, completing the handshake.

3. **Identity Federation**:
   - When emulating offline/LAN multiplayer with Steam integration, ensure the local platform emulator configuration (e.g. `anadius.cfg`) and the Blaze `SessionInfo` struct (`DSNM`, `PERSONA`, `UID`, `PID`) reflect the identical player persona name (`DEad>_<POol`) to satisfy the game client's identity check and eliminate offline reconnect prompts.
