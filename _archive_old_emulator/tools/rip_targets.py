"""Print the resolved target address of every RIP-relative operand in a range.

`disasm_region.py` prints capstone's raw `op_str`, which hides the actual
target of `mov reg, [rip + disp]`.  This tool resolves them, which is what
you need to follow a pointer chain inside FIFA22.exe.

Usage: python tools/rip_targets.py <pe> <start_va_hex> <size_bytes>
"""
import sys

sys.path.insert(0, __file__.rsplit("\\", 1)[0].rsplit("/", 1)[0] + "/tools")
from capstone import Cs, CS_ARCH_X86, CS_MODE_64  # noqa: E402
from capstone.x86 import X86_OP_MEM, X86_REG_RIP  # noqa: E402
from disasm_region import cstr, load_pe, make_va_to_off  # noqa: E402


def main(argv):
    pe = argv[1]
    start = int(argv[2], 16)
    size = int(argv[3], 0)

    d, base, sections = load_pe(pe)
    va_to_off = make_va_to_off(sections, base)

    def annot(va):
        s = cstr(d, va_to_off(va))
        return '"%s"' % s if s else ""

    off = va_to_off(start)
    md = Cs(CS_ARCH_X86, CS_MODE_64)
    md.detail = True

    for insn in md.disasm(d[off:off + size], start):
        for op in insn.operands:
            if op.type == X86_OP_MEM and op.value.mem.base == X86_REG_RIP:
                t = insn.address + insn.size + op.value.mem.disp
                o = va_to_off(t)
                tag = annot(t) if o is not None else "<unmapped>"
                if o is None:
                    tag = "<unmapped>"
                else:
                    extra = annot(t)
                    tag = extra if extra else ""
                print("%016X  %-8s %-40s -> %#x %s" % (
                    insn.address, insn.mnemonic, insn.op_str, t, tag))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
