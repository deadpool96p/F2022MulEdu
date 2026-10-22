"""Faithful Python model of FIFA22.exe's QoS response DOM scanner.

The client does NOT build a real XML tree.  The path-based DOM lookup helper
at `0x140CA1DE0` is a **flat forward text scanner** over the raw response
text:

  * a path is split on '.' (and '<'),
  * a leading '.' on the path means "descend from the current node", which
    resumes the scan *after* the current node's start tag,
  * a segment matches an element whose *name* equals the segment
    ('%*' is the wildcard token),
  * the whole lookup therefore depends on DOCUMENT ORDER, not nesting.

The value reader at `0x140CA2280` skips the opening tag, then parses an
optional sign followed by decimal digits (unsigned when edx == 0).

The sibling iterator at `0x140CA1FF0` advances to the next element,
transparently skipping closing tags, and the tag-end finder at `0x140CA1BB0`
locates the '>' that closes a tag.

This model reproduces those four routines so a candidate response body can be
validated against the client's parser **without launching the game** -- which
is how every field in `docs/FIFA22_QOS_PROTOPING_HTTP.md` was checked.

Usage
-----
    python qos_dom_model.py <body-file>

Prints the values the client would read out of the given response body.
"""
import sys


# ---------------------------------------------------------------------------
# Document scanning
# ---------------------------------------------------------------------------


def _elements(doc):
    """Yield (name, start, end) for every opening tag, in document order.

    `start` is the offset of '<', `end` the offset just past '>'.
    """
    i = 0
    n = len(doc)
    while i < n:
        lt = doc.find("<", i)
        if lt < 0:
            return
        # Skip declarations, comments and closing tags.
        if doc[lt + 1: lt + 2] in ("?", "!"):
            i = _tag_end(doc, lt)
            continue
        if doc[lt + 1: lt + 2] == "/":
            i = _tag_end(doc, lt)
            continue
        j = lt + 1
        while j < n and doc[j] not in " \t\r\n/>":
            j += 1
        name = doc[lt + 1: j]
        end = _tag_end(doc, lt)
        yield name, lt, end
        i = end


def _tag_end(doc, lt):
    """Offset just past the '>' that closes the tag starting at `lt`."""
    i = lt + 1
    n = len(doc)
    while i < n:
        c = doc[i]
        if c == ">":
            return i + 1
        if c == '"' or c == "'":
            close = doc.find(c, i + 1)
            if close < 0:
                return n
            i = close + 1
            continue
        i += 1
    return n


def find_node(doc, path, node=None):
    """Mirror of `0x140CA1DE0`: resolve a dotted path to an element.

    Returns (name, start, end) or None.
    """
    segments = path.split(".")
    if node is None:
        # Top-level lookup: scan the whole document from the beginning.
        cur = None
        segs = segments
    else:
        cur = node
        segs = segments
        if segs and segs[0] == "":
            # A leading '.' means "start scanning after the current node's
            # opening tag" -- the client skips the '<' of the current node
            # and resumes the scan there.
            segs = segs[1:]
        scan_from = cur[1] + 1

    for depth, seg in enumerate(segs):
        if seg == "":
            return None
        found = None
        for name, start, end in _elements(doc):
            if name != seg:
                continue
            if depth == 0:
                if node is None:
                    found = (name, start, end)
                    break
                if start > cur[1]:
                    found = (name, start, end)
                    break
            else:
                found = (name, start, end)
                break
        if found is None:
            return None
        cur = found
    return cur


def read_value(doc, node):
    """Mirror of `0x140CA2280`: the element's value as text (may be '')."""
    if node is None:
        return ""
    _, start, end = node
    gt = doc.find(">", start)
    if gt < 0:
        return ""
    text = doc[gt + 1:]
    # The value ends at the next tag.
    nxt = text.find("<")
    if nxt >= 0:
        text = text[:nxt]
    return text.strip()


def read_int(doc, node, default=0):
    """Mirror of `0x140CA2280` with edx == 0: unsigned base-10."""
    text = read_value(doc, node)
    i = 0
    while i < len(text) and text[i] in " \t\r\n":
        i += 1
    sign = 1
    if i < len(text) and text[i] in "+-":
        if text[i] == "-":
            sign = -1
        i += 1
    digits = ""
    while i < len(text) and text[i].isdigit():
        digits += text[i]
        i += 1
    if not digits:
        return default
    return sign * int(digits)


def next_sibling(doc, node):
    """Mirror of `0x140CA1FF0`: the next element after `node`."""
    if node is None:
        return None
    _, _, end = node
    for name, start, nend in _elements(doc):
        if start >= end:
            return (name, start, nend)
    return None


# ---------------------------------------------------------------------------
# The exact bodies the emulator serves, and the values the client must read
# ---------------------------------------------------------------------------

QOS_IP_DECIMAL = 2_130_706_433   # 127.0.0.1 as a big-endian decimal u32
QOS_UDP_PORT = 17498

QOS_BODY = """<?xml version="1.0" encoding="UTF-8"?>
<qos>
   <qosip>%d</qosip>
   <qosport>%d</qosport>
   <numprobes>2</numprobes>
   <probesize>1200</probesize>
   <requestid>1</requestid>
   <reqsecret>0</reqsecret>
</qos>""" % (QOS_IP_DECIMAL, QOS_UDP_PORT)

FIREWALL_BODY = """<?xml version="1.0" encoding="UTF-8"?>
<firewall>
   <numinterfaces>1</numinterfaces>
   <ips>
       <ips>%d</ips>
   </ips>
   <ports>
       <ports>%d</ports>
   </ports>
   <requestid>1</requestid>
   <reqsecret>0</reqsecret>
</firewall>""" % (QOS_IP_DECIMAL, QOS_UDP_PORT)

FIRETYPE_BODY = """<?xml version="1.0" encoding="UTF-8"?>
<firetype>
   <firetype>0</firetype>
</firetype>"""


def _check(doc, root, fields):
    """Read `fields` out of `doc` under `root`, and return them."""
    node = find_node(doc, root)
    if node is None:
        raise AssertionError("root element %r not found" % root)
    out = {}
    for field, path in fields.items():
        child = find_node(doc, path, node)
        if child is None:
            out[field] = None
            continue
        out[field] = read_int(doc, child)
    return out


def self_test():
    """Assert the model reads the emulator's own bodies as the client would."""
    qos = _check(QOS_BODY, "qos", {
        "qosip": ".qosip",
        "qosport": ".qosport",
        "numprobes": ".numprobes",
        "probesize": ".probesize",
        "requestid": ".requestid",
        "reqsecret": ".reqsecret",
    })
    assert qos["qosip"] == QOS_IP_DECIMAL, qos
    assert qos["qosport"] == QOS_UDP_PORT, qos
    assert qos["numprobes"] >= 2, qos          # 0x141FE8753
    assert qos["probesize"] != 0, qos          # 0x141FE8759
    assert qos["requestid"] != 0, qos          # 0x141FE874D
    assert qos["qosip"] != 0 and qos["qosport"] != 0, qos

    fw = _check(FIREWALL_BODY, "firewall", {
        "numinterfaces": ".numinterfaces",
        "ips": ".ips.ips",
        "ports": ".ports.ports",
    })
    assert fw["ips"] is not None, fw           # 0x141FE87D9
    assert fw["ports"] is not None, fw         # 0x141FE87F0

    ft = _check(FIRETYPE_BODY, "firetype", {"firetype": ".firetype"})
    assert ft["firetype"] is not None, ft

    # The dotted-quad form the open-ds2 reference serves parses to 127, which
    # is why it cannot be used.  This is the bug the model exists to catch.
    wrong = _check("<qos><qosip>127.0.0.1</qosip></qos>", "qos", {"qosip": ".qosip"})
    assert wrong["qosip"] == 127, wrong

    return True


def main(argv):
    ok = self_test()
    print("self-test: %s" % ("PASS" if ok else "FAIL"))
    if not ok:
        return 1
    if len(argv) < 2:
        print("\nUsage: python qos_dom_model.py [<body-file>]")
        print("With no body file, only the self-test runs.")
        return 0
    if len(argv) > 1:
        body = open(argv[1], encoding='utf-8', errors='replace').read()
        for root, fields in (
            ("qos", {".qosip": "qosip", ".qosport": "qosport",
                     ".numprobes": "numprobes", ".probesize": "probesize",
                     ".requestid": "requestid", ".reqsecret": "reqsecret"}),
            ("firewall", {".numinterfaces": "numinterfaces",
                          ".ips.ips": "ips", ".ports.ports": "ports"}),
            ("firetype", {".firetype": "firetype"}),
        ):
            node = find_node(body, root)
            if node is None:
                continue
            for path, label in fields.items():
                child = find_node(body, path, node)
                print("%-14s %-14s %s"
                      % (root, label,
                         "MISSING" if child is None else read_int(body, child)))
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
