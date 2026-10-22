import sys, re
sys.path.insert(0, "F:/Fifa22Multiplayer/tools")
from disasm_region import load_pe
d, base, sections = load_pe(r"G:\Games\FIFA 22\FIFA22.exe")
va = int(sys.argv[1], 16)
back = int(sys.argv[2], 16) if len(sys.argv) > 2 else 0x800
# find raw offset
def off(va):
    for name, vaddr, vsize, rawptr, rawsize in sections:
        if base+vaddr <= va < base+vaddr+min(vsize,rawsize):
            return rawptr + (va-(base+vaddr))
o = off(va)
best = None
# scan for common prologue bytes preceded by int3 padding
for i in range(o-back, o):
    if d[i-1:i] == b"\xcc" or d[i-2:i] == b"\xcc\xcc":
        # candidate start
        cands = [i, i+1]
        for c in cands:
            b = d[c:c+4]
            # push reg / sub rsp
            if b[0] in (0x40,0x41,0x42,0x43,0x44,0x45,0x46,0x47,0x55,0x56,0x57,0x53) and 0xc0 <= b[1] <= 0xff:
                if best is None or c > best[0]:
                    best = (c, base+0)
        # mov [rsp+8],reg prologue
        if d[c:c+3] == b"\x48\x89\x5c\x24"[:3]:
            pass
for name, vaddr, vsize, rawptr, rawsize in sections:
    pass
# convert
def vaof(offv):
    for name, vaddr, vsize, rawptr, rawsize in sections:
        if rawptr <= offv < rawptr+rawsize:
            return base+vaddr+(offv-rawptr)
if best:
    print("candidate func start: %x" % vaof(best[0]))
else:
    print("none")
