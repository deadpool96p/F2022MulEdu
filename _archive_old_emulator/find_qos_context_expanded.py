import mmap
import os

target_file = r"G:\Games\FIFA 22\FIFA22.exe"
search_terms = [b"qosapi", b"qosip", b"qosport", b"probesize", b"numprobes"]

try:
    with open(target_file, "rb") as f:
        mm = mmap.mmap(f.fileno(), 0, access=mmap.ACCESS_READ)
        
        # We'll look at a larger region around these strings to find patterns.
        # Since they are all close together (0x77946cc to 0x77947e1), 
        # let's just dump the whole area.
        start = 0x7794600
        end = 0x7794900
        context = mm[start:end]
        
        for i in range(0, len(context), 16):
            chunk = context[i:i+16]
            hex_val = " ".join(f"{b:02x}" for b in chunk)
            ascii_val = "".join(chr(b) if 32 <= b <= 126 else "." for b in chunk)
            print(f"{hex(start + i):>10} | {hex_val:<48} | {ascii_val}")
            
        mm.close()
except Exception as e:
    print(f"ERROR: {e}")
