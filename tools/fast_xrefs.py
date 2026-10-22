"""Fast RIP-relative xref finder for large PE binaries.

The naive scanner in find_addr_xrefs.py walks every 4-byte window in Python,
which is far too slow for a 592 MB image.  This version uses a byte-regex to
find only the *instruction prefixes* that can carry a RIP-relative disp32, so
the heavy lifting happens in C inside the `re` engine.

Forms handled (prefix, offset of the disp32 field within the match):
  48/4C 8D <modrm>   lea   r64, [rip+disp]      disp at +3
  48/4C 8B <modrm>   mov   r64, [rip+disp]      disp at +3
  48/4C 89 <modrm>   mov   [rip+disp], r64      disp at +3
  48/4C 3B <modrm>   cmp   r64, [rip+disp]      disp at +3
  48/4C 39 <modrm>   cmp   [rip+disp], r64      disp at +3
  0F 9x /0F Bx       jcc / movzx (ModRM.r/m=5) disp at +2
  E8 / E9            call / jmp                disp at +1
  FF /2, /4          call/jmp [rip+disp]       disp at +2

Usage: python tools/fast_xrefs.py <pe> <target_va_hex> [section-name-filter]
"""
import re
import struct
import sys

sys.path.insert(0, __file__.rsplit("\\", 1)[0].rsplit("/", 1)[0] + "/tools")
from disasm_region import load_pe  # noqa: E402

# (name, regex, disp_offset)
PREFIXES = [
    ("lea", re.compile(rb"[\x48\x4c]\x8d[\x05\x0d\x15\x1d\x25\x2d\x35\x3d](....)", re.S), 3),
    ("mov/lea2", re.compile(rb"[\x48\x4c]\x8b[\x05\x0d\x15\x1d\x25\x2d\x35\x3d](....)", re.S), 3),
    ("movsto", re.compile(rb"[\x48\x4c]\x89[\x05\x0d\x15\x1d\x25\x2d\x35\x3d](....)", re.S), 3),
    ("cmp", re.compile(rb"[\x48\x4c]\x3b[\x05\x0d\x15\x1d\x25\x2d\x35\x3d](....)", re.S), 3),
    ("cmpsto", re.compile(rb"[\x48\x4c]\x39[\x05\x0d\x15\x1d\x25\x2d\x35\x3d](....)", re.S), 3),
    ("ff", re.compile(rb"\xff[\x10\x20\x30\x18\x28\x38](....)", re.S), 2),
    ("call", re.compile(rb"\xe8(....)", re.S), 1),
    ("jmp", re.compile(rb"\xe9(....)", re.S), 1),
]


def main(argv):
    pe = argv[1]
    target = int(argv[2], 16)
    filt = argv[3] if len(argv) > 3 else None

    d, base, sections = load_pe(pe)
    for name, vaddr, vsize, rawptr, rawsize in sections:
        if filt and filt not in name:
            continue
        span = min(vsize, rawsize)
        blob = d[rawptr:rawptr + span]
        sec_va = base + vaddr
        for pname, rx, doff in PREFIXES:
            for m in rx.finditer(blob):
                field_va = sec_va + m.start() + doff
                disp = struct.unpack_from("<i", blob, m.start() + doff)[0]
                if field_va + 4 + disp == target:
                    print(f"{name} {pname}: field {field_va:#x} instr {field_va - doff:#x}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
