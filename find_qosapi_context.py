import mmap
import os

target_file = r"G:\Games\FIFA 22\FIFA22.exe"
search_str = b"qosapi"
offset = 0x77946cc

try:
    with open(target_file, "rb") as f:
        mm = mmap.mmap(f.fileno(), 0, access=mmap.ACCESS_READ)
        
        # Read some bytes before and after the string to see if there are nearby strings
        # that look like XML tags.
        start = max(0, offset - 1024)
        end = min(mm.size(), offset + 2048)
        context = mm[start:end]
        
        # Print hex and ascii for better visibility
        for i in range(0, len(context), 16):
            chunk = context[i:i+16]
            hex_val = " ".join(f"{b:02x}" for b in chunk)
            ascii_val = "".join(chr(b) if 32 <= b <= 126 else "." for b in chunk)
            print(f"{hex(start + i):>10} | {hex_val:<48} | {ascii_val}")
            
        mm.close()
except Exception as e:
    print(f"ERROR: {e}")
