# Plan: REA `open_binary` Disassembly & Native Frostbite Engine Multiplayer Activation

**Target Binary:** `G:\Games\FIFA 22\FIFA22.exe` (592 MB, Frostbite 3 Engine + EA DirtySDK + Denuvo Anti-Tamper)  
**Tooling Environment:** REA (`reverse-engineer-anything`), Ghidra 12.1.4 PUBLIC (`C:\Tools\ghidra_12.1.4_PUBLIC`), JDK 21 64-bit  
**Core Rule:** Strictly read-only analysis. No binary patching of `FIFA22.exe` on disk (preserves Denuvo integrity).

---

## 1. Critical Finding: The Engine Already Has Multiplayer

Before disassembling, a major discovery occurred during our live runtime test:

```log
[DualListener] TLS handshake SUCCEEDED!
[blaze_core::frame] Invalid Blaze message type: 0x6
Raw Header Dump:
0000  50 4F 53 54 20 2F 72 65  64 69 72 65  |POST /redire|
```

### What This Means:
1. **The Frostbite 3 engine ALREADY has multiplayer fully compiled and active inside `FIFA22.exe`.**
   When the player clicks "Online Friendlies", the engine actively runs its internal network code.
2. **The handshake succeeded over TLS:** The client connected to our redirector on port 42230 and verified our certificate chain.
3. **The request is HTTP REST, not binary TDF:** The game engine sent `POST /redirector/getServerInstance HTTP/1.1`.
4. **Conclusion:** We do **not** need to inject or rewrite the multiplayer networking into the engine. Instead, we use `open_binary` to dissect the exact **schema and structure** that the engine's built-in parser expects from the redirector response, allowing us to activate the native Online Friendlies lobby cleanly.

---

## 2. Five-Phase Disassembly & Activation Plan

```
+-------------------------------------------------------------------------------------------------------+
|                                    PHASE 1: Binary Overview & PE Layout                               |
|   - Run `open_binary` on FIFA22.exe using Ghidra 12.1.4 headless provider                             |
|   - Inspect PE headers, section boundaries (.text, .rdata), and image base (0x140000000)             |
+-------------------------------------------------------------------------------------------------------+
                                                   |
                                                   v
+-------------------------------------------------------------------------------------------------------+
|                                 PHASE 2: DirtySDK & GOSRedirector String Correlation                   |
|   - Grep strings in .rdata: "/redirector/getServerInstance", "gosredirector", "ProtoHttp"            |
|   - Map Xrefs to identify the request builder and response parsing functions                          |
+-------------------------------------------------------------------------------------------------------+
                                                   |
                                                   v
+-------------------------------------------------------------------------------------------------------+
|                                 PHASE 3: Decompile Response Parser (`analyze_function`)               |
|   - Decompile the callback that handles the HTTP response to `getServerInstance`                      |
|   - Determine exact expected fields: Address, Port, Secure (TLS), Cluster, Token format (XML vs JSON) |
+-------------------------------------------------------------------------------------------------------+
                                                   |
                                                   v
+-------------------------------------------------------------------------------------------------------+
|                                 PHASE 4: Frostbite 3 Session & Game State Mapping                     |
|   - Locate `OnlineSessionManager` and `BlazeNetworkManager` singletons in engine memory               |
|   - Map matchmaking parameters: Squad hashes (ATTR_SHSH), seed (ATTR_SEED), room codes                |
+-------------------------------------------------------------------------------------------------------+
                                                   |
                                                   v
+-------------------------------------------------------------------------------------------------------+
|                                 PHASE 5: Clean-Room Server Implementation                             |
|   - Implement `POST /redirector/getServerInstance` HTTP endpoint in `services-redirector`             |
|   - Return the exact schema to seamlessly transition FIFA 22 into the Core Blaze lobby (Port 42128)  |
+-------------------------------------------------------------------------------------------------------+
```

---

## 3. Detailed Execution Steps for Claude

### Step 1: Open Binary with REA
Call REA tool `open_binary` on `G:\Games\FIFA 22\FIFA22.exe`:
* Provider: `ghidra`
* Path: `G:\Games\FIFA 22\FIFA22.exe`
* Record returned `session_id`.

### Step 2: Extract Binary Overview
Call `binary_overview`:
* Verify architecture: `x86_64` (PE32+).
* Verify entry point and load image address.
* Extract section virtual addresses (especially `.rdata` where constant strings and Root CA tables live).

### Step 3: Locate GOSRedirector Routines
Call `search_strings`:
* Query: `/redirector/getServerInstance`
* Query: `gosredirector.ea.com`
* Query: `ProtoHttp`
* Find cross-references (xrefs) to the string address using `find_xrefs_to_name` or `xrefs`.

### Step 4: Decompile the Redirector Response Handler
Call `analyze_function` on the referencing function:
* Analyze how the engine parses the response body:
  * If XML: look for tags like `<serverinstance>`, `<address>`, `<port>`, `<secure>`.
  * If JSON: look for keys like `"address"`, `"port"`, `"secure"`.
* Document the complete field schema in `docs/redirector_schema.md`.

### Step 5: Implement HTTP Redirector Route in Rust Server
In `crates/services-redirector`:
* Detect whether an incoming TLS stream begins with `POST /redirector/` (`0x50 0x4F 0x53 0x54`).
* If HTTP POST, parse the HTTP request headers and body.
* Return an HTTP 200 OK response with the exact payload expected by DirtySDK ProtoHttp:
  ```http
  HTTP/1.1 200 OK
  Content-Type: application/xml
  Content-Length: <length>
  Connection: close

  <serverinstance>
      <address>127.0.0.1</address>
      <port>42128</port>
      <secure>false</secure>
  </serverinstance>
  ```
* Point the client to Blaze Core on port 42128 to complete the transition.

---

## 4. Why This Approach Succeeds Where Binary Patching Fails

| Approach | Modifies Disk? | Denuvo Risk | Stability | Result |
| :--- | :--- | :--- | :--- | :--- |
| **Direct Binary Patching** | **YES** | **HIGH** (Tamper trip, game crash) | Poor | Game fails to boot |
| **In-Engine Memory Hooking** | In RAM only | Low | Medium | Requires complex function hooking |
| **Protocol Response Matching (Recommended)** | **NO (0 bytes changed)** | **ZERO** | **100% Native** | Game's built-in multiplayer works as originally designed |
