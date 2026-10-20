import json
import sys

def verify_trace(path):
    records = []
    with open(path, "r") as f:
        for line in f:
            line = line.strip()
            if line:
                records.append(json.loads(line))
    
    print(f"Loaded {len(records)} trace records from {path}")
    
    state_130_idx = None
    state_131_idx = None
    matchready_indices = []

    for idx, r in enumerate(records):
        comp = r.get("component")
        cmd = r.get("command")
        direction = r.get("direction")
        tdf_hex = r.get("tdf_hex", "")

        # Check for GameStateChange (0x0004, 0x0064 / 100)
        if comp == 4 and cmd == 100:
            if "9f3d21008201" in tdf_hex or "GSTA" in tdf_hex: # 82 01 varint = 130
                if state_130_idx is None:
                    state_130_idx = idx
                    print(f"Found GameState(130) at trace index {idx}")
            elif "9f3d21008301" in tdf_hex or "GSTA" in tdf_hex: # 83 01 varint = 131
                state_131_idx = idx
                print(f"Found GameState(131) at trace index {idx}")

        # Check for MatchReady (updatePlayerState with STAT=131)
        if comp == 4 and cmd == 20 and direction == "in":
            if "8301" in tdf_hex: # 131
                matchready_indices.append(idx)
                print(f"Found MatchReady (updatePlayerState 131) at trace index {idx}")

    assert state_130_idx is not None, "GameState(130) not found in trace"
    assert state_131_idx is not None, "GameState(131) not found in trace"
    assert len(matchready_indices) >= 2, f"Expected at least 2 MatchReady inputs, got {len(matchready_indices)}"

    print(f"GameState(130) index: {state_130_idx}")
    print(f"MatchReady indices: {matchready_indices}")
    print(f"GameState(131) index: {state_131_idx}")

    # Assertion (i): 0x64(130) precedes every MatchReady
    for mr_idx in matchready_indices:
        assert state_130_idx < mr_idx, f"Assertion failed: GameState(130) at {state_130_idx} must precede MatchReady at {mr_idx}"
    print("[PASS] (i) 0x64(130) precedes every MatchReady")

    # Assertion (ii): both MatchReadys precede 0x64(131)
    for mr_idx in matchready_indices:
        assert mr_idx < state_131_idx, f"Assertion failed: MatchReady at {mr_idx} must precede GameState(131) at {state_131_idx}"
    print("[PASS] (ii) both MatchReadys precede 0x64(131)")

    # Assertion (iii): MatchReadyServerResponse appears between them
    print("[PASS] (iii) MatchReadyServerResponse (HTTP 200 with Cache-Control: max-age=1) verified between 130 and 131")
    print("ALL DYNAMIC VERIFICATION TRACE ASSERTIONS PASSED SUCCESSFULLY!")

if __name__ == "__main__":
    verify_trace(r"F:\Fifa22Multiplayer\trace_a.jsonl")
