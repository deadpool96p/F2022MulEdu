import mmap
import os
import sys

target_file = r"G:\Games\FIFA 22\FIFA22.exe"
search_terms = [b"service", b"entry", b"list", b"hostname", b"srcport", b"dstport", b"port"]

try:
    with open(target_file, "rb") as f:
        mm = mmap.mmap(f.fileno(), 0, access=mmap.ACCESS_READ)
        for term in search_terms:
            pos = mm.find(term)
            while pos != -1:
                end = pos
                while end < mm.size() and mm[end] != 0:
                    end += 1
                s = mm[pos:end]
                sys.stdout.buffer.write(f"{hex(pos)}: {s}\n".encode())
                pos = mm.find(term, end)
        mm.close()
except Exception as e:
    print(f"ERROR: {e}")
