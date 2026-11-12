# Version and build numbers

Three independent lines, one per kind of repository:

| Line | Source | Tag | Covers |
|---|---|---|---|
| Kit | warp-suite `version.toml` | `kits-v<version>` | the six `warpui-kit-*` crates |
| App | the app repository's `version.toml` | `v<version>` | that app's binary/APK |
| Suite | [`suite.toml`](../../suite.toml) | `v<suite version>` | the pin table: one app tag per app plus the kit tag |

## Rules

- `scripts/meta/version.py` is the only writer in warp-suite; each app
  repository carries its own copy at `scripts/version.py`.
- A pre-commit hook bumps PATCH on every commit in that repository, never
  resets, and keeps that repository's `Cargo.toml` and `Cargo.lock` in step.
- A build-script run bumps that repository's build counter. A release job
  passes the tag's value instead, so one tag produces one build number.
- MINOR/MAJOR move by hand (`version.py bump minor` / `bump major`).
- `WARP_VERSION_SKIP=1 git commit ...` skips the bump for an amend, a rebase or
  an emergency.

## What the facts are

| Fact | Source |
|---|---|
| `version` | the repository's `version.toml` (`CARGO_PKG_VERSION` in the binary) |
| `build` | the repository's build counter |
| `commit` | `git rev-parse --short=7 HEAD`, `-dirty` when the tree differs |
| `channel` | `stable` only when HEAD carries the repository's release tag |
| `released` | the tag's commit date in a tagged build, otherwise `not released` |

## Where the facts appear

| Surface | Shows |
|---|---|
| `Copy version`, `<app> --version` | `Warp Station 0.1.1 (build 7 · d904edc) · dev` |
| About Build card | Version, Build, Commit, Channel, Released, Runtime |
| Windows `FILEVERSION` / `PRODUCTVERSION` | `<version>.<build>`; 16-bit fields |
| Android | `versionName` from the app `version.toml`; `versionCode` derived from it |
| `GET /health` (bridge) | `version`, `build`, `commit`, `channel` |
| `REVISIONS.txt` | the suite pin table: every app tag, its commit and the kit tag |

`kits/warpui-kit-build` reads the app's own `version.toml` through
`CARGO_MANIFEST_DIR`; it never reads warp-suite's. A plain `cargo build` in an
app checkout therefore reports that app's build, not the suite's.

## One-time setup per clone

```sh
python3 scripts/meta/version.py install-hooks   # warp-suite
python3 scripts/version.py install-hooks        # each app repository
```

## Commands

warp-suite:

```sh
python3 scripts/meta/version.py show [--json] [--next]
python3 scripts/meta/version.py bump {patch|minor|major}
python3 scripts/meta/version.py build bump
python3 scripts/meta/version.py check
```

An app repository is the same with `scripts/version.py`.

## Releasing

Kit line:

```sh
python3 scripts/meta/version.py build bump      # if this kit release owns a new build
python3 scripts/meta/version.py bump minor      # only when MINOR/MAJOR moves
git commit -m "chore(kits): release the kit line"
git tag -a "kits-v<version>" -m "warpui-kits <version>"
git push origin main "kits-v<version>"           # apps consume the tag
```

App line:

```sh
# in the app repository
python3 scripts/version.py build bump
git commit -m "chore(release): v<version>"
git tag -a "v<version>" -m "<app> <version>"
git push origin main "v<version>"                # release.yml publishes it
```

Suite line: put the app tags and the kit tag into `suite.toml`, tag this repo
with the suite version, and let the suite assembler publish the combined
release. It downloads the pinned app releases; it never rebuilds app sources.

## CI

- `kits.yml` runs `version.py check` and the kit gate.
- Each app's `ci.yml` / `release.yml` do the same for that app's line.
