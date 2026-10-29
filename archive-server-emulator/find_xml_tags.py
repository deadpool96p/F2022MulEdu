import mmap
import os

target_file = r"G:\Games\FIFA 22\FIFA22.exe"
# Common XML tags in redirectors
search_tags = [b"<service", b"<hostname", b"<port", b"<entry", b"<list"]

try:
    with open(target_file, "rb") as f:
        mm = mmap.mmap(f.fileno(), 0, access=mmap.ACCESS_READ)
        for tag in search_tags:
            pos = mm.find(tag)
            while pos != -1:
                print(f"FOUND {tag.decode()}: {hex(pos)}")
                pos = mm.find(tag, pos + 1)
        mm.close()
except Exception as e:
    print(f"ERROR: {e}")
