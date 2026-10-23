import mmap
import os
import sys

target_file = r"G:\Games\FIFA 22\FIFA22.exe"
start = 0x79eb000
end = 0x79ebc00

try:
    with open(target_file, "rb") as f:
        mm = mmap.mmap(f.fileno(), 0, access=mmap.ACCESS_READ)
        chunk = mm[start:end]
        
        # Search for < and >
        pos = 0
        while True:
            pos = chunk.find(b"<", pos)
            if pos == -1: break
            
            end_tag = chunk.find(b">", pos)
            if end_tag != -1:
                tag = chunk[pos:end_tag+1]
                sys.stdout.buffer.write(f"Found tag {tag} at {hex(start + pos)}\n".encode())
            pos += 1
        mm.close()
except Exception as e:
    print(f"ERROR: {e}")
