#!/usr/bin/env python3
"""Download SingleStepTests CPU suites and convert them for icon-core's tests.

The 80186 repository (github.com/SingleStepTests/80186) currently contains no
test data, so two hardware-generated suites stand in for it:
  * the 8086 v1 suite (JSON), for the 8086 instruction set; the harness skips
    the forms whose behaviour differs on the 80186;
  * the 80286 real-mode v1 suite (MOO binary), only for the opcodes the 80186
    added (PUSHA/POPA, BOUND, PUSH imm, IMUL imm, shift imm, ENTER/LEAVE).

Usage:  python3 scripts/fetch_cpu_tests.py [--limit N]

Writes testdata/8086/*.txt and testdata/80286/*.txt (gitignored), each with a
meta.txt. Each test becomes five or six lines:
    T <idx>
    I ax bx cx dx cs ss ds es sp bp si di ip flags
    R addr val addr val ...         (initial RAM)
    F name=val ...                  (final registers that changed)
    M addr val addr val ...         (final RAM)
    X num addr                      (optional: exception number and linear
                                     address of the FLAGS it pushed)
"""
import gzip
import json
import os
import struct
import sys
import urllib.error
import urllib.request

BASE = "https://raw.githubusercontent.com/SingleStepTests/8086/main/v1/"
BASE286 = "https://raw.githubusercontent.com/SingleStepTests/80286/main/v1_real_mode/"
# Opcodes the 80186 added over the 8086; only these are taken from the 286 suite.
OPS_186 = ["60", "61", "62", "68", "69", "6A", "6B", "C8", "C9"] + \
    [f"C0.{r}" for r in range(8)] + [f"C1.{r}" for r in range(8)]
REGS = ["ax", "bx", "cx", "dx", "cs", "ss", "ds", "es", "sp", "bp", "si", "di", "ip", "flags"]


def fetch(url):
    with urllib.request.urlopen(url, timeout=120) as r:
        return r.read()


def parse_moo(data):
    """Minimal reader for the MOO format (github.com/dbalsom/moo, v1.1)."""
    assert data[:4] == b"MOO "
    off = 8 + struct.unpack_from("<I", data, 4)[0]
    tests = []

    def state(o, end):
        st = {"regs": {}, "ram": []}
        while o < end:
            tag = data[o:o + 4]
            ln = struct.unpack_from("<I", data, o + 4)[0]
            p = o + 8
            if tag == b"REGS":
                mask = struct.unpack_from("<H", data, p)[0]
                p += 2
                for i, name in enumerate(REGS):
                    if mask & (1 << i):
                        st["regs"][name] = struct.unpack_from("<H", data, p)[0]
                        p += 2
            elif tag == b"RAM ":
                n = struct.unpack_from("<I", data, p)[0]
                p += 4
                for _ in range(n):
                    a, v = struct.unpack_from("<IB", data, p)
                    st["ram"].append((a, v))
                    p += 5
            o += 8 + ln
        return st

    while off < len(data):
        tag = data[off:off + 4]
        ln = struct.unpack_from("<I", data, off + 4)[0]
        body = off + 8
        if tag == b"TEST":
            t = {"idx": struct.unpack_from("<I", data, body)[0]}
            p, end = body + 4, body + ln
            while p < end:
                st = data[p:p + 4]
                sl = struct.unpack_from("<I", data, p + 4)[0]
                q = p + 8
                if st == b"INIT":
                    t["initial"] = state(q, q + sl)
                elif st == b"FINA":
                    t["final"] = state(q, q + sl)
                elif st == b"EXCP":
                    t["exception"] = struct.unpack_from("<BI", data, q)
                p = q + sl
            tests.append(t)
        off = body + ln
    return tests


def write_tests(out, tests, limit):
    if limit:
        tests = tests[:limit]
    lines = []
    for t in tests:
        ini, fin = t["initial"], t["final"]
        lines.append(f"T {t.get('idx', 0)}")
        lines.append("I " + " ".join(str(ini["regs"][k]) for k in REGS))
        lines.append("R " + " ".join(f"{a} {v}" for a, v in ini["ram"]))
        lines.append("F " + " ".join(f"{k}={v}" for k, v in fin["regs"].items()))
        lines.append("M " + " ".join(f"{a} {v}" for a, v in fin["ram"]))
        if "exception" in t:
            lines.append(f"X {t['exception'][0]} {t['exception'][1]}")
    tmp = out + ".tmp"
    with open(tmp, "w") as f:
        f.write("\n".join(lines) + "\n")
    os.replace(tmp, out)
    return len(tests)


def fetch_286(limit):
    root = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "testdata", "80286")
    os.makedirs(root, exist_ok=True)
    meta = json.loads(fetch(BASE286 + "metadata.json"))
    with open(os.path.join(root, "meta.txt"), "w") as m:
        for stem in OPS_186:
            op, _, r = stem.partition(".")
            info = meta["opcodes"].get(op, {})
            if r:
                info = info.get("reg", {}).get(r, {})
            m.write(f"{stem} {info.get('status', '?')} {info.get('flags-mask', 0xFFFF)}\n")
    for stem in OPS_186:
        out = os.path.join(root, stem + ".txt")
        if os.path.exists(out):
            continue
        tests = parse_moo(gzip.decompress(fetch(BASE286 + stem + ".MOO.gz")))
        print(f"80286 {stem}: {write_tests(out, tests, limit)} tests")


def main():
    limit = None
    if "--limit" in sys.argv:
        limit = int(sys.argv[sys.argv.index("--limit") + 1])
    root = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "testdata", "8086")
    os.makedirs(root, exist_ok=True)

    meta = json.loads(fetch(BASE + "metadata.json"))
    entries = []  # (stem, status, flags-mask)
    for op, info in sorted(meta["opcodes"].items()):
        if "reg" in info:
            for r, sub in sorted(info["reg"].items()):
                entries.append((f"{op}.{r}", sub.get("status", "?"), sub.get("flags-mask", 0xFFFF)))
        else:
            entries.append((op, info.get("status", "?"), info.get("flags-mask", 0xFFFF)))

    with open(os.path.join(root, "meta.txt"), "w") as m:
        for stem, status, mask in entries:
            m.write(f"{stem} {status} {mask}\n")

    for stem, status, _ in entries:
        if status in ("prefix",):
            continue
        out = os.path.join(root, stem + ".txt")
        if os.path.exists(out):
            continue
        try:
            raw = fetch(BASE + stem + ".json.gz")
        except urllib.error.HTTPError as e:
            print(f"{stem}: {e.code}, skipped")
            continue
        tests = json.loads(gzip.decompress(raw))
        n = write_tests(out, tests, limit)
        print(f"{stem}: {n} tests")

    fetch_286(limit)


if __name__ == "__main__":
    main()
