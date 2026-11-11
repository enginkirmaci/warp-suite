# Version and build numbers

One version (`MAJOR.MINOR.PATCH`) and one build counter for every binary in the
suite: the four desktop apps, the Android bridge and the APK. About, `--version`,
the Windows file version, the APK manifest, `/health` and the release artifacts
all read the same facts.

## Source of truth

`version.toml` at the repository root:

```toml
version = "0.1.8"
build = 42
```

`scripts/version.py` is the only writer. Nothing else edits a version by hand.

| Element | Rule |
|---|---|
| PATCH | +1 on every commit, through `.githooks/pre-commit`; never resets |
| MINOR / MAJOR | by hand, `version.py bump minor` / `bump major`; patch keeps counting |
| build | +1 on every run of `scripts/build.sh` or `scripts/build.ps1` |

A commit carries the version it ships: the hook bumps PATCH, rewrites
`Cargo.toml` and `Cargo.lock`, and stages all three files before the commit is
created. `WARP_VERSION_SKIP=1 git commit ...` skips the bump for an amend, a
rebase or an emergency. `check --history` reports the gap afterwards.

The build counter is consumed at the start of a build script, so a failed build
uses a number and the numbering can have gaps; it only ever grows. `cargo build`
does not count and reads the last value from `version.toml` through
`kits/warpui-kit-build`.

A CI job passes `WARP_SUITE_BUILD` instead: one value read from the tag's
`version.toml`, used by both the Linux and Windows jobs, so one tag produces one
build number on both platforms. Bump the counter before creating a release tag
(see *Releasing*).

## What the facts are

| Fact | Source |
|---|---|
| `version` | `version.toml` (`CARGO_PKG_VERSION` in the binaries) |
| `build` | the counter |
| `commit` | `git rev-parse --short=7 HEAD`, `-dirty` when the tree differs |
| `channel` | `stable` only when HEAD carries the tag `v<version>`; `alpha`/`beta`/`rc` for such a tag; `dev` otherwise |
| `released` | the tag's commit date in a tagged build, otherwise `not released` |

The channel comes from the tag, not from the version's shape: with
patch-per-commit every version is a plain SemVer, so the shape would call every
development build stable.

## Where the facts appear

| Surface | Shows |
|---|---|
| `Copy version`, `<app> --version` | `Warp Station 0.1.8 (build 42 · d904edc) · dev` |
| About Build card | Version, Build, Commit, Channel, Released, Runtime |
| Windows `FILEVERSION` / `PRODUCTVERSION` | `0.1.8.42`; 16-bit fields, the build fails above 65535 |
| Windows strings | Product name, description, company, copyright, original filename |
| Android | `versionName` from `version.toml`; `versionCode = major*10_000_000 + minor*100_000 + patch` |
| `GET /health` (bridge) | `version`, `build`, `commit`, `channel` |
| `REVISIONS.txt` | `suite 0.1.8 build 42 commit d904edc channel dev released not released` |
| Artifact names | `warp-suite-0.1.8-linux.tar.gz`, `warp-station-0.1.8-setup-x64.exe` |

The Android project reads `../../../version.toml` when Gradle configures; it keeps
no version copy. The bridge and the apps get the facts from
`kits/warpui-kit-build` in their `build.rs`.

## One-time setup per clone

```sh
python3 scripts/version.py install-hooks   # git config core.hooksPath .githooks
```

## Commands

```sh
python3 scripts/version.py show [--json] [--next]
python3 scripts/version.py bump {patch|minor|major}
python3 scripts/version.py build bump
python3 scripts/version.py sync          # rewrite Cargo.toml/Cargo.lock
python3 scripts/version.py check [--history]
```

- `bump` stages the three files it writes; `--no-stage` leaves the index alone.
- `check` compares `Cargo.toml`, every workspace member and `Cargo.lock` with
  `version.toml`, and rejects a literal version in a member manifest, an Inno
  `MyAppVersion` fallback, a literal Gradle version and a hardcoded release
  input default.
- `check --history` walks every commit from the one that added `version.toml` and
  requires PATCH to be exactly the parent's + 1.
- `sync` is the repair path after a hand edit; it also refreshes `Cargo.lock`.

## Releasing

Starting from `0.1.41`:

```sh
python3 scripts/version.py build bump          # if this release should own a new build
python3 scripts/version.py bump minor          # only when MINOR/MAJOR moves; tree says 0.2.41
next=$(python3 scripts/version.py show --next) # 0.2.42: the hook bumps PATCH again
git commit -m "chore(release): v$next"
git tag -a "v$next" -m "warp-suite $next"
git push origin main "v$next"
gh workflow run suite-release.yml --ref "v$next" -f version=$next
```

The workflow checks that `version` matches `version.toml` and that the dispatch
ref is `v<version>`; it publishes to that existing tag and never invents one.

## Android

The APK is sideloaded and stays outside the suite release. Its Gradle build
reads `version.toml`, so an APK built from a checkout always carries that
checkout's version; no extra sync step exists.

## CI

- `kits.yml` runs `version.py check --history` in the fmt job (full history).
- `suite-release.yml` cross-checks the dispatched version and the tag.
- `--locked` fails when the lock was not refreshed; the pre-commit hook is what
  keeps it fresh.
