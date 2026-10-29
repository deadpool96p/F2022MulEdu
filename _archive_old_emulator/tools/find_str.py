"""Scan a binary for literal substrings and report their file offsets.

Usage: python tools/find_str.py <file> <needle> [<needle> ...]
"""

import sys


def main(argv):
    path = argv[1]
    needles = [n.encode("latin1") for n in argv[2:]]
    with open(path, "rb") as fh:
        data = fh.read()
    for needle in needles:
        start = 0
        hits = []
        while True:
            pos = data.find(needle, start)
            if pos == -1:
                break
            hits.append(pos)
            start = pos + 1
            if len(hits) > 40:
                break
        print("%r: %d hit(s)" % (needle.decode("latin1"), len(hits)))
        for pos in hits[:40]:
            lo = max(0, pos - 0)
            ctx = data[max(0, pos - 24):pos + len(needle) + 24]
            printable = "".join(chr(b) if 32 <= b < 127 else "." for b in ctx)
            print("   0x%08X  ...%s..." % (pos, printable))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
