import mmap
import os

target_file = r"G:\Games\FIFA 22\FIFA22.exe"
terms = [b"qosip", b"qosport", b"probesize", b"numprobes"]

try:
    with open(target_file, "rb") as f:
        mm = mmap.mmap(f.fileno(), 0, access=mmap.ACCESS_READ)
        for term in terms:
            pos = mm.find(term)
            while pos != -1:
                # Check if it's preceded by a dot
                if pos > 0 and mm[pos-1] == ord('.'):
                    print(f"Found . {term.decode()} at {hex(pos)}")
                else:
                    print(f"Found   {term.decode()} at {hex(pos)}")
                pos = mm.find(term, pos + 1)
        mm.close()
except Exception as e:
    print(f"ERROR: {e}")
