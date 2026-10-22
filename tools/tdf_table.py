"""Reconstruct a TDF struct definition from FIFA22.exe's compiled-in registry.

EA's client embeds a TDF type registry: a table of type descriptions that
point at the type name and at a field table of ``{ const char* name; u32 type }``
entries.  The same field names are what the redirector XML uses (lower-cased),
so the registry is the ground truth for the redirector reply format.

Usage: python tools/tdf_table.py <pe> <type name substring>
"""

import struct
import sys

sys.path.insert(0, __file__.rsplit("\\", 1)[0].rsplit("/", 1)[0] + "/tools")
from disasm_region import load_pe  # noqa: E402


def file_to_va(sections, base, off):
    for name, vaddr, vsize, rawptr, rawsize in sections:
        if rawptr <= off < rawptr + rawsize:
            return base + vaddr + (off - rawptr)
    return None


def va_to_off(sections, base, va):
    for name, vaddr, vsize, rawptr, rawsize in sections:
        start = base + vaddr
        if start <= va < start + vsize:
            return rawptr + (va - start)
    return None


def cstr(d, off):
    end = d.find(b"\0", off, off + 0x200)
    if end == -1:
        return None
    return d[off:end].decode("latin1")


def find_all(d, needle):
    out = []
    start = 0
    while True:
        pos = d.find(needle, start)
        if pos == -1:
            break
        out.append(pos)
        start = pos + 1
    return out


def main(argv):
    pe = argv[1]
    want = argv[2]
    d, base, sections = load_pe(pe)

    # 1. locate the type-name string
    type_va = None
    for off in find_all(d, want.encode("latin1")):
        va = file_to_va(sections, base, off)
        print("type name at file 0x%X va %s" % (off, hex(va) if va else "?"))
        if va:
            type_va = va
    if type_va is None:
        return 1

    # 2. find pointers to it (the registry entry)
    ptr = struct.pack("<Q", type_va)
    for off in find_all(d, ptr):
        desc_va = file_to_va(sections, base, off)
        if desc_va is None:
            continue
        # the description struct is { name_ptr, field_count, fields_ptr, ... }
        desc_off = va_to_off(sections, base, desc_va)
        if desc_off is None:
            continue
        chunk = d[desc_off:desc_off + 0x40]
        vals = struct.unpack_from("<4Q", chunk, 0)
        count = struct.unpack_from("<I", chunk, 0x18)[0]
        print("\nregistry entry at file 0x%X va %s: %s count=%d"
              % (desc_off, hex(desc_va), [hex(v) for v in vals], count))
        # the field table pointer is usually the 3rd qword
        for idx, fptr in enumerate(vals[:4]):
            foff = va_to_off(sections, base, fptr) if fptr else None
            if foff is None:
                continue
            first = cstr(d, foff)
            if first is None:
                continue
            print("  qword %d -> file 0x%X  first field name %r" % (idx, foff, first))
            for i in range(min(count if count else 16, 64)):
                ent = struct.unpack_from("<QI", d, foff + i * 16)
                nm = cstr(d, va_to_off(sections, base, ent[0]) or 0) if ent[0] else None
                print("     field %2d  %-32s type=0x%X" % (i, nm, ent[1]))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
