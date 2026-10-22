import sys, struct
sys.path.insert(0, "F:/Fifa22Multiplayer/tools")
from disasm_region import load_pe
pe = r"G:\Games\FIFA 22\FIFA22.exe"
d, base, sections = load_pe(pe)
def cstr(va):
    for name, vaddr, vsize, rawptr, rawsize in sections:
        if base+vaddr <= va < base+vaddr+min(vsize,rawsize):
            off = rawptr + (va - (base+vaddr))
            end = d.index(b"\x00", off)
            return d[off:end].decode("ascii", "replace")
    return None
va = int(sys.argv[1], 16)
n = int(sys.argv[2]) if len(sys.argv)>2 else 16
for i in range(n):
    a = va + i*8
    for name, vaddr, vsize, rawptr, rawsize in sections:
        if base+vaddr <= a < base+vaddr+min(vsize,rawsize):
            q = struct.unpack_from("<Q", d, rawptr+(a-(base+vaddr)))[0]
            print("%x  %x" % (a, q))
            break
