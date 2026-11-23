#!/usr/bin/env python3
"""Check that every documentation pointer still resolves.

Run from anywhere: python3 scripts/meta/check-docs.py

Two gates, one failure class: a pointer to something that moved. Neither
crashes anything — the text just stops being true, and a person reading the
file cannot see it.

  links     a relative markdown link in AGENTS.md or docs/**/*.md whose target
            is not on disk. The suite's apps reach the suite's guides through
            `../warp-suite/...`, which resolves from the app's own checkout and
            from `apps/<name>` alike; `../../AGENTS.md` resolved from neither.

  symlinks  an entry under `.agents/skills/*/references/` whose target is gone.
            A skill's body loads from its own `SKILL.md`, so a dangling
            reference fails only when an agent reaches for it mid-task.

The app checkouts are read through `apps/<name>`, the symlinks
`scripts/dev/link-apps.sh` creates. A machine with no links checks the suite
alone and says so, which is what CI does.
"""

from __future__ import annotations

import os
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
APPS = ROOT / "apps"

# `[label](target)` and `![alt](target)`. The target stops at whitespace, so a
# title (`[x](path "title")`) still resolves on its path.
LINK = re.compile(r"!?\[[^\]]*\]\(([^)\s]+)\)")

# A link that is not a path: a URL, a mailto, or a same-page anchor.
EXTERNAL = ("http://", "https://", "mailto:", "#")

# The repositories whose prose is checked: the suite plus every linked app.
SUITE = ROOT


def linked_apps() -> list[Path]:
    """The app checkouts reachable through `apps/`, in suite.toml order."""
    if not APPS.is_dir():
        return []
    return sorted(p for p in APPS.iterdir() if p.is_dir())


def markdown_files(repo: Path) -> list[Path]:
    """`AGENTS.md` plus every markdown file under `docs/`."""
    files = [repo / "AGENTS.md"]
    docs = repo / "docs"
    if docs.is_dir():
        files.extend(sorted(docs.rglob("*.md")))
    return [p for p in files if p.is_file()]


def check_links(repo: Path) -> list[str]:
    """Every relative link in the repo's prose that does not resolve."""
    problems: list[str] = []
    for path in markdown_files(repo):
        # The kernel resolves a symlinked directory before applying `..`, so a
        # link written for the app's own checkout must be resolved from the
        # checkout's real path, not from `apps/<name>`.
        base = Path(os.path.realpath(path.parent))
        text = path.read_text(encoding="utf-8", errors="replace")
        for target in LINK.findall(text):
            if target.startswith(EXTERNAL):
                continue
            # A fragment is not a path: `path#anchor` resolves on `path`.
            target = target.split("#", 1)[0]
            if not target:
                continue
            resolved = Path(os.path.normpath(base / target))
            if not resolved.exists():
                problems.append(f"{rel(path)}: link target missing: {target}")
    return problems


def check_symlinks(repo: Path) -> list[str]:
    """Every skill reference that points at a file that is gone."""
    problems: list[str] = []
    skills = repo / ".agents" / "skills"
    if not skills.is_dir():
        return problems
    for reference in sorted(skills.glob("*/references/*")):
        if not reference.exists():
            problems.append(
                f"{rel(reference)}: dangling reference -> {os.readlink(reference)}"
            )
    return problems


def rel(path: Path) -> str:
    try:
        return path.relative_to(ROOT).as_posix()
    except ValueError:
        return path.as_posix()


def main() -> int:
    repos = [SUITE, *linked_apps()]
    problems: list[str] = []
    for repo in repos:
        problems.extend(check_links(repo))
        problems.extend(check_symlinks(repo))

    if problems:
        print("\n".join(problems), file=sys.stderr)
        print(f"\n{len(problems)} documentation problem(s)", file=sys.stderr)
        return 1

    apps = len(repos) - 1
    scope = f"the suite and {apps} linked app(s)" if apps else "the suite alone"
    print(f"documentation ok ({scope})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
