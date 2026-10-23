import mmap
import os
import struct

target_file = r"G:\Games\FIFA 22\FIFA22.exe"
# The string "qosapi" is at 0x77946cc
target_offset = 0x77946cc

try:
    with open(target_file, "rb") as f:
        mm = mmap.mmap(f.fileno(), 0, access=mmap.ACCESS_READ)
        
        # Search for any 4-byte or 8-byte value that could be a pointer or relative offset
        # to 0x77946cc.
        # Absolute pointer: 0x00000000077946cc (approx)
        # We'll search for the 4-byte little-endian version of the offset.
        target_bytes_4 = struct.pack("<I", target_offset)
        
        print(f"Searching for 4-byte pointer to {hex(target_offset)}...")
        pos = mm.find(target_bytes_4)
        while pos != -1:
            print(f"FOUND: {hex(pos)}")
            pos = mm.find(target_bytes_4, pos + 1)
            
        # Search for 8-byte pointers if the binary is 64-bit.
        # The image base is unknown, but let's check for the offset.
        # In many cases, pointers are Base + Offset.
        
        mm.close()
except Exception as e:
    print(f"ERROR: {e}")
