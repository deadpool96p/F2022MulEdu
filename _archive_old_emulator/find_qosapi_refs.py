import mmap
import struct
import os

target_file = r"G:\Games\FIFA 22\FIFA22.exe"
search_str = b"qosapi"
# The offset we found earlier was 0x77946cc
# We want to find where in the binary this offset (or the address it represents) is referenced.
# This is hard without a proper disassembler because of RIP-relative addressing in x64.
# However, we can search for the byte pattern that would be used to reference this address.
# Relative offset = Target - (Instruction_Address + 4)
# Since we don't know the Instruction_Address, we can't easily calculate the relative offset.
# BUT, we can search for the absolute address if it's stored in a table, or search for common patterns.

# Let's try searching for the 4-byte little-endian value of the offset (if it's used as a relative offset).
# Actually, a better way is to just search for the string "qosapi" and then look around it for pointers.
# Or, we can search for the value 0x77946cc in the binary to see if there are absolute pointers to it.

target_offset = 0x77946cc
target_bytes = struct.pack("<I", target_offset)

try:
    with open(target_file, "rb") as f:
        mm = mmap.mmap(f.fileno(), 0, access=mmap.ACCESS_READ)
        
        # Find all occurrences of the absolute offset
        print(f"Searching for absolute offset {hex(target_offset)}...")
        pos = mm.find(target_bytes)
        while pos != -1:
            print(f"FOUND ABSOLUTE REF: {hex(pos)}")
            pos = mm.find(target_bytes, pos + 1)
            
        mm.close()
except Exception as e:
    print(f"ERROR: {e}")
