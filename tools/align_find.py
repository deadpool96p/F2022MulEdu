import sys
sys.path.insert(0, "F:/Fifa22Multiplayer/tools")
import capstone
from disasm_region import load_pe
d, base, sections = load_pe(r"G:\Games\FIFA 22\FIFA22.exe")
target = int(sys.argv[1], 16)
back = int(sys.argv[2], 16) if len(sys.argv) > 2 else 0x400
def off(va):
    for name, vaddr, vsize, rawptr, rawsize in sections:
        if base+vaddr <= va < base+vaddr+min(vsize,rawsize):
            return rawptr + (va-(base+vaddr))
o = off(target)
md = capstone.Cs(capstone.CS_ARCH_X86, capstone.CS_MODE_64)
md.detail = False
cands = []
for c in range(o-back, o):
    blob = d[c:c+0x400]
    va = None
    for name, vaddr, vsize, rawptr, rawsize in sections:
        if rawptr <= c < rawptr+rawsize:
            va = base+vaddr+(c-rawptr)
    if va is None: continue
    hit = False
    bad = False
    for ins in md.disasm(blob, va):
        if ins.address == target:
            hit = True
        if ins.address > target: break
        if ins.mnemonic == "(bad)":
            bad = True; break
    if hit and not bad:
        cands.append(va)
# dedupe & keep those that are "prologue-ish"
uniq = sorted(set(cands))
for u in uniq[:40]:
    print(hex(u), d[off(u):off(u)+6].hex(' '))
