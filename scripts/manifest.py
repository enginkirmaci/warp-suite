#!/usr/bin/env python3
"""apps.toml helper shared by the warp-suite scripts and CI.

apps.toml sections are tiny and hand-maintained; tomllib (read-only) covers
reads and set-ref does minimal text surgery for writes.

Usage:
  manifest.py list                    print "name ref repo" per app
  manifest.py get <app> <key>         print one field of [apps.<app>]
  manifest.py set-ref <app> <ref>     rewrite the ref line under [apps.<app>]
"""

import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MANIFEST = ROOT / "apps.toml"


def apps():
    with MANIFEST.open("rb") as f:
        return tomllib.load(f).get("apps", {})


def get(name, key):
    known = apps()
    if name not in known:
        sys.exit(f"apps.toml has no [apps.{name}] (known: {', '.join(known) or 'none'})")
    data = known[name]
    if key not in data:
        sys.exit(f"[apps.{name}] has no {key}")
    return data[key]


def set_ref(name, ref):
    lines = MANIFEST.read_text().splitlines(keepends=True)
    header = f"[apps.{name}]"
    inside = False
    done = False
    for i, line in enumerate(lines):
        stripped = line.strip()
        if stripped.startswith("["):
            inside = stripped == header
            continue
        if inside and not done and stripped.split("=", 1)[0].strip() == "ref":
            lines[i] = f'ref = "{ref}"\n'
            done = True
    if not done:
        sys.exit(f"no ref line under [apps.{name}]")
    MANIFEST.write_text("".join(lines))


def main():
    argv = sys.argv[1:]
    if argv == ["list"]:
        for name, data in apps().items():
            print(name, data["ref"], data["repo"])
        return
    if len(argv) == 3 and argv[0] == "get":
        print(get(argv[1], argv[2]))
        return
    if len(argv) == 3 and argv[0] == "set-ref":
        set_ref(argv[1], argv[2])
        return
    sys.exit(__doc__)


if __name__ == "__main__":
    main()
