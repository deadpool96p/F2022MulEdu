import sys, struct
sys.path.insert(0, "F:/Fifa22Multiplayer/tools")
from disasm_region import load_pe
pe = r"G:\Games\FIFA 22\FIFA22.exe"
d, base, sections = load_pe(pe)
def read_cstr(va):
    for name, vaddr, vsize, rawptr, rawsize in sections:
        if base+vaddr <= va < base+vaddr+min(vsize,rawsize):
            off = rawptr + (va - (base+vaddr))
            end = d.index(b"\x00", off)
            s = d[off:end]
            if all(32 <= c < 127 for c in s) and len(s) >= 3:
                return s.decode("ascii")
            return None
    return None
lo = int(sys.argv[1], 16); hi = int(sys.argv[2], 16)
for a in range(lo, hi, 8):
    for name, vaddr, vsize, rawptr, rawsize in sections:
        if base+vaddr <= a < base+vaddr+min(vsize,rawsize):
            q = struct.unpack_from("<Q", d, rawptr+(a-(base+vaddr)))[0]
            s = read_cstr(q)
            print("%x  %016x  %s" % (a, q, repr(s) if s else ""))
            break
