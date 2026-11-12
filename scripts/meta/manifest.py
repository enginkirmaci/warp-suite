#!/usr/bin/env python3
"""apps.toml helper shared by the warp-suite scripts and CI.

apps.toml sections are tiny and hand-maintained; tomllib covers the reads.

Usage:
  manifest.py list                 print one app name per line
  manifest.py get <app> <key>      print one field of [apps.<app>]
"""

import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
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
    value = data[key]
    if isinstance(value, bool):
        # TOML spelling: shell consumers compare against lowercase "true".
        return "true" if value else "false"
    return value


def main():
    argv = sys.argv[1:]
    if argv == ["list"]:
        for name in apps():
            print(name)
        return
    if len(argv) == 3 and argv[0] == "get":
        print(get(argv[1], argv[2]))
        return
    sys.exit(__doc__)


if __name__ == "__main__":
    main()
