# warp-suite

Monorepo for Engin's connected apps. Each app lives under `apps/<name>`,
imported with `git subtree` so its history stays intact; warp-suite builds
them together and publishes **one combined release**.

Currently in the suite:

| App | Path | What it is |
|---|---|---|
| warp-station | `apps/warp-station` | Repository workbench (Rust/GPUI workspace) |
| warp-agent | `apps/warp-agent` | Chat client over `opencode serve` (Rust/GPUI) |
| warp-snapit | `apps/warp-snapit` | Rust port of the SnapIt zone snapper (Windows-only, portable zip) |
| warp-notes | `apps/warp-notes` | Per-repo markdown notes with a live editor (Rust/GPUI) |

The C#/.NET SnapIt repo ([enginkirmaci/SnapIt](https://github.com/enginkirmaci/SnapIt))
stays excluded — the Rust port above is the suite's zone snapper.
See [Adding an app](#adding-an-app).

## Deploy

```sh
# GitHub → warp-suite → Actions → suite-release → Run workflow (version: 1.2.0)
gh workflow run suite-release.yml -f version=1.2.0
```

The run builds every app under `apps/` at the dispatched commit and publishes
release `v<version>` with the Linux tarball plus per-app Windows installers
and portable zips. The release records the suite commit, and the Linux
tarball's `REVISIONS.txt` records it per app — one commit identifies the whole
suite.

## Install on a Linux machine

```sh
git clone https://github.com/enginkirmaci/warp-suite && cd warp-suite
./install.sh                    # latest published suite release
./install.sh --release v1.2.0   # a specific one
./install.sh --local            # build the apps in this checkout, then install
```

Binaries go to `~/.local/bin`, desktop entries + hicolor icons to
`~/.local/share`. Everything is per-user; no root needed.

## Build and deploy locally

```sh
scripts/build.sh                  # Linux: dist/linux/warp-suite-<v>-linux/ + tarball
scripts/build.sh --app warp-notes # one app, fast loop
scripts/deploy.sh --local         # build, then install into ~/.local
scripts/deploy.sh --uninstall     # remove exactly what it installed
```

```powershell
scripts\build.ps1                 # Windows: dist\windows\ zips + installers
scripts\build.ps1 -App warp-notes -NoPackage
scripts\deploy.ps1 -Local         # build, then install a portable copy
scripts\deploy.ps1 -Release       # install a published release (installers)
scripts\deploy.ps1 -Uninstall
```

Both builds default the version to `git describe --tags --always
--first-parent`; CI passes `--version` / `-Version` explicitly.

## Layout

| Path | Purpose |
|---|---|
| `apps/<name>/` | the app sources, each with its own history (imported with `git subtree`) |
| `apps.toml` | per-app build knobs (`linux_build`, `linux_skip`, `icon_prefix`, `scalable_svg`) and contract notes |
| `scripts/manifest.py` | apps.toml reader shared by the scripts + CI |
| `.cargo/config.toml` | one cargo target dir for the whole suite: `.build/target` for every app, `kits` and `scripts/build.sh` |
| `kits/` | cargo workspace of the four `warpui-kit-*` crates the apps depend on (`../../kits`) |
| `scripts/build.sh` | Linux build: `cargo build --release` each app → `dist/linux/warp-suite-<v>-linux/` + tarball |
| `scripts/build.ps1` | Windows build: `dist/windows/` portable zips + Inno Setup installers |
| `scripts/deploy.sh` | Linux install/uninstall: local bundle or published release |
| `scripts/deploy.ps1` | Windows install/uninstall: portable locally, installers from a release |
| `scripts/bundle-install.sh` | runs inside an extracted bundle; installs into `~/.local` and records what it wrote |
| `install.sh` | shim over `scripts/deploy.sh` for the documented one-liner |
| `.github/workflows/suite-release.yml` | the combined release pipeline |
| `.github/workflows/kits.yml` | CI gate: kit fmt/clippy/tests + the apps' tests on every push that touches `kits/`, `apps/` or `.cargo/` |
| `design/v3/` | the committed design contract (DESIGN.md + prototype source); the live source is the OpenDesign project `warp-suite-v3` |

Each app keeps only packaging assets (`packaging/linux/{*.desktop,icons}`,
`packaging/windows/{*.ico,*.iss,settings}`); every build and install step
lives in `scripts/`, so a local build and CI run the same code.

## Cross-app contracts

The apps talk to each other; these are the interfaces that must move
together. When one changes, update the other side in the **same** commit.

- **warp-agent loopback door** (`~/.warp-agent/ipc.port` + `ipc.token`,
  token-authenticated NDJSON on `127.0.0.1`): commands `open-workspace`,
  `new-chat`, `run-prompt`, `models`, `status` (+ status pushes).
  `new-chat`/`run-prompt` accept `harness`/`model`/`variant`, and `models`
  answers with the harnesses and each model's variants. warp-agent owns the
  protocol; Warp Station is the client — a missing warp-agent binary turns
  Warp Station's AI features off, a closed one is launched on demand.
- **SnapIt settings**: [warp-snapit](https://github.com/enginkirmaci/warp-snapit)
  and Warp Station read the same `~/.warp/snapit` settings/layouts
  (string-typed numbers, `"x,y"` points, the misspelled `DeactivedScreens`
  field). Warp Station's `SnapIt.Executable` setting points at the Rust
  `snapit.exe`, which is a portable-zip deliverable (no installer) and
  skipped in the Linux bundle (`linux_skip = true` in `apps.toml`).
- **Shared notes store** (`~/.warp/notes`, one folder per repo): Warp
  Station's notes surfaces (titlebar toggle, tools menu, repo-row icons,
  global-search note hits) launch
  [warp-notes](https://github.com/enginkirmaci/warp-notes)
  with `--store` (Warp Station's resolved `notes_store_path`) plus `--repo`/`--note`;
  standalone launches root at the shared store and a legacy
  `~/.devtools/notes` renames over on whichever app runs first. Missing
  warp-notes binary = the notes surfaces fail with a toast, nothing else
  breaks.
- **Shared text size and palette** (`~/.warp/ui.json`: `text_size` is `small`,
  `normal`, `large` or `larger` — the old `compact` spelling reads as normal —
  and `palette` is `copper`, `plum`, `pine` or `blue`): `warpui-kit-core` owns
  the file and the one control that writes each key. A click applies in that
  app at once; the other apps read the value when they next start. The apps'
  old `font_scale` settings only seed the file on its first run, so every app
  must be on the same `warpui-kit-core`. Both keys are additive: a file that
  lacks one keeps the other.

## Adding an app

1. Import it with history: `git subtree add --prefix=apps/<name> <repo> <ref>`.
2. Add a section to `apps.toml` if it needs non-default build knobs
   (`linux_build`/`linux_skip`/`icon_prefix`/`scalable_svg`) — `build.sh`
   picks it up automatically. Windows needs no entry: `build.ps1` discovers
   every app under `apps/` and compiles an installer only when
   `packaging/windows/<app>.iss` exists.
3. If it needs a shared library crate, add it to the `kits/`
   workspace.
4. Document its contracts in [Cross-app contracts](#cross-app-contracts).

That is exactly how warp-snapit joined (Windows-only: `linux_skip = true`,
no `.iss` so `build.ps1` ships a portable zip only). The
C#/.NET SnapIt repo would join the same way if ever wanted.

## Notes

- The apps are imported with `git subtree` and keep their own histories. Do
  not `git subtree pull`/`push`: warp-suite is the source of truth, and the
  old per-app repos are read-only.
- Each imported tip is kept as a local ref, `refs/imports/<app>/main`
  (`git show refs/imports/warp-agent/main`). They are not branches, so they
  do not appear in `git branch` and a normal `git push` does not send them.
- A release builds whatever is committed on the dispatched ref; the release
  body records the suite commit, and `REVISIONS.txt` records it per app.
- The apt dependency list in `suite-release.yml` covers GPUI's Linux needs
  (wayland, xkbcommon, vulkan, fontconfig); expect to touch it when GPUI's
  requirements grow.
- **Kit crates:** the apps depend on `warpui-kit-core`/`-diff`/`-markdown`/`-picker`
  through `../../kits/*` path deps. They live in this repo as the
  `kits/` cargo workspace, so a release builds from this repo alone.
  The suite commit is the kits' provenance — no sync step, no second copy.
  Run `cargo test` in `kits/` after changing one; the apps' lock files
  alone do not describe the libraries. `kits/Cargo.lock` is committed (the
  only workspace that keeps one) so `cargo --locked` in CI resolves the same
  versions every run.

## CI caching

Both workflows use `Swatinem/rust-cache`, and both must name the target dir
explicitly (`workspaces: <workspace> -> <target>`):

- `.cargo/config.toml` sends every workspace to `.build/target`, so the
  action's default `<workspace>/target` does not exist. A bare
  `workspaces: kits` caches only the registry, and the build stays cold.
- The five app workspaces share that one target dir. The action prunes a
  target dir down to the owning workspace's dependency closure, so listing
  several workspaces runs several prune passes over the same tree and the
  last one wins — warp-snapit keeps ~31 of ~816 packages and would delete
  everything the others built. Each job therefore uses one anchor workspace
  (`apps/warp-notes`, whose closure covers ~98% of the others').

The target dir is ~4.5 GB release / ~2.9 GB debug, so the caches are pruned
rather than uploaded whole (the repo cache limit is 10 GB).
