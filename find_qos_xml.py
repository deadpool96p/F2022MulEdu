import mmap
import os

target_file = r"G:\Games\FIFA 22\FIFA22.exe"
search_terms = [b"<", b">", b"qos", b"service", b"hostname", b"port"]

try:
    with open(target_file, "rb") as f:
        mm = mmap.mmap(f.fileno(), 0, access=mmap.ACCESS_READ)
        
        # Search for "qos" and then look for nearby XML-like tags.
        pos = mm.find(b"qos")
        while pos != -1:
            # Dump context around the "qos" string
            start = max(0, pos - 512)
            end = min(mm.size(), pos + 512)
            chunk = mm[start:end]
            
            # If we find something that looks like XML (e.g., <something>), print it.
            if b" <" in chunk or b" <" in chunk.lower():
                print(f"POTENTIAL XML AT {hex(pos)}:")
                print(chunk.decode('ascii', errors='replace'))
                print("-" * 40)
            
            pos = mm.find(b"qos", pos + 1)
        mm.close()
except Exception as e:
    print(f"ERROR: {e}")
