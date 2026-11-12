#!/usr/bin/env python3
"""suite.toml + app.toml helper shared by the warp-suite scripts and CI.

The suite pins apps in suite.toml; each app repository carries its own build
knobs in app.toml. The symlink under apps/<name> is what makes the second file
reachable from this checkout.

Usage:
  manifest.py list                 print one pinned app name per line
  manifest.py pin <app> <key>      print one field of [apps.<app>] in suite.toml
  manifest.py get <app> <key>      print one field of apps/<app>/app.toml
"""

import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
SUITE = ROOT / "suite.toml"
APPS = ROOT / "apps"


def suite() -> dict:
    with SUITE.open("rb") as f:
        return tomllib.load(f).get("apps", {})


def app_names() -> list[str]:
    return list(suite())


def pin(name: str, key: str):
    if name == "kits":
        data = tomllib.loads(SUITE.read_text(encoding="utf-8")).get("kits", {})
        if key not in data:
            sys.exit(f"[kits] has no {key}")
        return data[key]
    known = suite()
    if name not in known:
        sys.exit(f"suite.toml has no [apps.{name}] (known: {', '.join(known) or 'none'})")
    if key not in known[name]:
        sys.exit(f"[apps.{name}] has no {key}")
    return known[name][key]


def get(name: str, key: str):
    known = app_names()
    if name not in known:
        sys.exit(f"suite.toml has no [apps.{name}] (known: {', '.join(known) or 'none'})")
    path = APPS / name / "app.toml"
    if not path.is_file():
        sys.exit(f"apps/{name}/app.toml is missing; run scripts/dev/link-apps.sh")
    data = tomllib.loads(path.read_text(encoding="utf-8")).get("app", {})
    if key not in data:
        sys.exit(f"apps/{name}/app.toml has no app.{key}")
    value = data[key]
    if isinstance(value, bool):
        # TOML spelling: shell consumers compare against lowercase "true".
        return "true" if value else "false"
    return value


def main() -> None:
    argv = sys.argv[1:]
    if argv == ["list"]:
        for name in app_names():
            print(name)
        return
    if len(argv) == 3 and argv[0] == "pin":
        print(pin(argv[1], argv[2]))
        return
    if len(argv) == 3 and argv[0] == "get":
        print(get(argv[1], argv[2]))
        return
    sys.exit(__doc__)


if __name__ == "__main__":
    main()
