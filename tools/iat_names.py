import sys, struct
sys.path.insert(0, "F:/Fifa22Multiplayer/tools")
from disasm_region import load_pe
pe = r"G:\Games\FIFA 22\FIFA22.exe"
d, base, sections = load_pe(pe)
def rva2off(rva):
    for name, vaddr, vsize, rawptr, rawsize in sections:
        if vaddr <= rva < vaddr + max(vsize, rawsize):
            return rawptr + (rva - vaddr)
    return None
e_lfanew = struct.unpack_from("<I", d, 0x3c)[0]
opt = e_lfanew + 0x18
magic = struct.unpack_from("<H", d, opt)[0]
ddoff = opt + (112 if magic == 0x20b else 96)
imp_rva, imp_size = struct.unpack_from("<II", d, ddoff + 8)
imp_off = rva2off(imp_rva)
res = {}
off = imp_off
while True:
    ilt, ts, fwd, name_rva, iat_rva = struct.unpack_from("<IIIII", d, off)
    if ilt == 0 and name_rva == 0:
        break
    dll = d[rva2off(name_rva):].split(b"\x00")[0].decode()
    thunk = ilt if ilt else iat_rva
    t = rva2off(thunk)
    i = 0
    while True:
        v = struct.unpack_from("<Q", d, t + i*8)[0]
        if v == 0:
            break
        if v & (1 << 63):
            nm = "#ord%d" % (v & 0xffff)
        else:
            no = rva2off(v & 0x7fffffff)
            nm = d[no+2:].split(b"\x00")[0].decode()
        res[base + iat_rva + i*8] = "%s!%s" % (dll, nm)
        i += 1
    off += 20
import json
for a in sys.argv[1:]:
    va = int(a, 16)
    print("%x  %s" % (va, res.get(va, "?")))
