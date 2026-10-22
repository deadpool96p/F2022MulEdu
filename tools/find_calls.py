"""Find direct `call rel32` instructions that target a given VA.

`target = (address of the rel32 field) + 4 + disp32`, and for a call the
disp32 field ends at insn+5, so the field address is insn+1.

Usage: python tools/find_calls.py <pe> <target_va_hex> [<target_va_hex> ...]
"""
import struct
import sys

sys.path.insert(0, __file__.rsplit("\\", 1)[0].rsplit("/", 1)[0] + "/tools")
from disasm_region import load_pe  # noqa: E402


def main(argv):
    pe = argv[1]
    targets = {int(a, 16) for a in argv[2:]}
    d, base, sections = load_pe(pe)
    hits = {t: [] for t in targets}
    for name, vaddr, vsize, rawptr, rawsize in sections:
        span = min(vsize, rawsize)
        blob = d[rawptr:rawptr + span]
        start = 0
        while True:
            i = blob.find(b"\xe8", start)
            if i < 0 or i + 5 > len(blob):
                break
            disp = struct.unpack_from("<i", blob, i + 1)[0]
            field_va = base + vaddr + i + 1
            t = field_va + 4 + disp
            if t in hits:
                hits[t].append(field_va - 1)
            start = i + 1
    for t in sorted(targets):
        print("target %#x: %d call site(s)" % (t, len(hits[t])))
        for h in hits[t]:
            print("   call at %#x" % h)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
