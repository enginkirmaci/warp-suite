# Local setup: one workspace, sibling app checkouts

warp-suite keeps the kits and the suite assembler. The apps live in their own
repositories next to it. On a developer machine `warp-suite/apps/` is a set of
symlinks into those checkouts, so the root workspace still resolves every app
and kit together: one lockfile, one target directory, one gpui compile.

> The split is complete: the apps live in their own repositories and `apps/`
> holds only symlinks. `scripts/dev/link-apps.sh` is the only writer of those
> links; it reports a real directory instead of replacing it.

## Layout

```
~/Developments/
├── warp-suite/                 # this repo: kits, suite tools, suite.toml
├── warp-station/
├── warp-agent/
├── warp-notes/
├── warp-snapit/
└── warp-android-scribe/        # Gradle project + Rust bridge (own repo)
```

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
gh repo clone enginkirmaci/warp-snapit
gh repo clone enginkirmaci/warp-android-scribe

# 2. link the app checkouts into apps/
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
`warp-notes`, `warp-snapit` and the six `warpui-kit-*` crates.

## Daily commands

```sh
cargo build -p warp-notes            # one app, against the local kits
cargo test -p warpui-kit-core        # one kit
cargo test -p warp-station -p warp-station-core
cargo fmt --all                      # the whole symlinked workspace
cargo clippy --locked --all-targets -p warp-agent -- -D warnings

scripts/build/build.sh                     # Linux bundle + tarball
scripts/build/build.sh --app warp-notes    # one app
scripts/build/kit-build.sh                 # kit release artifacts in dist/kits/
```

`scripts/dev/link-apps.sh --check` is the quick health check: it reports a missing
target, a wrong link and a real directory separately.

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

## Kit changes

1. Edit under `kits/` in warp-suite.
2. `cargo test -p warpui-kit-core` (or the kit you touched).
3. Build an app through `apps/`: it picks the change up immediately, no tag
   needed.
4. When the change is ready for the apps, cut a kit tag
   (`kits-v<version>`) and publish it with `kits-release.yml`.
5. Each app bumps its kit pin in its own PR and runs its CI.

## App changes

Work in the app checkout itself. `cargo build` there resolves the kits from the
pinned tag; to see a local kit change before it is tagged, build through
`warp-suite/apps/<name>` instead.

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
