import mmap
import os

target_file = r"G:\Games\FIFA 22\FIFA22.exe"
# Search for any string containing 'qos' and print it.
try:
    with open(target_file, "rb") as f:
        mm = mmap.mmap(f.fileno(), 0, access=mmap.ACCESS_READ)
        
        # Iterate through the binary and find all 'qos' occurrences.
        # To avoid too much output, we'll print the strings.
        pos = mm.find(b"qos")
        while pos != -1:
            # Find the end of the string (null terminator)
            end = pos
            while end < mm.size() and mm[end] != 0:
                end += 1
            
            print(f"{hex(pos)}: {mm[pos:end].decode('ascii', errors='replace')}")
            pos = mm.find(b"qos", end)
        mm.close()
except Exception as e:
    print(f"ERROR: {e}")
