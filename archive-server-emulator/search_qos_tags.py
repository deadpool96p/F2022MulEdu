import mmap
import os

target_file = r"G:\Games\FIFA 22\FIFA22.exe"
# The strings .qosip, .qosport, .probesize, .numprobes strongly suggest a structure.
# Let's search for any strings that look like they are being used as XML tags 
# in the same region as these.
# Specifically, look for strings that start with < and end with >.

try:
    with open(target_file, "rb") as f:
        mm = mmap.mmap(f.fileno(), 0, access=mmap.ACCESS_READ)
        
        # Search for < as a starting character of a string
        pos = 0
        while True:
            pos = mm.find(b"<", pos)
            if pos == -1: break
            
            # Find the end of the string
            end = pos
            while end < mm.size() and mm[end] != 0:
                end += 1
            
            s = mm[pos:end]
            if s.endswith(b">"):
                print(f"{hex(pos)}: {s.decode('ascii', errors='replace')}")
            
            pos = end + 1
        mm.close()
except Exception as e:
    print(f"ERROR: {e}")
