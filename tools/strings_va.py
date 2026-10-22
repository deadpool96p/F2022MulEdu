"""Find ASCII string literals in a PE and print their virtual addresses.

Usage: python tools/strings_va.py <pe> <needle> [min_va] [max_va]
"""
import sys
sys.path.insert(0, __file__.rsplit("\\", 1)[0].rsplit("/", 1)[0] + "/tools")
from disasm_region import load_pe  # noqa: E402


def main(argv):
    pe = argv[1]
    needle = argv[2].encode()
    lo = int(argv[3], 16) if len(argv) > 3 else 0
    hi = int(argv[4], 16) if len(argv) > 4 else 1 << 64
    d, base, sections = load_pe(pe)
    for name, vaddr, vsize, rawptr, rawsize in sections:
        span = min(vsize, rawsize)
        blob = d[rawptr:rawptr + span]
        start = 0
        while True:
            i = blob.find(needle, start)
            if i < 0:
                break
            # must be preceded/followed by a non-alnum to be a standalone literal
            before = blob[i - 1:i]
            after = blob[i + len(needle):i + len(needle) + 1]
            ok_before = not before or not (before.isalnum() or before in b"_.")
            ok_after = not after or not (after.isalnum() or after in b"_.")
            va = base + vaddr + i
            if ok_before and ok_after and lo <= va <= hi:
                end = blob.find(b"\x00", i)
                print(f"{va:#x}  {blob[i:end].decode('latin1')!r}  ({name})")
            start = i + 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
