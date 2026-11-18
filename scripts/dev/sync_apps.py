#!/usr/bin/env python3
"""Fast-forward the app checkouts to the latest main branch.

Each app lives in its own repository and `apps/<name>` is a symlink to the
sibling checkout (`scripts/dev/link-apps.sh`). This script fetches every app's
`origin` and fast-forwards `main`. It never merges, rebases, resets or stashes:
an uncommitted tree, a feature branch or a diverged history is reported and
left alone.

`scripts/dev/refresh.sh` and `scripts/dev/refresh.ps1` wrap this script and
then run the suite build with the installers off.

Usage:
  scripts/dev/sync-apps.sh [--source DIR] [--app NAME]... [--check] [--dry-run]
"""

from __future__ import annotations

import argparse
import subprocess
import sys
import tomllib
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
APPS = ROOT / "apps"
SUITE = ROOT / "suite.toml"
MAIN = "main"


def app_names() -> list[str]:
    """The pinned apps, in suite.toml order."""
    if not SUITE.is_file():
        raise SystemExit(f"sync-apps: {SUITE} is missing")
    data = tomllib.loads(SUITE.read_text(encoding="utf-8"))
    apps = data.get("apps")
    if not isinstance(apps, dict) or not apps:
        raise SystemExit("sync-apps: suite.toml lists no apps")
    return list(apps)


def git(path: Path, *args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["git", "-C", str(path), *args],
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
    )


def git_value(path: Path, *args: str) -> str | None:
    run = git(path, *args)
    value = run.stdout.strip()
    return value if run.returncode == 0 and value else None


def commit_count(path: Path, spec: str) -> int:
    value = git_value(path, "rev-list", "--count", spec)
    try:
        return int(value or 0)
    except ValueError:
        return 0


def app_version(path: Path) -> str | None:
    file = path / "version.toml"
    if not file.is_file():
        return None
    try:
        data = tomllib.loads(file.read_text(encoding="utf-8"))
    except (OSError, tomllib.TOMLDecodeError):
        return None
    version = str(data.get("version", "")).strip()
    return version or None


@dataclass
class Result:
    status: str
    detail: str


def sync_checkout(path: Path, *, check: bool, dry_run: bool) -> Result:
    """Bring one checkout's main up to date, or say why it was left alone."""
    if git_value(path, "rev-parse", "--is-inside-work-tree") != "true":
        return Result("error", "not a git checkout")
    if git_value(path, "remote", "get-url", "origin") is None:
        return Result("error", "no origin remote")

    branch = git_value(path, "symbolic-ref", "--quiet", "--short", "HEAD")
    if branch is None:
        return Result("skipped", "detached HEAD")
    if branch != MAIN:
        return Result("skipped", f"on {branch}; only {MAIN} is refreshed")
    if git(path, "status", "--porcelain", "--untracked-files=no").stdout.strip():
        return Result("skipped", "uncommitted changes")

    if dry_run:
        return Result("plan", f"git fetch --prune origin && git merge --ff-only origin/{MAIN}")

    fetched = git(path, "fetch", "--prune", "--quiet", "origin")
    if fetched.returncode != 0:
        lines = [line for line in fetched.stderr.strip().splitlines() if line.strip()]
        return Result("error", f"fetch failed: {lines[-1] if lines else 'unknown error'}")

    before = git_value(path, "rev-parse", "--short=7", "HEAD") or "?"
    ahead = commit_count(path, f"origin/{MAIN}..HEAD")
    behind = commit_count(path, f"HEAD..origin/{MAIN}")

    if check:
        if behind == 0:
            return Result("ok", f"{before} up to date")
        if ahead:
            return Result("diverged", f"{ahead} ahead, {behind} behind origin/{MAIN}")
        return Result("behind", f"{before}; {behind} behind origin/{MAIN}")

    if behind == 0:
        return Result("ok", f"{before} up to date")
    if ahead:
        # A fast-forward would drop the local commits, so stop before it.
        return Result("diverged", f"{ahead} ahead, {behind} behind origin/{MAIN}")

    merged = git(path, "merge", "--ff-only", f"origin/{MAIN}")
    if merged.returncode != 0:
        lines = [line for line in merged.stderr.strip().splitlines() if line.strip()]
        return Result("error", f"merge failed: {lines[-1] if lines else 'unknown error'}")

    after = git_value(path, "rev-parse", "--short=7", "HEAD") or "?"
    commits = f"{behind} commit{'s' if behind != 1 else ''}"
    return Result("updated", f"{before} -> {after} ({commits})")


def is_link(path: Path) -> bool:
    """Symlink, or a Windows junction (which is_symlink() does not report)."""
    is_junction = getattr(path, "is_junction", None)
    return path.is_symlink() or bool(is_junction and is_junction())


def resolve(name: str, source: Path, *, use_links: bool) -> Path | str:
    """The checkout the workspace builds, or a message explaining the miss."""
    if use_links:
        link = APPS / name
        if is_link(link):
            target = link.resolve()
            if target.is_dir():
                return target
        elif link.exists():
            return (
                f"apps/{name} is a real directory; move it aside and run "
                "scripts/dev/link-apps.sh"
            )

    candidate = source / name
    if candidate.is_dir():
        return candidate.resolve()
    return f"no checkout at {candidate}"


def ensure_links(names: list[str]) -> None:
    """Link a checkout that was cloned after the last link-apps run."""
    missing = []
    for name in names:
        link = APPS / name
        if not is_link(link) or not link.resolve().is_dir():
            missing.append(name)
    if not missing:
        return
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import link_apps

    print(f"sync-apps: linking {', '.join(missing)}")
    link_apps.main([])


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument(
        "--source",
        type=Path,
        default=ROOT.parent,
        help="directory holding the app checkouts (default: the directory above warp-suite)",
    )
    parser.add_argument(
        "--app",
        action="append",
        default=[],
        metavar="NAME",
        help="only these apps (repeatable; default: every app in suite.toml)",
    )
    parser.add_argument("--check", action="store_true", help="fetch and report; change nothing")
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="print the git commands instead of running them",
    )
    args = parser.parse_args(argv)

    if args.check and args.dry_run:
        raise SystemExit("sync-apps: pick one of --check and --dry-run")

    known = app_names()
    selected = args.app or known
    unknown = [name for name in selected if name not in known]
    if unknown:
        raise SystemExit(
            f"sync-apps: unknown app(s): {', '.join(unknown)} (known: {', '.join(known)})"
        )

    source = args.source.resolve()
    if not source.is_dir():
        raise SystemExit(f"sync-apps: source directory {source} does not exist")
    # An explicit --source is authoritative; with the default source the
    # apps/ links are what the workspace builds, so they come first.
    use_links = source == ROOT.parent.resolve()

    width = max(len(name) for name in selected)
    status_width = 8
    failed = 0
    refreshed: list[str] = []

    for name in selected:
        checkout = resolve(name, source, use_links=use_links)
        if isinstance(checkout, str):
            result = Result("error", checkout)
        else:
            result = sync_checkout(checkout, check=args.check, dry_run=args.dry_run)
            if result.status == "updated":
                refreshed.append(name)

        version = app_version(checkout) if isinstance(checkout, Path) else None
        suffix = f" (v{version})" if version else ""
        print(f"{name:<{width}}  {result.status:<{status_width}}  {result.detail}{suffix}")

        if result.status in {"error", "diverged"}:
            failed += 1
        elif args.check and result.status == "behind":
            failed += 1

    if not args.check and not args.dry_run and use_links:
        ensure_links(selected)

    if refreshed:
        print(f"sync-apps: updated {len(refreshed)} of {len(selected)} app(s)")
    if failed:
        print(f"sync-apps: {failed} app(s) need attention", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
