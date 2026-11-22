# Local setup: one workspace, sibling app checkouts

warp-suite keeps the kits and the suite assembler. The apps live in their own
repositories next to it. On a developer machine `warp-suite/apps/` is a set of
symlinks into those checkouts, so the root workspace still resolves every app
and kit together: one lockfile, one target directory, one gpui compile.

The one target directory is `.build/target`, and on a developer machine it is
a **symlink** to a fast local disk (`~/.cache/warp-suite/target` here) because
the suite's build is I/O-bound; see
[`build-times.md`](build-times.md) before moving it, wiping it or wondering why
a package selector took minutes.

Each app checkout's own `target/` is a symlink to that same cache, so a
standalone `cargo build` in `~/Developments/warp-station` and a build through
`warp-suite/apps/warp-station` reuse one dependency tree instead of compiling
the gpui graph twice. `scripts/dev/link-apps.sh` creates and repairs those
links; `--check` reports a missing link, a wrong one and a real directory
separately, and `--replace-target` moves a real `target/` aside and links the
cache in its place.

Two rules follow from the shared cache:

- Every app's `[profile.dev] debug = "line-tables-only"` must match warp-suite's
  root `[profile.dev]`. Cargo keys a crate's fingerprint on the resolved
  profile, so a different setting makes every shared dependency compile a
  second time — the duplication the single target directory exists to avoid.
- `cargo clean` in an app removes the **symlink**, not the cache, and the next
  build then creates a real `target/` on the slow disk. Re-run
  `scripts/dev/link-apps.sh` (or `ln -s ~/.cache/warp-suite/target target`) to
  put the link back.

> The split is complete: the apps live in their own repositories and `apps/`
> holds only symlinks. `scripts/dev/link-apps.sh` is the only writer of those
> links; it reports a real directory instead of replacing it.

## Layout

```
~/Developments/
├── warp-suite/                 # this repo: kits, suite tools, suite.toml
├── warp-station/
├── warp-agent/
├── warp-flow/
├── warp-notes/
├── warp-pomodoro/
├── warp-snapit/
└── warp-android-scribe/        # Gradle project + Rust bridge (own repo)
```

`warp-flow`'s link is made by hand until it has a published release: the
linker takes app names from `suite.toml`, whose entries carry a release tag, so
that table gains warp-flow in the same change window as its first tag.

The app directories must be siblings of `warp-suite`. Any other layout works
with `scripts/dev/link-apps.sh --source <dir>`; the symlink then points at an
absolute path instead of the relative sibling.

## Requirements

- Rust: the channel pinned in `rust-toolchain.toml` (1.99). `rustup` reads it
  from the checkout.
- Python 3.11+ (`tomllib`) for `scripts/meta/version.py`, `scripts/meta/manifest.py` and
  `scripts/dev/link_apps.py`.
- `git`, and `gh` for the release workflows and for cloning private repos.
- The Linux packages the GPUI build needs: CI installs them through
  `.github/actions/gpui-deps`; run that action's commands once on a fresh
  machine.

## First setup

```sh
# 1. clone the suite and the apps as siblings
gh repo clone enginkirmaci/warp-suite
gh repo clone enginkirmaci/warp-station
gh repo clone enginkirmaci/warp-agent
gh repo clone enginkirmaci/warp-notes
gh repo clone enginkirmaci/warp-pomodoro
gh repo clone enginkirmaci/warp-snapit
gh repo clone enginkirmaci/warp-android-scribe

# 2. link the app checkouts into apps/, and each app's target/ at the cache
cd warp-suite
scripts/dev/link-apps.sh --check   # report what would happen, change nothing
scripts/dev/link-apps.sh           # create the symlinks (junctions on Windows)

# 3. install the version hook in this repo (each app repo has its own copy)
python3 scripts/meta/version.py install-hooks

# 4. verify the workspace sees the apps through the links
cargo metadata --no-deps --format-version 1 |
  python3 -c 'import json,sys; print("\n".join(sorted(p["name"] for p in json.load(sys.stdin)["packages"])))'
```

Step 4 must list `warp-station`, `warp-station-core`, `warp-agent`,
`warp-notes`, `warp-pomodoro`, `warp-snapit` and the seven `warpui-kit-*`
crates.

## Daily commands

```sh
cargo build -p warp-notes            # one app, against the local kits
cargo test -p warpui-kit-core        # one kit
cargo test -p warp-station -p warp-station-core
cargo fmt --all                      # the whole symlinked workspace
cargo clippy --locked --all-targets -p warp-agent -- -D warnings

scripts/dev/refresh.sh                     # pull the apps' main, then build
scripts/dev/refresh.sh --app warp-notes    # one app, sync and build
scripts/build/build.sh                     # Linux bundle + tarball
scripts/build/build.sh --app warp-notes    # one app
scripts/build/kit-build.sh                 # kit release artifacts in dist/kits/
```

`scripts/dev/link-apps.sh --check` is the quick health check: it reports a missing
target, a wrong link and a real directory separately.

`scripts/dev/refresh.sh` (and `scripts/dev/refresh.ps1` on Windows) fetches
each app's `origin`, fast-forwards `main`, and then runs the platform build
with installers off (`build.ps1 -NoPackage` on Windows; the Linux build has no
installer step). It never merges, rebases, resets or stashes: an uncommitted
tree, a feature branch or diverged history is reported and left alone.
`--check` fetches and reports only, `--no-build` stops after the sync, and
`--dry-run` prints the git and build commands.

## Why the apps have no `[workspace]` table

Each app repository is a single package (station adds `warp-station-core` as a
path dependency). With no `[workspace]` table, Cargo treats the app root as an
implicit workspace when the app is built alone, and the app can also be a
member of warp-suite's workspace when it is reached through the symlink.

The app `Cargo.toml` pins the kits by tag:

```toml
warpui-kit-core = { git = "https://github.com/enginkirmaci/warp-suite", tag = "kits-v0.2.0", package = "warpui-kit-core" }
```

warp-suite's root `Cargo.toml` reverses that for the local build:

```toml
[patch."https://github.com/enginkirmaci/warp-suite"]
warpui-kit-core = { path = "kits/warpui-kit-core" }
```

So the same app manifest builds two ways: from the tag in the app's own CI,
and from the local kits when built through `apps/`. The two lockfiles are
expected to differ (git source vs path source); never copy one over the other.
Building a linked app also extends warp-suite's `Cargo.lock` with that app's
dependencies; do not commit that extension. The app repositories keep
`--locked` on their own lockfiles.

## Kit changes

1. Edit under `kits/` in warp-suite.
2. `cargo test -p warpui-kit-core` (or the kit you touched).
3. Build an app through `apps/`: it picks the change up immediately, no tag
   needed.
4. When the change is ready for the apps, cut a kit tag
   (`kits-v<version>`); apps consume the tag directly.
5. Each app bumps its kit pin in its own PR and runs its CI.

## App changes

Work in the app checkout itself. `cargo build` there resolves the kits from the
pinned tag; to see a local kit change before it is tagged, build through
`warp-suite/apps/<name>` instead.

On a developer machine each app checkout carries a **gitignored**
`.cargo/config.toml` that overrides the warpui-kit crates with cargo's `paths`
to point at this repo's `kits/`. That makes a standalone app build use the same
kit sources as a build through `apps/<name>`, so the two share compiled kit
artifacts and kit breakpoints resolve to `kits/` rather than to a
`~/.cargo/git/checkouts` copy. `Cargo.toml` and `Cargo.lock` keep the pinned
tag, so CI's `--locked` build is unaffected — the file is gitignored precisely
because CI has no sibling `warp-suite` checkout and a committed copy would fail
on a dangling path. Delete it to build against the tag again.

## Adding an app

1. Create the app repository with `git subtree split` (history kept).
2. Add the app to `suite.toml` with its tag and repo.
3. Run `scripts/dev/link-apps.sh`; `--check` must report the new link.
4. `cargo build -p <app>` from warp-suite to prove the workspace sees it.

## Removing an app

Delete its `suite.toml` entry and its `apps/<name>` link. Do not delete the app
repository from the suite: it lives on its own.

## Private-repo access

The kits live in the private `warp-suite` repository. An app checkout fetches
them with the developer's git credentials; CI uses a fine-grained token with
read access, configured as `KITS_READ_TOKEN` in the app repository. A local
`cargo build` that fails with "repository not found" means the credential is
missing, not that the tag is wrong.

## Troubleshooting

| Symptom | Cause |
|---|---|
| `apps/<name> is a real directory` | pre-split layout; move the directory out before linking |
| `cargo` does not list an app | the symlink is missing or dangling; run `scripts/dev/link-apps.sh --check` |
| `failed to select a version for warpui-kit-core` | the pinned tag does not exist, or the app was built outside the patch table |
| `the lock file needs to be updated` | the app lock and the suite lock are different by design; run `cargo update -p warpui-kit-core` in the app checkout, not here |
| an app build recompiles the whole gpui graph | `<app>/target` is a real directory, not the cache link; run `scripts/dev/link-apps.sh --replace-target` |
| an app build is slow and `target/` reappeared on the slow disk | `cargo clean` removed the link; re-run `scripts/dev/link-apps.sh` |
| the same dependency compiles twice across app and suite builds | the app's `[profile.dev]` differs from warp-suite's root `[profile.dev]` |
| an app build ignores a local kit edit | the app's `.cargo/config.toml` is missing; it is gitignored, so a fresh clone has none |
| an app build fails on `../warp-suite/kits/...` | the sibling checkout is missing or renamed; delete the app's `.cargo/config.toml` |
