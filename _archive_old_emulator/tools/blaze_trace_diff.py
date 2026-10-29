#!/usr/bin/env python3
"""
Blaze NDJSON Trace Diff Tool (tools/blaze_trace_diff.py)

Compares two newline-delimited JSON (NDJSON) traces produced by:
    cargo run -p emulator-server -- --trace-blaze <path>

Each record contains:
    {"direction": "in"|"out", "component": X, "command": Y, "tdf_hex": "...", "timestamp_us": Z}

Usage:
    python tools/blaze_trace_diff.py trace_a.ndjson trace_b.ndjson [--verbose]
"""

import sys
import json
import argparse
from typing import List, Dict, Any, Optional

COMPONENT_NAMES = {
    0x0001: "Authentication",
    0x0004: "GameManager",
    0x0007: "Stats",
    0x0009: "Util",
    0x000A: "CensusData",
    0x000F: "Clubs",
    0x0019: "AssociationLists",
    0x001C: "UserSessions",
}

COMMAND_NAMES = {
    (0x0001, 0x001C): "Login",
    (0x0001, 0x001D): "Logout",
    (0x0004, 0x0001): "CreateGame",
    (0x0004, 0x0002): "DestroyGame",
    (0x0004, 0x0004): "JoinGame",
    (0x0004, 0x0007): "StartMatchmaking",
    (0x0004, 0x0008): "CancelMatchmaking",
    (0x0004, 0x000A): "NotifyGameRemoved",
    (0x0004, 0x000B): "FinalizeGameCreation",
    (0x0004, 0x000C): "SetGameSettings/NotifyMatchmakingAsyncStatus",
    (0x0004, 0x000F): "NotifyGameCreated",
    (0x0004, 0x0014): "UpdatePlayerState/NotifyGameSetup",
    (0x0004, 0x0015): "NotifyPlayerJoining",
    (0x0004, 0x0016): "NotifyPlayerLeft",
    (0x0004, 0x0017): "NotifyJoiningPlayerInitiateConnections",
    (0x0004, 0x0018): "NotifyPlatformHostInitialized",
    (0x0004, 0x0019): "NotifyGameReset",
    (0x0004, 0x0020): "NotifyPlayerRemoved",
    (0x0004, 0x0050): "NotifyGameAttribChange",
    (0x0004, 0x0064): "NotifyGameStateChange",
    (0x0004, 0x006E): "NotifyGameSettingsChange",
    (0x0004, 0x0070): "NotifyGameReportingIdChange",
    (0x0004, 0x0074): "NotifyGamePlayerStateChange",
    (0x0004, 0x0075): "NotifyGamePlayerTeamRoleSlotChange",
    (0x0009, 0x0001): "FetchClientConfig",
    (0x0009, 0x0002): "Ping",
    (0x0009, 0x000B): "PreAuth",
    (0x0009, 0x000C): "PostAuth",
}

def fmt_cmd(comp: int, cmd: int) -> str:
    comp_str = COMPONENT_NAMES.get(comp, f"0x{comp:04X}")
    cmd_str = COMMAND_NAMES.get((comp, cmd), f"0x{cmd:04X}")
    return f"{comp_str}::{cmd_str} (comp=0x{comp:04X}, cmd=0x{cmd:04X})"

def load_trace(path: str) -> List[Dict[str, Any]]:
    records = []
    with open(path, "r", encoding="utf-8") as f:
        for idx, line in enumerate(f, start=1):
            line = line.strip()
            if not line:
                continue
            try:
                data = json.loads(line)
                records.append(data)
            except Exception as e:
                print(f"[-] Warning: Failed to parse JSON on line {idx} of {path}: {e}", file=sys.stderr)
    return records

def diff_traces(file_a: str, file_b: str, verbose: bool = False) -> int:
    trace_a = load_trace(file_a)
    trace_b = load_trace(file_b)

    print(f"[*] Loaded {len(trace_a)} records from '{file_a}'")
    print(f"[*] Loaded {len(trace_b)} records from '{file_b}'")
    print("-" * 70)

    min_len = min(len(trace_a), len(trace_b))

    for i in range(min_len):
        rec_a = trace_a[i]
        rec_b = trace_b[i]

        dir_a = rec_a.get("direction", "").lower()
        dir_b = rec_b.get("direction", "").lower()
        comp_a = rec_a.get("component", 0)
        comp_b = rec_b.get("component", 0)
        cmd_a = rec_a.get("command", 0)
        cmd_b = rec_b.get("command", 0)
        hex_a = rec_a.get("tdf_hex", "")
        hex_b = rec_b.get("tdf_hex", "")

        matches = (dir_a == dir_b) and (comp_a == comp_b) and (cmd_a == cmd_b) and (hex_a == hex_b)

        if matches:
            if verbose:
                print(f"[# {i:04d}] MATCH: {dir_a.upper():3s} {fmt_cmd(comp_a, cmd_a)} (bytes: {len(hex_a)//2})")
            continue

        # Divergence Detected!
        print(f"[!] FIRST DIVERGENCE AT MESSAGE INDEX #{i}:")
        print(f"    Trace A: {dir_a.upper():3s} {fmt_cmd(comp_a, cmd_a)}")
        print(f"    Trace B: {dir_b.upper():3s} {fmt_cmd(comp_b, cmd_b)}")

        if dir_a != dir_b:
            print(f"    -> Direction Mismatch: A is '{dir_a}', but B is '{dir_b}'")
        elif comp_a != comp_b or cmd_a != cmd_b:
            print(f"    -> RPC Target Mismatch: A invoked ({comp_a},{cmd_a}), B invoked ({comp_b},{cmd_b})")
        elif hex_a != hex_b:
            print(f"    -> TDF Payload Mismatch:")
            print(f"       Trace A ({len(hex_a)//2} bytes): {hex_a[:64]}{'...' if len(hex_a) > 64 else ''}")
            print(f"       Trace B ({len(hex_b)//2} bytes): {hex_b[:64]}{'...' if len(hex_b) > 64 else ''}")
            # Find first hex difference
            for char_idx in range(min(len(hex_a), len(hex_b))):
                if hex_a[char_idx] != hex_b[char_idx]:
                    byte_offset = char_idx // 2
                    print(f"       First byte mismatch at offset 0x{byte_offset:04X} ({byte_offset} bytes in payload)")
                    break

        print("-" * 70)
        print("[!] Traces diverge. Stopping comparison.")
        return 1

    if len(trace_a) != len(trace_b):
        longer_name = file_a if len(trace_a) > len(trace_b) else file_b
        shorter_name = file_b if len(trace_a) > len(trace_b) else file_a
        diff_count = abs(len(trace_a) - len(trace_b))
        print(f"[!] TRACE LENGTH MISMATCH: First {min_len} messages matched identically,")
        print(f"    but '{longer_name}' contains {diff_count} additional trailing messages not present in '{shorter_name}'.")
        print("-" * 70)
        return 1

    print(f"[+] SUCCESS: Traces match identically ({min_len} messages verified with 0 divergences).")
    print("-" * 70)
    return 0

def main():
    parser = argparse.ArgumentParser(description="Blaze NDJSON Trace Diffing Tool")
    parser.add_argument("trace_a", help="Path to first trace (.ndjson)")
    parser.add_argument("trace_b", help="Path to second trace (.ndjson)")
    parser.add_argument("-v", "--verbose", action="store_true", help="Print matching messages prior to divergence")

    args = parser.parse_args()
    sys.exit(diff_traces(args.trace_a, args.trace_b, args.verbose))

if __name__ == "__main__":
    main()
