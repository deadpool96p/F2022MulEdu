"""Hex dump a virtual-address range of a PE image.

Usage: python tools/hexdump.py <pe> <start_va_hex> <size_bytes>
"""

import sys

sys.path.insert(0, __file__.rsplit("\\", 1)[0].rsplit("/", 1)[0] + "/tools")
from disasm_region import load_pe, make_va_to_off  # noqa: E402


def main(argv):
    pe = argv[1]
    start = int(argv[2], 16)
    size = int(argv[3], 0)

    d, base, sections = load_pe(pe)
    va_to_off = make_va_to_off(sections, base)
    off = va_to_off(start)
    if off is None:
        print("VA 0x%X not mapped" % start)
        return 1

    blob = d[off:off + size]
    for i in range(0, len(blob), 16):
        row = blob[i:i + 16]
        hexs = " ".join("%02x" % b for b in row)
        text = "".join(chr(b) if 32 <= b < 127 else "." for b in row)
        print("%016X  %-47s  %s" % (start + i, hexs, text))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
