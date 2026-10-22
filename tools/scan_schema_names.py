"""Scan a VA range for `lea rax, [rip+disp32]` (48 8D 05) and report the
referenced string, plus the two immediates that follow it.

The FIFA 22 client registers its Redirector/ServiceInstance schema as a
table of (member-name pointer, member offset, member type) tuples.  Dumping
those in address order reconstructs the exact member vocabulary the client
expects in the HTTP redirector response.

Usage: python tools/scan_schema_names.py <pe> <lo_va_hex> <hi_va_hex>
"""
import sys
import struct

sys.path.insert(0, __file__.rsplit("\\", 1)[0].rsplit("/", 1)[0] + "/tools")
from disasm_region import load_pe, make_va_to_off  # noqa: E402


def cstr(data: bytes, off: int, limit: int = 80) -> str:
    end = data.find(b"\x00", off, off + limit)
    if end < 0:
        return ""
    try:
        return data[off:end].decode("ascii")
    except UnicodeDecodeError:
        return ""


def main() -> None:
    pe = sys.argv[1]
    lo = int(sys.argv[2], 16)
    hi = int(sys.argv[3], 16)

    with open(pe, "rb") as fh:
        data = fh.read()

    _, image_base, sections = load_pe(pe)
    va_to_off = make_va_to_off(sections, image_base)

    for va in range(lo, hi):
        off = va_to_off(va)
        if off is None or off + 7 > len(data):
            continue
        if data[off] != 0x48 or data[off + 1] != 0x8D or data[off + 2] != 0x05:
            continue
        disp = struct.unpack_from("<i", data, off + 3)[0]
        target = va + 7 + disp
        toff = va_to_off(target)
        if toff is None:
            continue
        s = cstr(data, toff)
        if not s or not all(32 <= ord(c) < 127 for c in s):
            continue

        # The tuple layout is: name ptr, member offset (u32), member type (u16).
        extra = []
        for delta, size in ((7, 4), (11, 4), (15, 2)):
            o2 = va_to_off(va + delta)
            if o2 is None:
                continue
            if size == 4:
                extra.append(struct.unpack_from("<I", data, o2)[0])
            else:
                extra.append(struct.unpack_from("<H", data, o2)[0])
        print(f"{va:#x}  {s!r}  raw={extra}")


if __name__ == "__main__":
    main()
