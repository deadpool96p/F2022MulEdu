"""Locate EA's compiled-in trust-anchor table and every reference to it.

The table entries are 0x60-byte structs:

    { char name[0x40]; u32 f40; u32 f44; u32 f48; u32 f4C;
      u8* modulus; u32 exp_len; u32 exp; }

with ``exp_len == 3`` and ``exp == 0x10001`` for every RSA anchor.  The
shim only patched the "DigiCert Global Root CA" entry, so the other roots
still held EA's real moduli -- which is why a chain issued by a CA that
is *not* one of those roots could never validate.

Usage:

    python tools/anchor_xrefs.py <pe> [--refs]
"""

import struct
import sys

sys.path.insert(0, __file__.rsplit("\\", 1)[0].rsplit("/", 1)[0] + "/tools")
from disasm_region import load_pe  # noqa: E402

CODE_HINTS = ("text", "code", "xtext", "ecode")


def section_for_offset(sections, off):
    for name, vaddr, vsize, rawptr, rawsize in sections:
        if rawptr <= off < rawptr + rawsize:
            return name, vaddr, rawptr
    return None, None, None


def find_anchors(d):
    """Every 0x60-byte struct that looks like a compiled-in RSA root."""
    hits = []
    for off in range(0, len(d) - 0x60):
        exp_len, exp = struct.unpack_from("<II", d, off + 0x58)
        if exp_len != 3 or exp != 0x10001:
            continue
        name = d[off:off + 0x40].split(b"\0")[0]
        if len(name) < 4 or not all(32 <= c < 127 for c in name):
            continue
        mod = struct.unpack_from("<Q", d, off + 0x50)[0]
        fields = struct.unpack_from("<IIII", d, off + 0x40)
        hits.append((off, name.decode(), mod, fields))
    return hits


def main(argv):
    pe = argv[1]
    d, base, sections = load_pe(pe)
    hits = find_anchors(d)

    print("image base 0x%016X" % base)
    entries = []
    for off, name, mod, fields in hits:
        sname, vaddr, rawptr = section_for_offset(sections, off)
        va = base + vaddr + (off - rawptr) if vaddr is not None else None
        print(
            "anchor file 0x%X va %s section %s name=%r fields=%s modulus_va=0x%X"
            % (off, hex(va) if va else "?", sname, name,
               [hex(f) for f in fields], mod)
        )
        entries.append((off, va, name, mod))

    for i in range(1, len(entries)):
        print("gap %r -> %r : 0x%X"
              % (entries[i - 1][2], entries[i][2], entries[i][0] - entries[i - 1][0]))

    if "--refs" not in argv:
        return

    targets = {va: name for _, va, name, _ in entries if va}
    print()
    for sname, vaddr, vsize, rawptr, rawsize in sections:
        if not any(h in sname for h in CODE_HINTS):
            continue
        span = min(vsize, rawsize)
        base_va = base + vaddr
        for k in range(0, span - 4):
            disp = struct.unpack_from("<i", d, rawptr + k)[0]
            for extra in range(0, 5):
                field_va = base_va + k + 4 + extra
                tgt = field_va + disp
                if tgt in targets:
                    print("ref to %r at va 0x%X (section %s file 0x%X)"
                          % (targets[tgt], base_va + k, sname, rawptr + k))


if __name__ == "__main__":
    main(sys.argv)
