import mmap
import os

target_file = r"G:\Games\FIFA 22\FIFA22.exe"
# Look for strings that are likely used as XML tags for the qosapi service.
# The strings we found: .qosip, .qosport, .probesize, .numprobes
# These look like they might be used as tags or keys in a map.
# Notice the leading dot: ".qosip", ".qosport", etc.
# This often implies they are members of a structure or keys in a lookup.

search_terms = [b".qosip", b".qosport", b".probesize", b".numprobes"]

try:
    with open(target_file, "rb") as f:
        mm = mmap.mmap(f.fileno(), 0, access=mmap.ACCESS_READ)
        for term in search_terms:
            pos = mm.find(term)
            while pos != -1:
                # Search for any strings that look like XML tags in the vicinity
                start = max(0, pos - 2048)
                end = min(mm.size(), pos + 2048)
                chunk = mm[start:end]
                
                # Look for < and > characters
                if b"<" in chunk and b">" in chunk:
                    print(f"Found XML-like context around {term.decode()} at {hex(pos)}:")
                    print(chunk.decode('ascii', errors='replace'))
                    print("-" * 40)
                
                pos = mm.find(term, pos + 1)
        mm.close()
except Exception as e:
    print(f"ERROR: {e}")
