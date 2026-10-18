# Ghidra / Deep Analysis Notes — FIFA 22 (Educational / Read-Only)

## Status (2026-10-18)
- Ghidra 12.1.4 PUBLIC installed at `C:\Tools\ghidra_12.1.4_PUBLIC`.
- Environment variable `GHIDRA_INSTALL_DIR` set to `C:\Tools\ghidra_12.1.4_PUBLIC` (User & Process).
- JDK 21.0.4 64-bit detected at `C:\Program Files\Java\jdk-21`.
- REA `doctor` verification: Ghidra status is **healthy** (platform: win32 x64, headless: analyzeHeadless.bat, native decompiler: verified).
- `open_binary` ready for read-only educational exploration.
- Binary untouched at `G:\Games\FIFA 22\FIFA22.exe` (592,922,624 bytes).

## Planned Targets (read-only, per SKILL.md / plan)
- `.rdata`: DirtySDK CA array ("DigiCert Global Root CA", RSA 2048, modulus `+0x50`).
- `.text`: `ProtoSSL` / `CertVerify`; Blaze tag encoder (`encode_tdf_tag`, 6-bit × 4 chars).
- `sysdll_Win64_retail.dll` / `EAWebKit.dll` / `Core/Activation64.dll` (unencrypted aux).
- Memory observation (if authorized): `ATTR_SEED`, squad hash region, `connect()` to `go.redirector.ea.com:42230`.

## Safety (hard)
- No `VirtualProtect`; no `.rdata`/`.text` write; no `FIFA22.exe` disk modification.
- Evidence only (`parent_evidence_id` required per `SKILL.md`).
- Distinguish observation / inference / unknown per SGHA / REA AGENTS.md.

## Next Step (pending provider)
Once `GHIDRA_INSTALL_DIR` set and `doctor:healthy`, call `open_binary` → `binary_overview` → `analyze_function` on DirtySDK TLS section.
