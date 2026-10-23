import mmap
import os

target_file = r"G:\Games\FIFA 22\FIFA22.exe"
search_str = b"qosapi"

try:
    with open(target_file, "rb") as f:
        mm = mmap.mmap(f.fileno(), 0, access=mmap.ACCESS_READ)
        offset = mm.find(search_str)
        if offset != -1:
            print(f"FOUND: {hex(offset)}")
        else:
            print("NOT FOUND")
        mm.close()
except Exception as e:
    print(f"ERROR: {e}")
