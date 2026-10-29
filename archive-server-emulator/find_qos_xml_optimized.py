import mmap
import os
import sys

target_file = r"G:\Games\FIFA 22\FIFA22.exe"

try:
    with open(target_file, "rb") as f:
        mm = mmap.mmap(f.fileno(), 0, access=mmap.ACCESS_READ)
        
        # Find all "qos" strings and look for < and > around them
        pos = mm.find(b"qos")
        while pos != -1:
            start = max(0, pos - 1024)
            end = min(mm.size(), pos + 1024)
            chunk = mm[start:end]
            
            # Look for anything that looks like <tag>
            if b"<" in chunk and b">" in chunk:
                # Find all <...> patterns in the chunk
                import re
                tags = re.findall(b"<[^>]+>", chunk)
                for tag in tags:
                    sys.stdout.buffer.write(f"Found tag {tag} near {hex(pos)}\n".encode())
            
            pos = mm.find(b"qos", pos + 1)
        mm.close()
except Exception as e:
    print(f"ERROR: {e}")
