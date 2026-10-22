"""Find every occurrence of an absolute 64-bit pointer in a PE image.

Usage: python tools/find_ptr.py <pe> <pointer_hex> [--va]

`--va` prints the virtual address of each hit instead of the file offset.
"""

import struct
import sys

sys.path.insert(0, __file__.rsplit("\\", 1)[0].rsplit("/", 1)[0] + "/tools")
from disasm_region import load_pe  # noqa: E402


def main(argv):
    pe = argv[1]
    needle = int(argv[2], 16)
    want_va = "--va" in argv

    d, base, sections = load_pe(pe)
    pattern = struct.pack("<Q", needle)

    start = 0
    while True:
        pos = d.find(pattern, start)
        if pos == -1:
            break
        if want_va:
            for name, vaddr, vsize, rawptr, rawsize in sections:
                if rawptr <= pos < rawptr + rawsize:
                    va = base + vaddr + (pos - rawptr)
                    print("file 0x%X -> va 0x%X (section %s)" % (pos, va, name))
                    break
            else:
                print("file 0x%X (not mapped)" % pos)
        else:
            print("file 0x%X" % pos)
        start = pos + 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
