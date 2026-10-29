"""Dump EA's compiled-in trust anchor entry from a PE image.

EA compiles the "DigiCert Global Root CA" trust anchor into every module
that does its own TLS.  The shim patches the RSA modulus pointer that sits
at `entry + 0x50`, which is enough for the DirtySDK ProtoSSL stack (it
only compares the modulus) but not for the HTTP/2 stack, which rejects the
emulator's certificate with a `certificate_unknown` alert.

This script prints the raw bytes of the entry and resolves every pointer
that points back into the image, so the fields that still hold the *real*
DigiCert data can be identified.
"""

import struct
import sys


def read_pe(path):
    with open(path, "rb") as handle:
        data = handle.read()

    if data[:2] != b"MZ":
        raise SystemExit("not a PE image")

    pe_offset = struct.unpack_from("<I", data, 0x3C)[0]
    if data[pe_offset : pe_offset + 4] != b"PE\0\0":
        raise SystemExit("bad PE signature")

    coff = pe_offset + 4
    num_sections = struct.unpack_from("<H", data, coff + 2)[0]
    optional_size = struct.unpack_from("<H", data, coff + 16)[0]
    optional = coff + 20
    magic = struct.unpack_from("<H", data, optional)[0]
    pe_plus = magic == 0x20B

    image_base = struct.unpack_from("<Q", data, optional + 24)[0] if pe_plus else struct.unpack_from(
        "<I", data, optional + 28
    )[0]

    sections = []
    section_table = optional + optional_size
    for index in range(num_sections):
        entry = section_table + index * 40
        name = data[entry : entry + 8].rstrip(b"\0").decode("ascii", "replace")
        virtual_size, virtual_address, raw_size, raw_offset = struct.unpack_from(
            "<IIII", data, entry + 8
        )
        sections.append(
            {
                "name": name,
                "vaddr": virtual_address,
                "vsize": virtual_size,
                "raw_size": raw_size,
                "raw_offset": raw_offset,
            }
        )

    return data, image_base, sections


def rva_to_offset(sections, rva):
    for section in sections:
        start = section["vaddr"]
        end = start + max(section["vsize"], section["raw_size"])
        if start <= rva < end:
            return section["raw_offset"] + (rva - start)
    return None


def offset_to_rva(sections, offset):
    for section in sections:
        start = section["raw_offset"]
        end = start + section["raw_size"]
        if start <= offset < end:
            return section["vaddr"] + (offset - start)
    return None


def resolve(data, sections, image_base, value):
    """If `value` looks like a pointer into this image, return (rva, offset)."""
    rva = value - image_base
    if rva < 0:
        return None
    offset = rva_to_offset(sections, rva)
    if offset is None:
        return None
    return rva, offset


def main():
    path = sys.argv[1] if len(sys.argv) > 1 else r"G:\Games\FIFA 22\FIFA22.exe"
    needle = b"DigiCert Global Root CA\x00"

    data, image_base, sections = read_pe(path)
    print(f"image base : 0x{image_base:016X}")
    for section in sections:
        print(
            f"  section {section['name']:<10} vaddr 0x{section['vaddr']:08X} "
            f"vsize 0x{section['vsize']:08X} raw 0x{section['raw_offset']:08X} "
            f"rawsize 0x{section['raw_size']:08X}"
        )

    position = data.find(needle)
    while position != -1:
        rva = offset_to_rva(sections, position)
        print()
        print("=" * 78)
        print(f"anchor string at file offset 0x{position:X} (rva 0x{rva:X})")
        print("=" * 78)

        entry = position
        print(f"  entry +0x00 name          : {data[entry:entry + 24]!r}")
        for offset in range(0x18, 0x70, 8):
            value = struct.unpack_from("<Q", data, entry + offset)[0]
            marker = ""
            resolved = resolve(data, sections, image_base, value)
            if resolved:
                target_rva, target_offset = resolved
                head = data[target_offset : target_offset + 16]
                marker = f"  -> rva 0x{target_rva:X} @ file 0x{target_offset:X}  {head.hex()}"
                if head[:1] == b"\x30":
                    marker += "  (looks like DER)"
            print(f"  entry +0x{offset:02X} qword   : 0x{value:016X}{marker}")

        for offset, width in ((0x58, 4), (0x5C, 4)):
            value = struct.unpack_from("<I", data, entry + offset)[0]
            print(f"  entry +0x{offset:02X} dword   : 0x{value:08X} ({value})")

        # The 0x40 bytes before the name may hold the preceding entry's tail.
        print()
        print("  0x30 bytes of context before the name:")
        print("   ", data[position - 0x30 : position].hex(" "))

        position = data.find(needle, position + 1)


if __name__ == "__main__":
    main()
