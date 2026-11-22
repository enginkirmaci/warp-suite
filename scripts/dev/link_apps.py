#!/usr/bin/env python3
"""Link the sibling app checkouts into apps/ so the suite workspace sees them.

After the repository split each app lives in its own checkout next to
warp-suite. This script creates `apps/<name>` as a symlink (POSIX) or a
junction (Windows) to `<source>/<name>`, so the root workspace's `apps/*`
member glob resolves the app crates and the local `[patch]` table can point
the kit git dependencies at `kits/`.

It also points each app checkout's own `target/` at the suite's shared build
cache, so a standalone `cargo build` in `<app>/` and a build through
`apps/<app>` reuse one dependency tree. Without that link the gpui graph is
compiled once per checkout, which is the whole reason the suite has a single
target directory. The link is what `cargo clean` in an app removes, so this
script is also the repair path after one.

The app names come from `suite.toml` when it exists, otherwise from
`apps.toml`. A real directory at `apps/<name>` is never removed: that is the
pre-split layout, and it is a signal that the caller is running this too early.
A real `target/` directory is build output, so `--replace-target` moves it
aside and links the cache instead; without that flag it is only reported.

Usage:
  scripts/dev/link-apps.sh [--source DIR] [--check] [--dry-run]
                           [--target-dir DIR] [--replace-target]
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


def default_target_dir() -> Path:
    """The suite's shared build cache.

    `.build/target` is the committed path (`.cargo/config.toml` sends every
    suite build there) and on a developer machine it is itself a symlink into
    a fast local disk. Resolving it means the app links point at the real
    cache, so a `cargo clean` in warp-suite cannot strand them on a dangling
    link.
    """
    build_target = ROOT / ".build" / "target"
    if build_target.exists():
        return build_target.resolve()
    return build_target


KIT_CONFIG = """\
# Machine-local: build this app against the sibling warp-suite kits/ checkout.
#
# The app's Cargo.toml pins the warpui-kit crates by git tag, which is what CI
# and the release workflow build. On a developer machine that means a kit edit
# is invisible here until it is tagged. Cargo's `paths` override swaps each kit
# for the local checkout without touching Cargo.toml or Cargo.lock: the lock
# keeps its git sources, so `cargo build --locked` in CI is unaffected.
#
# This file is gitignored on purpose. CI has no sibling warp-suite checkout, so
# a committed copy would make every CI build fail on a dangling path.
#
# The effect matches a build through warp-suite/apps/<app>: same kit sources,
# same fingerprints, so the two share the compiled kit artifacts in the suite's
# target directory. Remove this file to build against the pinned tag again.
paths = [
{paths}
]
"""


def kit_paths() -> list[str]:
    """The kit crates this suite owns, as paths relative to an app checkout."""
    kits = ROOT / "kits"
    if not kits.is_dir():
        return []
    return [f"../warp-suite/kits/{d.name}" for d in sorted(kits.iterdir()) if (d / "Cargo.toml").is_file()]


def link_kit_config(app_dir: Path, *, check: bool, dry_run: bool) -> tuple[str, bool]:
    """Write the gitignored `.cargo/config.toml` that points kits at kits/.

    Returns (message, is_error). The file is machine-local: CI never has it, so
    a missing one is reported but is not an error.
    """
    paths = kit_paths()
    if not paths:
        return f"skip   {app_dir.name}/.cargo/config.toml: no kits/ in this checkout", False

    config = app_dir / ".cargo" / "config.toml"
    body = KIT_CONFIG.format(paths="\n".join(f'    "{p}",' for p in paths))

    if config.is_file() and config.read_text(encoding="utf-8") == body:
        return f"ok     {app_dir.name}/.cargo/config.toml -> kits/", False
    if check or dry_run:
        return f"plan   {app_dir.name}/.cargo/config.toml -> kits/", False

    config.parent.mkdir(exist_ok=True)
    config.write_text(body, encoding="utf-8")
    return f"linked {app_dir.name}/.cargo/config.toml -> kits/", False


def link_target_dir(
    app_dir: Path,
    target_dir: Path,
    *,
    check: bool,
    dry_run: bool,
    replace: bool,
) -> tuple[str, bool]:
    """Point `<app>/target` at the shared cache.

    Returns (message, is_error). A real `target/` is build output, not source:
    it is reported, and only moved aside with --replace-target.
    """
    link = app_dir / "target"
    wanted = target_dir.resolve()

    if link.is_symlink():
        if link.resolve() == wanted:
            return f"ok     {app_dir.name}/target -> {describe(link)}", False
        if check or dry_run:
            return f"wrong  {app_dir.name}/target -> {describe(link)} (want {wanted})", True
        link.unlink()
    elif link.exists():
        # A real directory: the pre-link layout, or a build after a cargo clean.
        if not replace:
            return (
                f"real   {app_dir.name}/target is a real directory "
                f"(rerun with --replace-target to link the cache)",
                check,
            )
        if check or dry_run:
            return f"plan   {app_dir.name}/target: move aside, link {wanted}", False
        aside = link.with_name(f"target.old-{os.getpid()}")
        link.rename(aside)
        print(f"moved  {app_dir.name}/target -> {aside.name} (delete it once a build passes)")

    if check or dry_run:
        return f"plan   {app_dir.name}/target -> {wanted}", False

    os.symlink(str(wanted), link, target_is_directory=True)
    return f"linked {app_dir.name}/target -> {wanted}", False


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
    parser.add_argument(
        "--target-dir",
        type=Path,
        default=None,
        help="shared build cache the app target/ links point at "
        "(default: warp-suite/.build/target, resolved)",
    )
    parser.add_argument(
        "--replace-target",
        action="store_true",
        help="move a real <app>/target aside and link the shared cache instead",
    )
    args = parser.parse_args(argv)

    source = args.source.resolve()
    if not source.is_dir():
        raise SystemExit(f"link-apps: source directory {source} does not exist")

    target_dir = (args.target_dir or default_target_dir()).resolve()
    if not target_dir.is_dir():
        raise SystemExit(f"link-apps: shared target directory {target_dir} does not exist")

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

        # The apps/ link and the app's own target/ link are independent: a
        # checkout can be linked into the workspace while its target/ is still
        # a real directory (or the reverse, after a cargo clean). Report and
        # fix each on its own, so one problem does not hide the other.
        if link.is_symlink() or (os.name == "nt" and link.is_dir()):
            if link.resolve() == target.resolve():
                print(f"ok     {name} -> {describe(link)}")
            elif args.check or args.dry_run:
                print(f"wrong  {name} -> {describe(link)} (want {target})")
                errors += 1
            else:
                remove_link(link)
                create_link(link, target, source)
                print(f"linked {name} -> {target}")
        elif link.exists():
            if args.check:
                # The pre-split layout: the app source is still in this repo.
                # That is not a broken link, it is the transition state.
                print(f"pre-split {name}: apps/{name} is a real directory")
            else:
                print(f"error  {name}: apps/{name} is a real directory; move it away first")
                errors += 1
        elif args.check or args.dry_run:
            print(f"plan   {name} -> {target}")
        else:
            create_link(link, target, source)
            print(f"linked {name} -> {target}")

        message, is_error = link_target_dir(
            target,
            target_dir,
            check=args.check,
            dry_run=args.dry_run,
            replace=args.replace_target,
        )
        print(message)
        errors += is_error

        message, is_error = link_kit_config(
            target,
            check=args.check,
            dry_run=args.dry_run,
        )
        print(message)
        errors += is_error

    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
