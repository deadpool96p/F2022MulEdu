"""Find RIP-relative references to several VAs in one pass over a PE.

`target = (end of disp32 field) + disp32`, and the end of the field does not
depend on the instruction length, so scanning every 4-byte window of every
section finds every reference regardless of alignment or instruction size.

Usage: python tools/find_xrefs_multi.py <pe> <target_va_hex> [<target_va_hex> ...]
       [--exclude LO,HI ...]

`--exclude` drops references that land inside the given VA ranges; the schema
registration block in `.ecode` is the usual thing to hide, because a member
name's *registration* site is not the *lookup* site we are hunting for.
"""
import struct
import sys

sys.path.insert(0, __file__.rsplit("\\", 1)[0].rsplit("/", 1)[0] + "/tools")
from disasm_region import load_pe  # noqa: E402


def main(argv):
    pe = argv[1]
    rest = argv[2:]
    excluded = []
    while "--exclude" in rest:
        i = rest.index("--exclude")
        lo, hi = rest[i + 1].split(",")
        excluded.append((int(lo, 16), int(hi, 16)))
        del rest[i:i + 2]
    targets = {int(a, 16) for a in rest}
    d, base, sections = load_pe(pe)

    hits = {t: [] for t in targets}
    for name, vaddr, vsize, rawptr, rawsize in sections:
        span = min(vsize, rawsize)
        for k in range(0, span - 4):
            off = rawptr + k
            field_va = base + vaddr + k
            disp = struct.unpack_from("<i", d, off)[0]
            t = field_va + 4 + disp
            if t in hits:
                hits[t].append(field_va)

    for t in sorted(targets):
        keep = [h for h in hits[t]
                if not any(lo <= h <= hi for lo, hi in excluded)]
        print("target %#x: %d reference(s), %d after exclusion" % (t, len(hits[t]), len(keep)))
        for h in keep:
            print("   field at %#x" % h)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
