#!/usr/bin/env python3
"""Link the sibling app checkouts into apps/ so the suite workspace sees them.

After the repository split each app lives in its own checkout next to
warp-suite. This script creates `apps/<name>` as a symlink (POSIX) or a
junction (Windows) to `<source>/<name>`, so the root workspace's `apps/*`
member glob resolves the app crates and the local `[patch]` table can point
the kit git dependencies at `kits/`.

The app names come from `suite.toml` when it exists, otherwise from
`apps.toml`. A real directory at `apps/<name>` is never removed: that is the
pre-split layout, and it is a signal that the caller is running this too early.

Usage:
  scripts/dev/link-apps.sh [--source DIR] [--check] [--dry-run]
"""

from __future__ import annotations

import argparse
import os
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
APPS = ROOT / "apps"


def app_names() -> list[str]:
    for name in ("suite.toml", "apps.toml"):
        path = ROOT / name
        if not path.is_file():
            continue
        data = tomllib.loads(path.read_text(encoding="utf-8"))
        apps = data.get("apps")
        if isinstance(apps, dict) and apps:
            return sorted(apps)
    raise SystemExit("link-apps: neither suite.toml nor apps.toml lists any apps")


def remove_link(link: Path) -> None:
    if link.is_symlink():
        link.unlink()
    elif os.name == "nt" and link.is_dir():
        subprocess.run(["cmd", "/c", "rmdir", str(link)], check=True)
    else:
        raise SystemExit(f"link-apps: {link} is not a symlink; move it by hand")


def create_link(link: Path, target: Path, source: Path) -> None:
    if os.name == "nt":
        subprocess.run(
            ["cmd", "/c", "mklink", "/J", str(link), str(target)],
            check=True,
            capture_output=True,
            text=True,
        )
        return
    # Keep the default sibling layout relative, so moving the whole
    # Developments/ tree keeps the links working.
    destination = (
        os.path.relpath(target, start=link.parent)
        if source.resolve() == ROOT.parent.resolve()
        else str(target)
    )
    os.symlink(destination, link, target_is_directory=True)


def describe(link: Path) -> str:
    if link.is_symlink():
        return os.readlink(link)
    if os.name == "nt" and link.is_dir():
        return "junction"
    return "missing"


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--source",
        type=Path,
        default=ROOT.parent,
        help="directory holding the app checkouts (default: the directory above warp-suite)",
    )
    parser.add_argument("--check", action="store_true", help="report the links, change nothing")
    parser.add_argument("--dry-run", action="store_true", help="print the planned links only")
    args = parser.parse_args(argv)

    source = args.source.resolve()
    if not source.is_dir():
        raise SystemExit(f"link-apps: source directory {source} does not exist")

    APPS.mkdir(exist_ok=True)
    errors = 0
    for name in app_names():
        target = source / name
        link = APPS / name
        wanted = (target / "Cargo.toml").is_file()
        if not wanted:
            print(f"skip   {name}: no Cargo.toml at {target}")
            errors += args.check
            continue

        if link.is_symlink() or (os.name == "nt" and link.is_dir()):
            if link.resolve() == target.resolve():
                print(f"ok     {name} -> {describe(link)}")
                continue
            if args.check or args.dry_run:
                print(f"wrong  {name} -> {describe(link)} (want {target})")
                errors += 1
                continue
            remove_link(link)

        if link.exists():
            if args.check:
                # The pre-split layout: the app source is still in this repo.
                # That is not a broken link, it is the transition state.
                print(f"pre-split {name}: apps/{name} is a real directory")
            else:
                print(f"error  {name}: apps/{name} is a real directory; move it away first")
                errors += 1
            continue

        if args.check or args.dry_run:
            print(f"plan   {name} -> {target}")
            continue

        create_link(link, target, source)
        print(f"linked {name} -> {target}")

    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
