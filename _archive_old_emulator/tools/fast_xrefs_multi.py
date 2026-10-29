"""Multi-target RIP-relative + absolute-pointer xref finder (fast).

Usage: python tools/fast_xrefs_multi.py <pe> <target_va_hex> [target_va_hex ...]
"""
import re
import struct
import sys

sys.path.insert(0, __file__.rsplit("\\", 1)[0].rsplit("/", 1)[0] + "/tools")
from disasm_region import load_pe  # noqa: E402

PREFIXES = [
    ("lea", re.compile(rb"[\x48\x4c]\x8d[\x05\x0d\x15\x1d\x25\x2d\x35\x3d](....)", re.S), 3),
    ("movld", re.compile(rb"[\x48\x4c]\x8b[\x05\x0d\x15\x1d\x25\x2d\x35\x3d](....)", re.S), 3),
    ("movst", re.compile(rb"[\x48\x4c]\x89[\x05\x0d\x15\x1d\x25\x2d\x35\x3d](....)", re.S), 3),
    ("cmp", re.compile(rb"[\x48\x4c]\x3b[\x05\x0d\x15\x1d\x25\x2d\x35\x3d](....)", re.S), 3),
    ("cmpst", re.compile(rb"[\x48\x4c]\x39[\x05\x0d\x15\x1d\x25\x2d\x35\x3d](....)", re.S), 3),
    ("add", re.compile(rb"[\x48\x4c]\x03[\x05\x0d\x15\x1d\x25\x2d\x35\x3d](....)", re.S), 3),
    ("sub", re.compile(rb"[\x48\x4c]\x2b[\x05\x0d\x15\x1d\x25\x2d\x35\x3d](....)", re.S), 3),
    ("ff", re.compile(rb"\xff[\x10\x20\x30\x18\x28\x38](....)", re.S), 2),
    ("call", re.compile(rb"\xe8(....)", re.S), 1),
    ("jmp", re.compile(rb"\xe9(....)", re.S), 1),
    ("lea32", re.compile(rb"\x8d[\x05\x0d\x15\x1d\x25\x2d\x35\x3d](....)", re.S), 2),
    ("mov32", re.compile(rb"\x8b[\x05\x0d\x15\x1d\x25\x2d\x35\x3d](....)", re.S), 2),
    ("movst32", re.compile(rb"\x89[\x05\x0d\x15\x1d\x25\x2d\x35\x3d](....)", re.S), 2),
    ("test32", re.compile(rb"\x85[\x05\x0d\x15\x1d\x25\x2d\x35\x3d](....)", re.S), 2),
]


def main(argv):
    pe = argv[1]
    targets = {int(a, 16) for a in argv[2:]}
    d, base, sections = load_pe(pe)

    for name, vaddr, vsize, rawptr, rawsize in sections:
        span = min(vsize, rawsize)
        blob = d[rawptr:rawptr + span]
        sec_va = base + vaddr
        for pname, rx, doff in PREFIXES:
            for m in rx.finditer(blob):
                field_va = sec_va + m.start() + doff
                disp = struct.unpack_from("<i", blob, m.start() + doff)[0]
                t = field_va + 4 + disp
                if t in targets:
                    print(f"{t:#x}  RIP {name} {pname}: instr {field_va - doff:#x} (field {field_va:#x})")

    for t in sorted(targets):
        needle = struct.pack("<Q", t)
        i = d.find(needle)
        while i >= 0:
            print(f"{t:#x}  ABS ptr at file {i:#x}")
            i = d.find(needle, i + 1)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
