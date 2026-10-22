"""Print every printable string in a file-offset range of a PE image.

Usage: python tools/dump_strings.py <pe> <start_file_off_hex> <end_file_off_hex>
"""

import re
import sys

sys.path.insert(0, __file__.rsplit("\\", 1)[0].rsplit("/", 1)[0] + "/tools")
from disasm_region import load_pe  # noqa: E402


def main(argv):
    pe = argv[1]
    start = int(argv[2], 16)
    end = int(argv[3], 16)

    d, base, sections = load_pe(pe)
    blob = d[start:end]
    pattern = re.compile(rb"[\x20-\x7e]{3,}")
    for m in pattern.finditer(blob):
        print("0x%08X  %r" % (start + m.start(), m.group().decode("latin1")))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
