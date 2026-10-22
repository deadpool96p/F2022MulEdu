"""Byte-exact RIP-relative xref finder for a PE binary.

Why this exists
---------------
Linear disassembly of a 592 MB binary is unreliable: any mis-aligned chunk
start poisons the rest of the run, and a game executable is full of data that
happens to look like code.  So we avoid disassembling entirely and use the
exact RIP-relative arithmetic instead:

    target = (end of the disp32 field) + disp32

The end of the disp32 field does **not** depend on the instruction length, so
we can scan every 4-byte window in every section and compute the target
address directly.  A hit means *some* instruction ends at that point with a
RIP-relative displacement pointing at our string.  This finds every
RIP-relative reference, with no false negatives from mis-alignment.

Two further details specific to FIFA22.exe (Frostbite's own linker):

  * Its PE section characteristics are non-standard -- `.didata` is
    0x60000020, `.xtext` is 0x40000040, `.ecode` is 0xE0000020 and
    `.srdata` is 0xC0000020.  `IMAGE_SCN_MEM_EXECUTE` is therefore *not* a
    reliable "is this code?" filter, so we scan every section.
  * Addresses in the docs are *virtual* addresses (image base + section
    relative offset), while the tables in `docs/FIFA22_QOS_PROTOPING_HTTP.md` give
    *file* offsets.  Mixing the two is the easiest way to get a false
    negative, so this tool reports both.

Usage
-----
    python find_str_xrefs.py <pe-file> <string> [<string> ...]

Each string is looked up as a NUL-terminated ASCII literal.  The tool prints
the virtual address of every occurrence and, for each, every RIP-relative
reference to it.

Requires `capstone` only for the optional disassembly of each hit.
"""
import struct
import sys


def load_pe(path):
    d = open(path, 'rb').read()
    e_lfanew = struct.unpack_from('<I', d, 0x3C)[0]
    assert d[e_lfanew:e_lfanew + 4] == b'PE\0\0', "not a PE file"
    coff = e_lfanew + 4
    n_sections, = struct.unpack_from('<H', d, coff + 2)
    opt_size, = struct.unpack_from('<H', d, coff + 16)
    opt = coff + 20
    magic, = struct.unpack_from('<H', d, opt)
    assert magic == 0x20b, "only PE32+ (x86-64) is supported, got 0x%X" % magic
    image_base, = struct.unpack_from('<Q', d, opt + 24)
    sec_off = opt + opt_size
    sections = []
    for i in range(n_sections):
        s = sec_off + i * 40
        name = d[s:s + 8].rstrip(b'\0').decode('latin1')
        vsize, vaddr, rawsize, rawptr = struct.unpack_from('<IIII', d, s + 8)
        chars, = struct.unpack_from('<I', d, s + 36)
        sections.append(dict(name=name, vaddr=vaddr, vsize=vsize, rawptr=rawptr,
                             rawsize=rawsize, chars=chars))
    return d, image_base, sections


def off_to_va(sections, image_base, off):
    for s in sections:
        if s['rawptr'] <= off < s['rawptr'] + s['rawsize']:
            return image_base + s['vaddr'] + (off - s['rawptr'])
    return None


def va_to_off(sections, image_base, va):
    rva = va - image_base
    for s in sections:
        if s['vaddr'] <= rva < s['vaddr'] + max(s['vsize'], s['rawsize']):
            return s['rawptr'] + (rva - s['vaddr'])
    return None


def find_literal(d, needle):
    """Every file offset at which `needle` appears as a NUL-terminated literal."""
    hits = []
    start = 0
    blob = needle.encode('latin1') + b'\0'
    while True:
        i = d.find(blob, start)
        if i < 0:
            return hits
        hits.append(i)
        start = i + 1


def main(argv):
    if len(argv) < 3:
        print(__doc__)
        return 2
    pe_path = argv[1]
    needles = argv[2:]

    d, base, sections = load_pe(pe_path)
    print("image_base=0x%X file_size=%d sections=%d"
          % (base, len(d), len(sections)))

    # Map every target string to the set of VAs it lives at.
    name_by_va = {}
    for needle in needles:
        offsets = find_literal(d, needle)
        if not offsets:
            print("!! %r not found" % needle)
            continue
        for off in offsets:
            va = off_to_va(sections, base, off)
            if va is None:
                print("!! %r at file 0x%X is not mapped" % (needle, off))
                continue
            name_by_va[va] = needle
            print("target %-28r file 0x%X -> VA 0x%X" % (needle, off, va))

    print("\n=== scanning every 4-byte window for a RIP-relative hit ===")
    hits = {}
    for s in sections:
        if s['rawsize'] == 0:
            continue
        blob = d[s['rawptr']:s['rawptr'] + s['rawsize']]
        sec_va = base + s['vaddr']
        n = len(blob)
        for p in range(0, n - 4):
            disp = struct.unpack_from('<i', blob, p)[0]
            target = (sec_va + p + 4) + disp
            if target in name_by_va:
                # The disp32 field ends at file offset s['rawptr']+p+4.
                hits.setdefault(name_by_va[target], []).append(
                    (sec_va + p + 4, s['rawptr'] + p))

    print("\n=== XREFS ===")
    for name in sorted(hits, key=lambda n: min(x[0] for x in hits[n])):
        v = sorted(hits[name])
        print("%-28s count=%d" % (name, len(v)))
        for va, foff in v[:24]:
            print("    disp field ends at VA=0x%X  (file 0x%X)" % (va, foff))
        if len(v) > 24:
            print("    ... %d more" % (len(v) - 24))
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
