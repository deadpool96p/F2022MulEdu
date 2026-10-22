"""Find references to a string inside a PE image (absolute VA, RVA, or VA-base).

Usage: python tools/find_refs.py <pe> <string> [--va <hex>]
"""

import struct
import sys

sys.path.insert(0, __file__.rsplit("\\", 1)[0].rsplit("/", 1)[0] + "/tools")
from disasm_region import load_pe  # noqa: E402


def main(argv):
    pe = argv[1]
    needle = argv[2].encode("latin1")
    d, base, sections = load_pe(pe)

    off = d.find(needle)
    if off == -1:
        print("string not found")
        return 1
    va = None
    for name, vaddr, vsize, rawptr, rawsize in sections:
        if rawptr <= off < rawptr + rawsize:
            va = base + vaddr + (off - rawptr)
            break
    print("string at file 0x%X va %s" % (off, hex(va) if va else "none"))
    if va is None:
        return 1

    for label, pat in (
        ("abs64", struct.pack("<Q", va)),
        ("rva32", struct.pack("<I", va - base)),
        ("low32", struct.pack("<I", va & 0xFFFFFFFF)),
    ):
        hits = []
        start = 0
        while True:
            pos = d.find(pat, start)
            if pos == -1:
                break
            hits.append(pos)
            start = pos + 1
            if len(hits) > 30:
                break
        print("%s: %d hit(s) %s" % (label, len(hits), [hex(h) for h in hits[:30]]))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
