import mmap
import os
import sys

target_file = r"G:\Games\FIFA 22\FIFA22.exe"

try:
    with open(target_file, "rb") as f:
        mm = mmap.mmap(f.fileno(), 0, access=mmap.ACCESS_READ)
        
        pos = 0
        while True:
            pos = mm.find(b"<", pos)
            if pos == -1: break
            
            end = pos
            while end < mm.size() and mm[end] != 0:
                end += 1
            
            s = mm[pos:end]
            if s.endswith(b">"):
                # Use sys.stdout.buffer.write to avoid encoding issues
                sys.stdout.buffer.write(f"{hex(pos)}: ".encode() + s + b"\n")
            
            pos = end + 1
        mm.close()
except Exception as e:
    print(f"ERROR: {e}")
