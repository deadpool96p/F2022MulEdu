import sys, struct, re
sys.path.insert(0, "F:/Fifa22Multiplayer/tools")
import capstone
from disasm_region import load_pe
d, base, sections = load_pe(r"G:\Games\FIFA 22\FIFA22.exe")
def off(va):
    for n,vaddr,vsize,rawptr,rawsize in sections:
        if base+vaddr<=va<base+vaddr+min(vsize,rawsize): return rawptr+(va-(base+vaddr))
def cstr(va):
    o = off(va)
    if o is None: return None
    e = d.index(b"\x00", o)
    s = d[o:e]
    if len(s) < 3 or not all(32<=c<127 for c in s): return None
    return s.decode()
lo = int(sys.argv[1],16); hi = int(sys.argv[2],16)
md = capstone.Cs(capstone.CS_ARCH_X86, capstone.CS_MODE_64)
o = off(lo); end = off(hi)
blob = d[o:o+ (end-o)]
ins = list(md.disasm(blob, lo))
for i, x in enumerate(ins):
    if x.mnemonic == "lea" and "rip" in x.op_str:
        # parse target
        try:
            target = int(x.op_str.split("[rip ")[1].split("]")[0], 16)
        except Exception:
            continue
        # compute actual
        # capstone prints absolute already for rip-relative
        s = cstr(target)
        if not s: continue
        nxt = ins[i+1] if i+1 < len(ins) else None
        nx = ""
        if nxt: nx = "%s %s" % (nxt.mnemonic, nxt.op_str)
        print("%x  %-46s  -> %r   | next: %s" % (x.address, x.op_str, s, nx))
