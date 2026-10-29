import mmap
import os

target_file = r"G:\Games\FIFA 22\FIFA22.exe"
search_terms = [b"qosapi", b"qosip", b"qosport", b"probesize", b"numprobes", b"hostname", b"dstport", b"srcport"]

try:
    with open(target_file, "rb") as f:
        mm = mmap.mmap(f.fileno(), 0, access=mmap.ACCESS_READ)
        for term in search_terms:
            pos = mm.find(term)
            while pos != -1:
                print(f"FOUND {term.decode()}: {hex(pos)}")
                pos = mm.find(term, pos + 1)
        mm.close()
except Exception as e:
    print(f"ERROR: {e}")
