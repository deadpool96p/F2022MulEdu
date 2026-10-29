# Seamless FIFA 22: Network Hooking Specification

## 1. Overview & Hooking Objective

In FIFA 22 (Frostbite 3 Engine), all multiplayer match gameplay is powered by Electronic Arts' **DirtySDK** networking middleware (`CommUDP`, `NetGameDist`, `ProtoTunnel`). 

Our goal is to intercept the game's network input/output at the Windows Sockets (`ws2_32.dll`) boundary, isolate the DirtySDK gameplay stream from EA telemetry and cloud services, encapsulate gameplay packets within a custom lightweight envelope, and route them to our custom relay server.

---

## 2. DirtySDK Networking Architecture in FIFA 22

```mermaid
graph TD
    subgraph FIFA22 ["FIFA22.exe Process Space"]
        Sim["Frostbite Game Simulation"] --> Dist["DirtySDK NetGameDist (Lockstep Sync)"]
        Dist --> Comm["DirtySDK CommUDP / ProtoTunnel"]
        Comm --> WS2["ws2_32.dll APIs (sendto / recvfrom / bind)"]
    end

    subgraph Shim ["seamless_fifa.dll Detours"]
        WS2 --> HookBind["Hooked_bind()"]
        WS2 --> HookSend["Hooked_sendto()"]
        WS2 --> HookRecv["Hooked_recvfrom()"]
        
        HookBind -->|Identify UDP Port 3659| StateTracker["Socket State Tracker"]
        HookSend --> FilterSend{"Filter Destination"}
        FilterSend -->|Game UDP (3659)| Encapsulate["Wrap with Seamless Envelope"]
        FilterSend -->|Telemetry / Cloud| DropBlock["Drop or Return 0 (Silent Drop)"]
        
        HookRecv --> FilterRecv{"Filter Source"}
        FilterRecv -->|Seamless Relay Packet| Decapsulate["Unwrap Envelope -> Return to Game"]
        FilterRecv -->|External Packet| PassRecv["Passthrough"]
    end
```

### 2.1. Sockets Used by FIFA 22
1. **Gameplay UDP Socket (`CommUDP`)**:
   - **Local Port**: Typically binds to UDP `3659` (with fallback to `3658`–`3665` if occupied).
   - **Protocol**: Unreliable/Reliable UDP multiplexed by DirtySDK.
   - **Characteristics**: High packet frequency (60 Hz or 30 Hz tick rate matching the Frostbite physics frame rate).
   - **Payload**: Contains player controller inputs, sequence numbers, clock synchronization, and frame verification hashes.
2. **Telemetry & QoS Sockets**:
   - **TCP 443 / 80**: Used for River/Pin telemetry (`river.data.ea.com`, `pin-river.data.ea.com`) and HTTP REST APIs.
   - **UDP 17498**: EA QoS / STUN ping probing (`ProtoPing`).
3. **Blaze / Redirector Sockets**:
   - **TCP 42127 / 42128 / 42230**: Legacy EA server connection attempts.

---

## 3. Function Detours & Calling Conventions

We hook the standard WinSock 2 functions in `ws2_32.dll` using **MinHook** with strict x64 ABI compliance:

```cpp
// Calling Convention: __stdcall / WSAAPI on x64
typedef int (WSAAPI *PFN_bind)(SOCKET s, const struct sockaddr *name, int namelen);
typedef int (WSAAPI *PFN_sendto)(SOCKET s, const char *buf, int len, int flags, const struct sockaddr *to, int tolen);
typedef int (WSAAPI *PFN_recvfrom)(SOCKET s, char *buf, int len, int flags, struct sockaddr *from, int *fromlen);
typedef int (WSAAPI *PFN_WSASendTo)(SOCKET s, LPWSABUF lpBuffers, DWORD dwBufferCount, LPDWORD lpNumberOfBytesSent, DWORD dwFlags, const struct sockaddr *lpTo, int iTolen, LPWSAOVERLAPPED lpOverlapped, LPWSAOVERLAPPED_COMPLETION_ROUTINE lpCompletionRoutine);
typedef int (WSAAPI *PFN_WSARecvFrom)(SOCKET s, LPWSABUF lpBuffers, DWORD dwBufferCount, LPDWORD lpNumberOfBytesRecvd, LPDWORD lpFlags, struct sockaddr *lpFrom, LPINT lpFromlen, LPWSAOVERLAPPED lpOverlapped, LPWSAOVERLAPPED_COMPLETION_ROUTINE lpCompletionRoutine);
typedef int (WSAAPI *PFN_connect)(SOCKET s, const struct sockaddr *name, int namelen);
typedef int (WSAAPI *PFN_send)(SOCKET s, const char *buf, int len, int flags);
typedef int (WSAAPI *PFN_recv)(SOCKET s, char *buf, int len, int flags);
```

### 3.1. Stack Alignment & Precision Guidelines
- **x64 Calling Convention**: Every detour function must preserve 16-byte stack alignment. Detour functions are declared with `__declspec(noinline)`.
- **Error Code Preservation**: Winsock calls use thread-local error state. All detours must capture `int err = WSAGetLastError()` immediately on failure and invoke `WSASetLastError(err)` prior to returning to the caller.
- **Fast-Path Fallback**: If a socket is not the game's registered UDP socket, the detour executes the original function pointer with zero logging or string formatting overhead to avoid thread starvation.

---

## 4. Game Traffic vs. EA Telemetry Discrimination

To ensure the game client runs smoothly without waiting on unresponsive EA servers or hanging on blocked sockets, our filter logic operates as follows:

| Traffic Class | Identification Criteria | Handling Action |
|---|---|---|
| **Game UDP (Send)** | Socket == `g_game_socket` OR destination port == `3659` (or `3658`–`3665`) | **Reroute**: Wrap with Seamless Envelope and send to `seamless-relay` UDP endpoint. |
| **Game UDP (Recv)** | Socket == `g_game_socket` AND sender == `seamless-relay` | **Process**: Strip Seamless Envelope, verify room token, synthesize remote peer `sockaddr_in`, and pass inner payload to DirtySDK. |
| **EA Telemetry (TCP)** | Destination port 443/80/8080 OR destination host `*.data.ea.com`, `*.ea.com` | **Silent Drop**: Return `SOCKET_ERROR` with `WSASetLastError(WSAECONNREFUSED)` or simulate an instant TCP close so threads do not block. |
| **Blaze / QoS (TCP/UDP)** | Ports 42127, 42128, 42230, 17498 | **Intercept / Bypass**: Drop or acknowledge locally; game stays in offline mode. |
| **Localhost / Control** | Port 42129 (Internal overlay / IPC) | **Passthrough**: Allow standard localhost communication for in-game overlay. |

### 4.1. Code Logic for Traffic Filtering
```cpp
// Hooked_sendto logic
static __declspec(noinline) int WSAAPI Hooked_sendto(
    SOCKET s,
    const char *buf,
    int len,
    int flags,
    const struct sockaddr *to,
    int tolen
) {
    if (!to || len <= 0) {
        return g_orig_sendto(s, buf, len, flags, to, tolen);
    }

    uint16_t dst_port = 0;
    if (to->sa_family == AF_INET) {
        dst_port = ntohs(reinterpret_cast<const struct sockaddr_in*>(to)->sin_port);
    }

    // Check if this is the game's lockstep CommUDP transmission
    if (s == g_game_socket || dst_port == 3659 || (dst_port >= 3658 && dst_port <= 3665)) {
        // Encapsulate into Seamless Envelope
        return ForwardToSeamlessRelay(s, buf, len, flags);
    }

    // Block telemetry / tracking silently
    if (dst_port == 443 || dst_port == 80) {
        WSASetLastError(WSAECONNREFUSED);
        return SOCKET_ERROR;
    }

    return g_orig_sendto(s, buf, len, flags, to, tolen);
}
```

---

## 5. Packet Inspection & Session Password Architecture

A critical design requirement is: **How do we identify and match sessions without breaking DirtySDK's internal packet structure or encryption?**

### 5.1. The Zero-Payload-Modification Principle
* DirtySDK packets contain internal checksums, sequence tracking, and potential lightweight session keys negotiated between peers.
* If a proxy modifies even a single byte inside DirtySDK's game packet, DirtySDK's integrity check fails and the client immediately drops the packet, causing match desync.
* **Our Solution: Tunneling via Outer Envelope Wrapping**. We never modify DirtySDK's payload. Instead, we encapsulate the entire untouched DirtySDK packet inside our own **Seamless Packet Envelope**.

```
+-------------------------------------------------------------------------------+
|                        Seamless Envelope Header (40 Bytes)                     |
+---------------+---------------+---------------+---------------+---------------+
| Magic (4B)    | Version (1B)  | MsgType (1B)  | Reserved (2B) | RoomHash (32B)|
| "SF22"        | 0x01          | 0x03 (Data)   | 0x0000        | BLAKE3(Pass)  |
+---------------+---------------+---------------+---------------+---------------+
|                           Inner DirtySDK Payload                              |
|                       (Byte-for-byte 100% UNTOUCHED)                          |
+-------------------------------------------------------------------------------+
| CommUDP Header | NetGameDist Input Frame / Sync / Tick Data | CRC / Auth Data  |
+-------------------------------------------------------------------------------+
```

### 5.2. Session Authentication Flow
1. **Password Configuration**:
   - The user sets `session_password = "ChampionLeague2026"` in `seamless.ini` (or through the launcher UI).
2. **Room Hash Generation**:
   - The client DLL computes `RoomHash = BLAKE3("FIFA22_SEAMLESS_SALT:" + session_password)`.
3. **Transmission**:
   - When the game sends a packet, `Hooked_sendto` prepends the 40-byte Seamless Envelope to the buffer.
   - The packet is directed to the `seamless-relay` server over UDP.
4. **Relay Verification**:
   - The relay server checks `RoomHash`. If two clients send packets with the exact same `RoomHash`, they are paired into the same match room.
   - The relay routes the packet to the other paired client.
5. **Reception (`Hooked_recvfrom`)**:
   - When `seamless_fifa.dll` receives a packet on `g_game_socket` from the relay server:
     1. It validates the magic bytes (`"SF22"`) and `RoomHash`.
     2. It strips the 40-byte Seamless Envelope header.
     3. It copies the raw inner DirtySDK payload into the game's buffer.
     4. It fills `from` with the simulated virtual opponent address (e.g. `127.0.0.1:3659` or peer IP).
     5. It returns the length of the *inner* payload to DirtySDK.

Because DirtySDK sees only its exact, original packet format, **the game's encryption, lockstep timestamps, and CRC checksums remain 100% valid and unbroken.**

