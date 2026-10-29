"""Find every RIP-relative reference to a given virtual address in a PE.

The reference target of a RIP-relative operand is `end_of_disp32_field +
disp32`, and `end_of_disp32_field` is the address of the *field*, not of the
instruction.  So scanning every 4-byte window of the file for a `disp32` that
satisfies the relation finds every reference, regardless of instruction
length or alignment.

Usage: python tools/find_addr_xrefs.py <pe> <target_va_hex>
"""
import struct
import sys

sys.path.insert(0, __file__.rsplit("\\", 1)[0].rsplit("/", 1)[0] + "/tools")
from disasm_region import load_pe, make_va_to_off  # noqa: E402


def main(argv):
    pe = argv[1]
    target = int(argv[2], 16)

    d, base, sections = load_pe(pe)
    va_to_off = make_va_to_off(sections, base)

    # Scan section by section: the whole-image offset->VA map for a 592 MB
    # binary does not fit in memory, but each section does.
    hits = []
    for name, vaddr, vsize, rawptr, rawsize in sections:
        span = min(vsize, rawsize)
        for k in range(0, span - 4):
            off = rawptr + k
            field_va = base + vaddr + k
            disp = struct.unpack_from("<i", d, off)[0]
            if field_va + 4 + disp == target:
                hits.append(field_va)

    print(f"target {target:#x}: {len(hits)} rip-relative reference(s)")
    for h in hits:
        # The instruction usually starts 1..3 bytes before the disp field.
        for back in range(1, 4):
            start = h - back
            o = va_to_off(start)
            if o is None:
                continue
            print(f"  field at {h:#x} (instr ~{start:#x})")
            break
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
