"""Disassemble a region of a PE binary, annotating string operands.

Used to read the ground truth out of FIFA22.exe.  For every instruction it
prints the disassembly, and where an operand refers to a printable ASCII
string it appends that string as a comment.

Usage:
    python disasm_region.py <pe-file> <start_va_hex> <size_bytes> [--strings]

`start_va_hex` is a *virtual* address (image base + section relative offset),
e.g. `0x141FE8540`.  Frostbite's linker emits non-standard section
characteristics for this binary (`.didata` 0x60000020, `.xtext` 0x40000040,
`.ecode` 0xE0000020, `.srdata` 0xC0000020+), so `IMAGE_SCN_MEM_EXECUTE` is
not a usable "is this code?" filter -- this tool disassembles whatever range
you ask for.

Requires `capstone`.
"""
import struct
import sys
from capstone import Cs, CS_ARCH_X86, CS_MODE_64


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
        sections.append((name, vaddr, vsize, rawptr, rawsize))
    return d, image_base, sections


def make_va_to_off(sections, image_base):
    def va_to_off(va):
        rva = va - image_base
        for name, vaddr, vsize, rawptr, rawsize in sections:
            if vaddr <= rva < vaddr + max(vsize, rawsize):
                return rawptr + (rva - vaddr)
        return None
    return va_to_off


def cstr(d, off, maxlen=90):
    if off is None:
        return None
    out = b''
    while len(out) < maxlen and off + len(out) < len(d):
        b = d[off + len(out)]
        if b == 0:
            break
        if 32 <= b < 127:
            out += bytes([b])
        else:
            return None
    return out.decode('latin1') if out else None


def main(argv):
    args = [a for a in argv[1:] if not a.startswith('--')]
    if len(args) < 3:
        print(__doc__)
        return 2
    pe_path = args[0]
    start = int(args[1], 16)
    size = int(args[2], 0)

    d, base, sections = load_pe(pe_path)
    va_to_off = make_va_to_off(sections, base)

    def annot(va):
        s = cstr(d, va_to_off(va))
        return '"%s"' % s if s else None

    off = va_to_off(start)
    if off is None:
        print("VA 0x%X not mapped" % start)
        return 1

    md = Cs(CS_ARCH_X86, CS_MODE_64)
    md.detail = True

    for insn in md.disasm(d[off:off + size], start):
        extra = ''
        for op in insn.operands:
            if op.type == 3:  # X86_OP_MEM
                m = op.value.mem
                if m.base == 0 and m.index == 0 and m.scale == 1 and m.disp:
                    a = annot(m.disp)
                    if a:
                        extra = '   ; ' + a
                elif m.base == 41:  # X86_REG_RIP
                    t = insn.address + insn.size + m.disp
                    a = annot(t)
                    if a:
                        extra = '   ; ' + a
            elif op.type == 2:  # X86_OP_IMM
                a = annot(op.value.imm)
                if a:
                    extra = '   ; ' + a
        print("%016X  %-8s %s%s" % (insn.address, insn.mnemonic, insn.op_str, extra))
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
