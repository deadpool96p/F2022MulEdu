import sys, struct
sys.path.insert(0, "F:/Fifa22Multiplayer/tools")
from disasm_region import load_pe
pe = r"G:\Games\FIFA 22\FIFA22.exe"
d, base, sections = load_pe(pe)
def off2va(off):
    for name, vaddr, vsize, rawptr, rawsize in sections:
        if rawptr <= off < rawptr + rawsize:
            return base + vaddr + (off - rawptr), name
    return None, None
fo = int(sys.argv[1], 16)
va, sec = off2va(fo)
print("file %x -> va %x (%s)" % (fo, va, sec))
# scan backwards/forwards for a run of 8-byte values that look like pointers into the image
lo = fo - 0x400; hi = fo + 0x400
for a in range(lo, hi, 8):
    q = struct.unpack_from("<Q", d, a)[0]
    v, s = off2va(a)
    if v is None: continue
    if base <= q < base + 0x60000000:
        print("  va %x (file %x): -> %x" % (v, a, q))
