#!/usr/bin/env python3
"""Check that every suite manifest follows the one shape.

Run from anywhere: python3 scripts/check-manifests.py

The rules are the ones in docs/agents/gpui-app-recipe.md: package fields are
inherited from [workspace.package], shared dependencies and their paths come
from [workspace.dependencies], and the tables appear in one order.
"""

from __future__ import annotations

import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ROOT_MANIFEST = ROOT / "Cargo.toml"

TABLE_ORDER = [
    "package",
    "lib",
    "bin",
    "features",
    "dependencies",
    "dev-dependencies",
    "build-dependencies",
    "target",
]

DEP_TABLES = ("dependencies", "dev-dependencies", "build-dependencies")


def load(path: Path) -> dict:
    with path.open("rb") as f:
        return tomllib.load(f)


def normalize(dep) -> dict:
    """A dependency is a bare version string or a table."""
    return {"version": dep} if isinstance(dep, str) else dict(dep)


def rel(path: Path) -> str:
    return path.relative_to(ROOT).as_posix()


def main() -> int:
    errors: list[str] = []
    root = load(ROOT_MANIFEST)
    workspace = root.get("workspace", {})
    package_fields = workspace.get("package", {})
    workspace_deps = workspace.get("dependencies", {})

    for key in ("license", "repository"):
        if key not in package_fields:
            errors.append(f"{rel(ROOT_MANIFEST)}: [workspace.package] has no {key}")

    expected = {Path(m).as_posix() for m in workspace.get("members", [])}
    on_disk: set[str] = set()
    for pattern in (
        "apps/*/Cargo.toml",
        "apps/*/crates/*/Cargo.toml",
        "android-apps/*/bridge/Cargo.toml",
        "kits/*/Cargo.toml",
    ):
        for path in ROOT.glob(pattern):
            on_disk.add(rel(path.parent))
    for extra in sorted(on_disk - expected):
        errors.append(f"{extra}: manifest on disk but not a workspace member")
    for missing in sorted(expected - on_disk):
        errors.append(f"{missing}: workspace member has no Cargo.toml")

    used: set[str] = set()

    def check_dep_table(where: str, table: dict) -> None:
        for name, value in table.items():
            dep = normalize(value)
            used.add(name)
            if "path" in dep:
                errors.append(
                    f"{where}: `{name}` carries a path; move it to [workspace.dependencies]"
                )
            if name in workspace_deps:
                if dep.get("workspace") is not True:
                    errors.append(f"{where}: `{name}` must be `{{ workspace = true }}`")
                for key in ("version", "path", "package"):
                    if key in dep:
                        errors.append(
                            f"{where}: `{name}` repeats `{key}` from [workspace.dependencies]"
                        )
            elif "version" not in dep:
                errors.append(f"{where}: `{name}` is local but has no version")

    for member in sorted(expected):
        path = ROOT / member / "Cargo.toml"
        if not path.is_file():
            continue
        doc = load(path)
        where = rel(path)

        order = [key for key in doc if key in TABLE_ORDER]
        if order != [key for key in TABLE_ORDER if key in doc]:
            errors.append(f"{where}: tables out of order: {' '.join(order)}")
        for key in doc:
            if key not in TABLE_ORDER:
                errors.append(f"{where}: unexpected top-level table [{key}]")

        package = doc.get("package", {})
        for key in ("version", "edition"):
            field = package.get(key)
            if not (isinstance(field, dict) and field.get("workspace") is True):
                errors.append(f"{where}: package.{key} must be `{key}.workspace = true`")
        description = package.get("description")
        if not isinstance(description, str) or not description.strip():
            errors.append(f"{where}: package.description is missing")
        if "lib" in doc or "bin" in doc:
            errors.append(
                f"{where}: a target table is redundant; cargo names the lib and "
                "bin from the package"
            )

        if member.startswith("kits/"):
            if "publish" in package:
                errors.append(f"{where}: a kit must stay publishable (no publish = false)")
            if package.get("license") != "MIT":
                errors.append(f'{where}: a kit must set license = "MIT"')
            repository = package.get("repository")
            if not (isinstance(repository, dict) and repository.get("workspace") is True):
                errors.append(f"{where}: a kit must set repository.workspace = true")
        else:
            if package.get("publish") is not False:
                errors.append(f"{where}: an app must set publish = false")
            license_field = package.get("license")
            if not (
                isinstance(license_field, dict)
                and license_field.get("workspace") is True
            ):
                errors.append(f"{where}: an app must set license.workspace = true")

        for table in DEP_TABLES:
            if table in doc:
                check_dep_table(f"{where}: [{table}]", doc[table])
        for target, tables in doc.get("target", {}).items():
            for table in DEP_TABLES:
                if table in tables:
                    check_dep_table(
                        f"{where}: [target.'{target}'.{table}]", tables[table]
                    )

    for name, dep in workspace_deps.items():
        dep = normalize(dep)
        path = dep.get("path")
        if path and not (ROOT / path).is_dir():
            errors.append(
                f"{rel(ROOT_MANIFEST)}: workspace dependency `{name}` points at missing {path}"
            )
        if name not in used:
            errors.append(f"{rel(ROOT_MANIFEST)}: workspace dependency `{name}` is unused")

    if errors:
        print("\n".join(errors), file=sys.stderr)
        print(f"\n{len(errors)} manifest problem(s)", file=sys.stderr)
        return 1
    print(f"manifests ok ({len(expected)} members)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
