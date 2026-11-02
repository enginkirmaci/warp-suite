# warp-suite

One-git deploy point for Engin's connected apps. Each app lives in its own
repo and stays the source of truth; warp-suite pins a known-good combination
of them, builds everything at those pins and publishes **one combined
release**.

Currently in the suite:

| App | Repo | What it is |
|---|---|---|
| warp-station | [enginkirmaci/warp-station](https://github.com/enginkirmaci/warp-station) | Repository workbench (Rust/GPUI workspace) |
| warp-agent | [enginkirmaci/warp-agent](https://github.com/enginkirmaci/warp-agent) | Chat client over `opencode serve` (Rust/GPUI) |
| warp-snapit | [enginkirmaci/warp-snapit](https://github.com/enginkirmaci/warp-snapit) | Rust port of the SnapIt zone snapper (Windows-only, portable zip) |
| warp-notes | [enginkirmaci/warp-notes](https://github.com/enginkirmaci/warp-notes) | Per-repo markdown notes with a live editor (Rust/GPUI) |

The C#/.NET SnapIt repo ([enginkirmaci/SnapIt](https://github.com/enginkirmaci/SnapIt))
stays excluded — the Rust port above is the suite's zone snapper.
See [Adding an app](#adding-an-app).

## Deploy

```sh
# 1. choose what ships
scripts/set-pin.sh warp-station rust      # or a tag / commit SHA
scripts/set-pin.sh warp-agent rust-gpui
scripts/set-pin.sh warp-snapit main

# 2. build + publish
#    GitHub → warp-suite → Actions → suite-release → Run workflow (version: 1.2.0)
gh workflow run suite-release.yml -f version=1.2.0
```

The run resolves every pinned ref to its commit SHA, builds each app, and
publishes release `v<version>` with the Linux tarball, per-app Windows
installers + portable zips, and a pins table (repo/ref/resolved SHA) so every
release is reproducible.

## Install on a Linux machine

```sh
git clone https://github.com/enginkirmaci/warp-suite && cd warp-suite
./install.sh                    # latest published suite release
./install.sh --release v1.2.0   # a specific one
./install.sh --local            # build the pinned apps here, then install
```

Binaries go to `~/.local/bin`, desktop entries + hicolor icons to
`~/.local/share`. Everything is per-user; no root needed.

## Layout

| Path | Purpose |
|---|---|
| `apps.toml` | the manifest: per app `repo`, `ref`, build knobs (`linux_build`, `icon_prefix`, `scalable_svg`) and contract notes |
| `scripts/manifest.py` | apps.toml reader/writer shared by all scripts + CI |
| `scripts/set-pin.sh` | verify a ref against the remote, write it into the manifest |
| `warpui-kits/` | cargo workspace of the four `warpui-kit-*` crates the apps depend on; the release jobs stage it at `.build/warp-suite/warpui-kits` |
| `scripts/bundle-linux.sh` | clone at pins → `cargo build --release` → stage `dist/warp-suite-<v>-linux/` + tarball (the exact bundle CI publishes) |
| `scripts/install-root.sh` | install an extracted bundle into `~/.local` |
| `install.sh` | suite installer: download a release or build locally |
| `.github/workflows/suite-release.yml` | the combined release pipeline |
| `design/v3/` | the committed design contract (DESIGN.md + prototype source); the live source is the OpenDesign project `warp-suite-v3` |

The suite never reimplements app builds: it checks out each repo at its pin
and invokes what the repo already ships (`packaging/windows/build.ps1`, the
`.iss` installers, the `*.desktop` + icon files). Only pin resolution,
orchestration and assembly are new.

## Cross-app contracts

The apps talk to each other; these are the interfaces that must move
together. When one changes, update the other side and re-pin both apps in the
**same** suite release.

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

1. Add a section to `apps.toml` (`repo`, `ref`, optional
   `linux_build`/`linux_skip`/`icon_prefix`/`scalable_svg` —
   `scripts/bundle-linux.sh` picks it up automatically; a Windows-only app
   sets `linux_skip = true` and needs a build + package block in
   `suite-release.yml` mirroring the existing ones).
2. If it needs a shared library crate, add it to the `warpui-kits/`
   workspace; both release jobs stage the whole workspace.
3. Document its contracts in [Cross-app contracts](#cross-app-contracts).

That is exactly how warp-snapit joined (Windows-only: `linux_skip = true`,
portable zip from its own `packaging/windows/build.ps1`, no installer). The
C#/.NET SnapIt repo would join the same way if ever wanted.

## Notes

- Private app repos (warp-agent is private): the `APPS_TOKEN` repo secret
  authenticates the CI checkouts and bundle clones (same extraheader
  mechanism `actions/checkout` uses). Update it with
  `gh secret set APPS_TOKEN --body "$(gh auth token)"` when the credential
  changes. Local builds don't need it — your git credentials apply.
- Symbolic pins (currently `rust`, `rust-gpui` and `main`) build the branch
  tip at
  dispatch time; the exact SHAs are recorded in the release notes and
  `REVISIONS.txt`. Pin tags or SHAs for fully frozen deploys.
  (Caveat kept in mind: warp-agent's `main` is the Avalonia prototype — the
  Rust apps ship on the branches above.)
- The apt dependency list in `suite-release.yml` covers GPUI's Linux needs
  (wayland, xkbcommon, vulkan, fontconfig); expect to touch it when GPUI's
  requirements grow.
- **Kit crates:** the apps depend on `warpui-kit-core`/`-diff`/`-markdown`/`-picker`
  through `../warp-suite/warpui-kits/*` path deps. They live in this repo as
  the `warpui-kits/` cargo workspace (not separate repos yet), and
  `bundle-linux.sh` + the Windows CI job stage the whole workspace at
  `.build/warp-suite/warpui-kits`, the same relative path from every app
  clone, so a release builds from this repo alone. The suite commit is the
  kits' provenance — no sync step, no second copy. A local app checkout needs
  this repo as a sibling; no symlinks or extra copies. Run `cargo test` in
  `warpui-kits/` after changing one; the app pins alone do not describe the
  libraries.
