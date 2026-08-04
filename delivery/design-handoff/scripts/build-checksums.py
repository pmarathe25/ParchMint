#!/usr/bin/env python3
"""
Regenerate and verify checksums.sha256 for a ParchMint design handoff package.

Usage:
    python3 build-checksums.py <handoff-dir>            # regenerate file
    python3 build-checksums.py <handoff-dir> --verify   # verify existing file
"""

from __future__ import annotations

import hashlib
import os
import sys


def sha256(path: str) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def all_files(root: str) -> list[str]:
    out = []
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames.sort()
        for name in sorted(filenames):
            rel = os.path.relpath(os.path.join(dirpath, name), root)
            if rel == "checksums.sha256":
                continue
            out.append(rel)
    return out


def main() -> int:
    if len(sys.argv) not in (2, 3):
        print(__doc__)
        return 2
    root = os.path.abspath(sys.argv[1])
    verify = len(sys.argv) == 3 and sys.argv[2] == "--verify"
    cksum = os.path.join(root, "checksums.sha256")
    lines = [f"{sha256(os.path.join(root, rel))}  {rel}" for rel in all_files(root)]
    if not verify:
        with open(cksum, "w", encoding="utf-8") as fh:
            fh.write("\n".join(lines) + "\n")
        print(f"wrote {len(lines)} checksums to {cksum}")
        return 0
    if not os.path.exists(cksum):
        print("no checksums.sha256 present")
        return 1
    with open(cksum, encoding="utf-8") as fh:
        recorded = {}
        for raw in fh.read().splitlines():
            if not raw.strip():
                continue
            h, _, rel = raw.rpartition("  ")
            recorded[rel] = h
    ok = True
    for rel in all_files(root):
        actual = sha256(os.path.join(root, rel))
        if recorded.get(rel) != actual:
            print(f"MISMATCH {rel}: recorded {recorded.get(rel)} actual {actual}")
            ok = False
    extra = set(recorded) - set(all_files(root))
    if extra:
        print("stale entries:", sorted(extra))
        ok = False
    print("ALL CHECKS OK" if ok else "CHECK FAILURES")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())