import mmap
import os

target_file = r"G:\Games\FIFA 22\FIFA22.exe"
offset = 0x79eb5c0

try:
    with open(target_file, "rb") as f:
        mm = mmap.mmap(f.fileno(), 0, access=mmap.ACCESS_READ)
        
        start = max(0, offset - 1024)
        end = min(mm.size(), offset + 1024)
        context = mm[start:end]
        
        for i in range(0, len(context), 16):
            chunk = context[i:i+16]
            hex_val = " ".join(f"{b:02x}" for b in chunk)
            ascii_val = "".join(chr(b) if 32 <= b <= 126 else "." for b in chunk)
            print(f"{hex(start + i):>10} | {hex_val:<48} | {ascii_val}")
            
        mm.close()
except Exception as e:
    print(f"ERROR: {e}")
