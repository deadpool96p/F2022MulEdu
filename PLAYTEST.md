# FIFA 22 Clean-Room Multiplayer: Playtest & Operational Guide

This document describes how to execute reproducible local network (LAN) playtests and remote multiplayer matches using **`deadpool96p/F2022MulEdu`**.

---

## 1. Quick-Start: Reproducible LAN Match (Copy-Paste Commands)

### Step 1: Start the Clean-Room Emulator Server (Host Machine)
Run this command from the repository root to launch all emulator services (Redirector `:42127`, Blaze `:42128`, Telemetry `:443`, QoS `:17498`, ProtoTunnel Relay `:3659`, and Control API `:42129`):

```bash
cargo run --release -p emulator-server -- --trace-blaze host_match.ndjson
```

*(Optional: To test with a custom SSL Common Name, regenerate certs beforehand with: `cargo run -p emulator-server -- certgen --cn custom.blaze.ea.com`)*

### Step 2: Launch the Game Client (Both Machines)
On both Host and Guest machines (ensure `shim/build/dxgi.dll` and `f22_emu.ini` are present in your game root):

```cmd
start_client.bat
```
*(Alternatively: start the companion hub with `python tools/companion_ui.py` and click **Launch FIFA 22**).*

---

## 2. Matchmaking Path A: Steam-Independent Room-Code Flow (F1 Overlay)

This path allows direct LAN or WAN peer connection without requiring Steam or Steamworks AppID 480.

1. **Launch `FIFA22.exe`** on both machines.
2. In-game, press **`F1`** to toggle the built-in DirectX 12 clean-room overlay.
3. **Player 1 (Host):**
   * In the overlay under **Room Matchmaking**, click **Host Room**.
   * A unique 6-character room code (e.g., `RF92K1`) will be generated and copied to your clipboard.
   * Direct UDP punch-through endpoints are queried automatically via the QoS responder on port `17498`.
4. **Player 2 (Guest):**
   * Press **`F1`** to open the overlay.
   * Enter the 6-character code into the **Room Code** input box and click **Join Room**.
5. **Kickoff:**
   * Both clients synchronize in the lobby.
   * In the FIFA 22 main menu, navigate to **Play > Online Friendlies > Friendly Season**.
   * The server advances the session state symmetrically (`NotifyPlayerJoining` $\rightarrow$ `NotifyJoiningPlayerInitiateConnections` $\rightarrow$ `NotifyPlatformHostInitialized` $\rightarrow$ `NotifyGameReportingIdChange` $\rightarrow$ `GameState 130` $\rightarrow$ `GameState 131`) into kickoff.

---

## 3. Matchmaking Path B: Steam Invite Flow (Spacewar AppID 480)

1. Ensure **Steam is running** on both machines.
2. Launch `FIFA22.exe` with `EnableSteam = true` in `f22_emu.ini`.
3. Press **`Shift + Tab`** to bring up the native Steam overlay.
4. Right-click your friend's profile and select **Invite to Game** (or **Join Game**).
5. The `steam_bridge` module intercepts the Steam P2P packet exchange and synchronizes lobby states directly into the local Blaze server.

---

## 4. Observability: Netcode HUD & Metrics

The emulator exposes live telemetry through `GET /metrics` on both `:44325` (HTTP content server) and `:42129` (Control API):

```bash
curl http://127.0.0.1:42129/metrics
```

**Sample Output:**
```json
{
  "rtt_ms": 18,
  "jitter_ms": 2,
  "dropped_frames": 0,
  "cipher_reuse_count": 14208
}
```

* **`cipher_reuse_count`**: Increments on every forwarded UDP frame using the per-session `SessionCipher`, verifying zero allocation thrashing.
* **In-Game HUD**: The in-game `F1` overlay displays live RTT, jitter, and transport mode (`DirectP2P` vs `ProtoTunnelRelay`). If `SYMMETRIC_NAT` or `PORT_RESTRICTED` is detected by the QoS probe, the server automatically switches to relay mode.

---

## 5. Message Tracing & Trace Diff Tool (`tools/blaze_trace_diff.py`)

To identify why one match kicks off smoothly while another stalls, compare the server's Blaze message traces.

### Capturing Traces
Run the server with `--trace-blaze <path>`:
```bash
cargo run -p emulator-server -- --trace-blaze run1.ndjson
```

### Diffing Two Traces
```bash
python tools/blaze_trace_diff.py run1.ndjson run2.ndjson
```

**Example Output (Divergence Found):**
```text
[*] Loaded 42 records from 'run1.ndjson'
[*] Loaded 41 records from 'run2.ndjson'
----------------------------------------------------------------------
[!] FIRST DIVERGENCE AT MESSAGE INDEX #18:
    Trace A: OUT GameManager::NotifyJoiningPlayerInitiateConnections (comp=0x0004, cmd=0x0017)
    Trace B: OUT GameManager::GameStateChange (comp=0x0004, cmd=0x0064)
    -> RPC Target Mismatch: A invoked (4,23), B invoked (4,100)
----------------------------------------------------------------------
[!] Traces diverge. Stopping comparison.
```

**Example Output (Identical Traces):**
```text
[+] SUCCESS: Traces match identically (42 messages verified with 0 divergences).
```

Add `--verbose` to inspect matching RPC messages up to the divergence point.
