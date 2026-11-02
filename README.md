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

## Layout

| Path | Purpose |
|---|---|
| `apps/<name>/` | the app sources, each with its own history (imported with `git subtree`) |
| `apps.toml` | per-app build knobs (`linux_build`, `linux_skip`, `icon_prefix`, `scalable_svg`) and contract notes |
| `scripts/manifest.py` | apps.toml reader shared by the scripts + CI |
| `.cargo/config.toml` | one cargo target dir for the whole suite: `.build/target` for every app, `warpui-kits` and `bundle-linux.sh` |
| `warpui-kits/` | cargo workspace of the four `warpui-kit-*` crates the apps depend on (`../../warpui-kits`) |
| `scripts/bundle-linux.sh` | `cargo build --release` each app → stage `dist/warp-suite-<v>-linux/` + tarball (the exact bundle CI publishes) |
| `scripts/install-root.sh` | install an extracted bundle into `~/.local` |
| `install.sh` | suite installer: download a release or build locally |
| `.github/workflows/suite-release.yml` | the combined release pipeline |
| `design/v3/` | the committed design contract (DESIGN.md + prototype source); the live source is the OpenDesign project `warp-suite-v3` |

The suite never reimplements app builds: it invokes what each app already
ships (`packaging/windows/build.ps1`, the `.iss` installers, the `*.desktop`
+ icon files). Only orchestration and assembly are new.

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
- **Shared text size** (`~/.warp/ui.json`, key `text_size`: `compact`, `normal`
  or `large`): `warpui-kit-core` owns the file and the one control that writes
  it. A click applies in that app at once; the other apps read the value when
  they next start. The apps' old `font_scale` settings only seed the file on
  its first run, so every app must be on the same `warpui-kit-core`.

## Adding an app

1. Import it with history: `git subtree add --prefix=apps/<name> <repo> <ref>`.
2. Add a section to `apps.toml` (optional
   `linux_build`/`linux_skip`/`icon_prefix`/`scalable_svg` —
   `scripts/bundle-linux.sh` picks it up automatically; a Windows-only app
   sets `linux_skip = true` and needs a build + package block in
   `suite-release.yml` mirroring the existing ones).
3. If it needs a shared library crate, add it to the `warpui-kits/`
   workspace.
4. Document its contracts in [Cross-app contracts](#cross-app-contracts).

That is exactly how warp-snapit joined (Windows-only: `linux_skip = true`,
portable zip from its own `packaging/windows/build.ps1`, no installer). The
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
  through `../../warpui-kits/*` path deps. They live in this repo as the
  `warpui-kits/` cargo workspace, so a release builds from this repo alone.
  The suite commit is the kits' provenance — no sync step, no second copy.
  Run `cargo test` in `warpui-kits/` after changing one; the apps' lock files
  alone do not describe the libraries.
