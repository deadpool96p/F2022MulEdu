"""Walk a Heat2 TDF payload and print every member tag / type it decodes.

Mirrors crates/blaze-tdf/src/reader.rs so that a payload which decodes into
an *empty* struct (a symptom of a wrong type byte) can still be inspected.

Usage: python tools/dump_tdf.py <frame-file> [--offset N]
"""
import sys

TYPES = {
    0x00: "Integer", 0x01: "String", 0x02: "Blob", 0x03: "Struct",
    0x04: "List", 0x05: "Map", 0x06: "Union", 0x07: "Variable",
    0x08: "ObjectType", 0x09: "ObjectId", 0x0A: "Float",
    0x0B: "TimeValue", 0x0C: "Generic", 0x0D: "Unknown",
}


def char_to_6bit(c):
    if c == 0:
        return 0
    if (c & 0x20) == 0:
        return 0x20 | (c & 0x1F)
    return c & 0x1F


def sixbit_to_char(p):
    p &= 0x3F
    if p == 0:
        return ""
    if (p & 0x20) != 0:
        return chr((p & 0x1F) | 0x40)
    return chr((p & 0x1F) | 0x20)


def decode_tag(b):
    """Decode a 3-byte big-endian TDF tag (24 bits = 4 x 6-bit chars)."""
    t = (b[0] << 16) | (b[1] << 8) | b[2]
    chars = []
    for shift in (18, 12, 6, 0):
        chars.append(sixbit_to_char((t >> shift) & 0x3F))
    return "".join(chars)


class R:
    def __init__(self, data, off=0):
        self.d = data
        self.o = off

    def byte(self):
        b = self.d[self.o]
        self.o += 1
        return b

    def raw(self, n):
        b = self.d[self.o:self.o + n]
        self.o += n
        return b

    def varint(self):
        result = 0
        shift = 0
        while True:
            b = self.byte()
            result |= (b & 0x7F) << shift
            if not (b & 0x80):
                return result
            shift += 7


def read_value(r, t, depth, indent):
    pad = "  " * indent
    if t == 0x00:
        v = r.varint()
        print(f"{pad}Integer {v}")
    elif t == 0x01:
        n = r.varint()
        b = r.raw(n)
        print(f"{pad}String {b!r}")
    elif t == 0x02:
        n = r.varint()
        b = r.raw(n)
        print(f"{pad}Blob len={n} {b[:48]!r}")
    elif t == 0x03:
        read_struct(r, depth + 1, indent)
    elif t == 0x04:
        it = r.byte()
        n = r.varint()
        print(f"{pad}List of {TYPES.get(it, it)} x{n}")
        for _ in range(n):
            read_value(r, it, depth + 1, indent + 1)
    elif t == 0x05:
        kt = r.byte()
        vt = r.byte()
        n = r.varint()
        print(f"{pad}Map {TYPES.get(kt, kt)}->{TYPES.get(vt, vt)} x{n}")
        for _ in range(n):
            read_value(r, kt, depth + 1, indent + 1)
            read_value(r, vt, depth + 1, indent + 1)
    elif t == 0x06:
        idx = r.byte()
        tag = decode_tag(r.raw(3))
        mt = r.byte()
        print(f"{pad}Union case={idx} member={tag!r} type={TYPES.get(mt, mt)}")
        read_value(r, mt, depth + 1, indent + 1)
    elif t == 0x07:
        present = r.byte()
        print(f"{pad}Variable present={present}")
        if present:
            print(f"{pad}  tdf_id={r.varint()}")
            tag = decode_tag(r.raw(3))
            mt = r.byte()
            print(f"{pad}  member={tag!r} type={TYPES.get(mt, hex(mt))}")
            read_value(r, mt, depth + 1, indent + 1)
            r.byte()  # trailing terminator
    elif t == 0x08:
        print(f"{pad}ObjectType {r.varint()} {r.varint()}")
    elif t == 0x09:
        print(f"{pad}ObjectId {r.varint()} {r.varint()} {r.varint()}")
    elif t == 0x0A:
        print(f"{pad}Float {r.raw(4).hex()}")
    elif t == 0x0B:
        print(f"{pad}TimeValue {r.varint()}")
    elif t == 0x0C:
        print(f"{pad}Generic {r.varint()} {r.varint()}")
    else:
        raise ValueError(f"unknown type 0x{t:02X} at {r.o}")


def read_struct(r, depth=0, indent=0):
    pad = "  " * indent
    while True:
        b0 = r.byte()
        if b0 == 0x00:
            print(f"{pad}(end struct)")
            return
        tag = decode_tag(bytes([b0]) + r.raw(2))
        t = r.byte()
        print(f"{pad}tag={tag!r} type={TYPES.get(t, hex(t))}")
        read_value(r, t, depth + 1, indent + 1)


def main():
    path = sys.argv[1]
    off = 16
    if "--offset" in sys.argv:
        off = int(sys.argv[sys.argv.index("--offset") + 1], 0)
    data = open(path, "rb").read()
    r = R(data, off)
    read_struct(r)


if __name__ == "__main__":
    main()
