#!/usr/bin/env python3
"""The suite's one version writer.

`version.toml` is the source of truth: `version` (MAJOR.MINOR.PATCH) and
`build` (a monotonic build counter). Every commit bumps PATCH through the
pre-commit hook; every run of scripts/build.sh or scripts/build.ps1 bumps
build. Cargo.toml and Cargo.lock are derived from this file, never edited by
hand for a version.

Usage:
  version.py show [--json] [--next]     print the build facts
  version.py bump {patch|minor|major}   change version and sync Cargo (stages)
  version.py build bump                 increment the build counter
  version.py sync                       rewrite Cargo.toml/Cargo.lock from version.toml
  version.py check [--history]          fail when a derived copy drifts
  version.py pre-commit [--dry-run]     the hook: bump PATCH and stage
  version.py install-hooks              point core.hooksPath at .githooks
"""

from __future__ import annotations

import json
import os
import re
import shutil
import subprocess
import sys
import tomllib
from contextlib import contextmanager
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
VERSION_FILE = ROOT / "version.toml"
CARGO_FILE = ROOT / "Cargo.toml"
LOCK_FILE = ROOT / "Cargo.lock"
HOOKS_DIR = ".githooks"

SEMVER = re.compile(
    r"^(?P<major>\d+)\.(?P<minor>\d+)\.(?P<patch>\d+)"
    r"(?:-(?P<pre>[0-9A-Za-z.-]+))?$"
)


# --------------------------------------------------------------------------- #
# small helpers


def fail(message: str) -> "NoReturn":
    raise SystemExit(f"version: {message}")


def git(*args: str, check: bool = True) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["git", *args],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=check,
    )


def in_git() -> bool:
    return git("rev-parse", "--git-dir", check=False).returncode == 0


def read_facts() -> dict:
    if not VERSION_FILE.is_file():
        fail(f"{VERSION_FILE} is missing")
    data = tomllib.loads(VERSION_FILE.read_text(encoding="utf-8"))
    version = str(data.get("version", "")).strip()
    build = data.get("build", 0)
    if not SEMVER.match(version):
        fail(f"version.toml version {version!r} is not MAJOR.MINOR.PATCH")
    if not isinstance(build, int) or build < 0:
        fail(f"version.toml build {build!r} is not a non-negative integer")
    return {"version": version, "build": build}


@contextmanager
def version_lock():
    """Hold an exclusive lock while a writer reads and rewrites version.toml."""

    fh = open(VERSION_FILE, "r+", encoding="utf-8")
    try:
        try:
            import fcntl

            fcntl.flock(fh.fileno(), fcntl.LOCK_EX)
            unlock = lambda: fcntl.flock(fh.fileno(), fcntl.LOCK_UN)
        except ImportError:  # Windows: one byte is enough, every writer agrees
            import msvcrt

            fh.seek(0)
            msvcrt.locking(fh.fileno(), msvcrt.LK_LOCK, 1)

            def unlock():
                fh.seek(0)
                msvcrt.locking(fh.fileno(), msvcrt.LK_UNLCK, 1)

        try:
            yield fh
        finally:
            unlock()
    finally:
        fh.close()


def write_version_text(text: str, version: str, build: int) -> str:
    text, n = re.subn(
        r'(?m)^(\s*version\s*=\s*")[^"]*(")',
        rf"\g<1>{version}\2",
        text,
        count=1,
    )
    if n != 1:
        fail("version.toml has no `version = ` line")
    text, n = re.subn(r"(?m)^(\s*build\s*=\s*)\d+", rf"\g<1>{build}", text, count=1)
    if n != 1:
        fail("version.toml has no `build = ` line")
    return text


def mutate_version(change):
    """Read version/build under the lock, let `change` return the new pair."""

    with version_lock() as fh:
        raw = fh.read()
        data = tomllib.loads(raw)
        version = str(data["version"]).strip()
        build = int(data.get("build", 0))
        new_version, new_build = change(version, build)
        fh.seek(0)
        fh.truncate()
        fh.write(write_version_text(raw, new_version, new_build))
        fh.flush()
        os.fsync(fh.fileno())
    return new_version, new_build


def bump_semver(version: str, kind: str) -> str:
    match = SEMVER.match(version)
    assert match is not None
    major, minor, patch = (int(match.group(k)) for k in ("major", "minor", "patch"))
    pre = match.group("pre")
    if pre:
        fail(f"cannot bump {version}: drop the pre-release suffix first")
    if kind == "patch":
        patch += 1
    elif kind == "minor":
        minor += 1
    elif kind == "major":
        major += 1
    else:
        fail(f"unknown bump kind {kind!r}")
    return f"{major}.{minor}.{patch}"


# --------------------------------------------------------------------------- #
# git facts


def git_facts(version: str) -> dict:
    if not in_git():
        return {"commit": "unknown", "channel": "dev", "released": ""}

    head = git("rev-parse", "--short=7", "HEAD").stdout.strip()
    dirty = any(
        line and not line.endswith(" version.toml")
        for line in git("status", "--porcelain", "--untracked-files=normal").stdout.splitlines()
    )
    if dirty:
        head = f"{head}-dirty"

    tags = git("tag", "--points-at", "HEAD", "--list", "v*").stdout.split()
    # CI may check a tag out without fetching every tag ref; the dispatch ref
    # identifies the same tag.
    tagged = f"v{version}" in tags or os.environ.get("GITHUB_REF") == f"refs/tags/v{version}"
    if tagged:
        pre = SEMVER.match(version).group("pre") if SEMVER.match(version) else None
        if pre:
            channel = next((name for name in ("alpha", "beta", "rc") if name in pre), "pre")
        else:
            channel = "stable"
        released = git("log", "-1", "--format=%cs", "HEAD").stdout.strip()
    else:
        channel = "dev"
        released = ""
    return {"commit": head, "channel": channel, "released": released}


def facts(version: str | None = None) -> dict:
    data = read_facts()
    if version is None:
        version = data["version"]
    return {"version": version, "build": data["build"], **git_facts(version)}


# --------------------------------------------------------------------------- #
# Cargo sync


def find_cargo() -> str:
    cargo = shutil.which("cargo")
    if cargo:
        return cargo
    local = Path.home() / ".cargo" / "bin" / ("cargo.exe" if os.name == "nt" else "cargo")
    if local.is_file():
        return str(local)
    fail("cargo is not on PATH; a version bump must refresh Cargo.lock")


def sync_cargo(version: str) -> None:
    text = CARGO_FILE.read_text(encoding="utf-8")
    lines = text.splitlines(keepends=True)
    in_section = False
    replaced = False
    for index, line in enumerate(lines):
        stripped = line.strip()
        if stripped.startswith("[") and stripped.endswith("]"):
            in_section = stripped == "[workspace.package]"
            continue
        if in_section and re.match(r"^version\s*=", stripped):
            lines[index] = re.sub(r'"[^"]*"', f'"{version}"', line, count=1)
            replaced = True
            break
    if not replaced:
        fail("Cargo.toml has no version under [workspace.package]")
    CARGO_FILE.write_text("".join(lines), encoding="utf-8")

    cargo = find_cargo()
    for extra in (["--offline"], []):
        run = subprocess.run(
            [cargo, "update", "--workspace", *extra],
            cwd=ROOT,
            text=True,
            capture_output=True,
        )
        if run.returncode == 0:
            break
    else:
        fail(f"cargo update --workspace failed:\n{run.stderr.strip()}")

    check_locked()


def check_locked() -> dict:
    cargo = find_cargo()
    run = subprocess.run(
        [cargo, "metadata", "--locked", "--no-deps", "--format-version", "1"],
        cwd=ROOT,
        text=True,
        capture_output=True,
    )
    if run.returncode != 0:
        fail(f"Cargo.lock is stale; run `cargo update --workspace`:\n{run.stderr.strip()}")
    return json.loads(run.stdout)


def stage(paths: list[Path]) -> None:
    if not in_git():
        return
    relative = [str(path.relative_to(ROOT)) for path in paths]
    run = git("add", "--", *relative, check=False)
    if run.returncode != 0:
        fail(f"git add failed: {run.stderr.strip()}")


# --------------------------------------------------------------------------- #
# commands


def cmd_show(argv: list[str]) -> int:
    as_json = "--json" in argv
    next_only = "--next" in argv
    current = read_facts()
    version = current["version"]
    if next_only:
        version = bump_semver(version, "patch")
    data = facts(version)
    if next_only:
        print(data["version"])
        return 0
    if as_json:
        print(json.dumps(data, sort_keys=True))
    else:
        changed = data["commit"]
        released = f" · released {data['released']}" if data["released"] else ""
        print(f"{data['version']} build {data['build']} · {data['channel']} · {changed}{released}")
    return 0


def cmd_bump(argv: list[str]) -> int:
    if not argv or argv[0] not in {"patch", "minor", "major"}:
        fail("usage: version.py bump {patch|minor|major}")
    kind = argv[0]
    no_stage = "--no-stage" in argv
    old, _ = read_facts()
    new, _ = mutate_version(lambda version, build: (bump_semver(version, kind), build))
    sync_cargo(new)
    if not no_stage:
        stage([VERSION_FILE, CARGO_FILE, LOCK_FILE])
    print(f"version: {old} -> {new}")
    return 0


def cmd_build(argv: list[str]) -> int:
    if argv != ["bump"]:
        fail("usage: version.py build bump")
    _, build = mutate_version(lambda version, current: (version, current + 1))
    print(build)
    return 0


def cmd_sync(argv: list[str]) -> int:
    if argv:
        fail("usage: version.py sync")
    current = read_facts()
    sync_cargo(current["version"])
    print(f"version: {current['version']}")
    return 0


def cmd_pre_commit(argv: list[str]) -> int:
    dry_run = "--dry-run" in argv
    if os.environ.get("WARP_VERSION_SKIP") == "1":
        print("version: skipped (WARP_VERSION_SKIP=1)")
        return 0
    current = read_facts()
    new = bump_semver(current["version"], "patch")
    if dry_run:
        print(f"version: {current['version']} -> {new}; would stage version.toml Cargo.toml Cargo.lock")
        return 0
    mutate_version(lambda version, build: (new, build))
    sync_cargo(new)
    stage([VERSION_FILE, CARGO_FILE, LOCK_FILE])
    print(f"version: {current['version']} -> {new} (build {current['build']})")
    return 0


def workspace_version_errors(data: dict) -> list[str]:
    version = data["version"]
    errors: list[str] = []
    metadata = check_locked()
    members = set(metadata.get("workspace_members", []))
    packages = [p for p in metadata.get("packages", []) if p.get("id") in members]
    if not packages:
        errors.append("cargo metadata returned no workspace member")
    for package in packages:
        if package.get("version") != version:
            errors.append(
                f"{package.get('manifest_path')}: {package.get('name')} "
                f"is {package.get('version')}, version.toml is {version}"
            )
    for package in packages:
        manifest = Path(package["manifest_path"])
        text = manifest.read_text(encoding="utf-8")
        if re.search(r"(?m)^version\s*=\s*\"", text):
            errors.append(f"{manifest.relative_to(ROOT)}: literal version; use `version.workspace = true`")
    return errors


def layout_errors() -> list[str]:
    """Every second place that used to carry a hand-written version."""

    errors: list[str] = []

    for iss in sorted((ROOT / "apps").glob("*/packaging/windows/*.iss")):
        text = iss.read_text(encoding="utf-8")
        relative = iss.relative_to(ROOT)
        if re.search(r'#define\s+MyAppVersion\s+"', text):
            errors.append(f"{relative}: MyAppVersion fallback; require /DMyAppVersion")
        if "#ifndef MyAppVersion" in text and "#error" not in text:
            errors.append(f"{relative}: #ifndef MyAppVersion has no #error")

    gradle = ROOT / "android-apps/warp-android-scribe/android/app/build.gradle.kts"
    gradle_text = gradle.read_text(encoding="utf-8")
    if "version.toml" not in gradle_text:
        errors.append(f"{gradle.relative_to(ROOT)}: does not read version.toml")
    if re.search(r'(?m)^\s*versionName\s*=\s*"', gradle_text):
        errors.append(f"{gradle.relative_to(ROOT)}: literal versionName")
    if re.search(r"(?m)^\s*versionCode\s*=\s*\d+\s*$", gradle_text):
        errors.append(f"{gradle.relative_to(ROOT)}: literal versionCode")

    workflow = ROOT / ".github/workflows/suite-release.yml"
    workflow_text = workflow.read_text(encoding="utf-8")
    if re.search(r"(?m)^\s*default:\s*'0\.\d+\.\d+'", workflow_text):
        errors.append(f"{workflow.relative_to(ROOT)}: hardcoded version default")

    return errors


def history_errors() -> list[str]:
    if not in_git():
        fail("check --history needs a git checkout")
    added = git("log", "--diff-filter=A", "--format=%H", "--", "version.toml").stdout.split()
    if not added:
        fail("version.toml is not committed yet")
    first = added[-1]
    commits = git("rev-list", "--reverse", f"{first}..HEAD").stdout.split()
    errors: list[str] = []
    previous = read_version_at(first)
    for commit in commits:
        current = read_version_at(commit)
        prev_match = SEMVER.match(previous["version"])
        cur_match = SEMVER.match(current["version"])
        if prev_match is None or cur_match is None:
            errors.append(f"{commit[:7]}: version is not MAJOR.MINOR.PATCH")
            previous = current
            continue
        expected_patch = int(prev_match.group("patch")) + 1
        if int(cur_match.group("patch")) != expected_patch:
            errors.append(
                f"{commit[:7]}: patch {cur_match.group('patch')}, expected {expected_patch} "
                f"(parent {previous['version']})"
            )
        prev_line = tuple(int(prev_match.group(k)) for k in ("major", "minor"))
        cur_line = tuple(int(cur_match.group(k)) for k in ("major", "minor"))
        if cur_line < prev_line:
            errors.append(
                f"{commit[:7]}: {current['version']} moves behind {previous['version']}"
            )
        previous = current
    return errors


def read_version_at(commit: str) -> dict:
    run = git("show", f"{commit}:version.toml", check=False)
    if run.returncode != 0:
        fail(f"{commit[:7]} has no version.toml")
    data = tomllib.loads(run.stdout)
    return {"version": str(data["version"]).strip(), "build": int(data.get("build", 0))}


def cmd_check(argv: list[str]) -> int:
    data = read_facts()
    version = data["version"]
    errors: list[str] = []

    match = SEMVER.match(version)
    if match is None:
        errors.append("version.toml version is not MAJOR.MINOR.PATCH")
    elif int(match.group("patch")) >= 100_000:
        errors.append("patch >= 100000: the Android versionCode would collide")
    if data["build"] >= 60_000:
        errors.append("build >= 60000: the Windows FILEVERSION field is about to overflow")

    cargo_version = None
    text = CARGO_FILE.read_text(encoding="utf-8")
    in_section = False
    for line in text.splitlines():
        stripped = line.strip()
        if stripped.startswith("[") and stripped.endswith("]"):
            in_section = stripped == "[workspace.package]"
        elif in_section and stripped.startswith("version"):
            cargo_version = stripped.split("=", 1)[1].strip().strip('"')
            break
    if cargo_version != version:
        errors.append(f"Cargo.toml [workspace.package] version is {cargo_version}, version.toml is {version}")

    errors.extend(workspace_version_errors(data))
    errors.extend(layout_errors())

    if "--history" in argv:
        errors.extend(history_errors())

    if errors:
        for error in errors:
            print(f"version: {error}", file=sys.stderr)
        return 1
    print(f"version: {version} build {data['build']} is consistent")
    return 0


def cmd_install_hooks(argv: list[str]) -> int:
    if argv:
        fail("usage: version.py install-hooks")
    if not in_git():
        fail("not a git checkout")
    hook = ROOT / HOOKS_DIR / "pre-commit"
    if not hook.is_file():
        fail(f"{hook.relative_to(ROOT)} is missing")
    hook.chmod(0o755)
    git("config", "core.hooksPath", HOOKS_DIR)
    print(f"version: core.hooksPath -> {HOOKS_DIR}")
    return 0


COMMANDS = {
    "show": cmd_show,
    "bump": cmd_bump,
    "build": cmd_build,
    "sync": cmd_sync,
    "check": cmd_check,
    "pre-commit": cmd_pre_commit,
    "install-hooks": cmd_install_hooks,
}


def main(argv: list[str]) -> int:
    if not argv or argv[0] not in COMMANDS:
        print(__doc__.strip(), file=sys.stderr)
        return 2
    return COMMANDS[argv[0]](argv[1:])


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
