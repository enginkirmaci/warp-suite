# warp-suite

One-git deploy point for Engin's connected apps. Each app lives in its own
repo and stays the source of truth; warp-suite pins a known-good combination
of them, builds everything at those pins and publishes **one combined
release**.

Currently in the suite:

| App | Repo | What it is |
|---|---|---|
| devtools | [enginkirmaci/DevTools](https://github.com/enginkirmaci/DevTools) | Repository workbench (Rust/GPUI workspace) |
| warp-agent | [enginkirmaci/warp-agent](https://github.com/enginkirmaci/warp-agent) | Chat client over `opencode serve` (Rust/GPUI) |
| warp-snapit | [enginkirmaci/warp-snapit](https://github.com/enginkirmaci/warp-snapit) | Rust port of the SnapIt zone snapper (Windows-only, portable zip) |
| warp-notes | [enginkirmaci/warp-notes](https://github.com/enginkirmaci/warp-notes) | Per-repo markdown notes with a live editor (Rust/GPUI) |

The C#/.NET SnapIt repo ([enginkirmaci/SnapIt](https://github.com/enginkirmaci/SnapIt))
stays excluded — DevTools vendors the .NET companion it ships, and the Rust
port above is the suite's zone snapper. See [Adding an app](#adding-an-app).

## Deploy

```sh
# 1. choose what ships
scripts/set-pin.sh devtools rust          # or a tag / commit SHA
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
| `scripts/bundle-linux.sh` | clone at pins → `cargo build --release` → stage `dist/warp-suite-<v>-linux/` + tarball (the exact bundle CI publishes) |
| `scripts/install-root.sh` | install an extracted bundle into `~/.local` |
| `install.sh` | suite installer: download a release or build locally |
| `.github/workflows/suite-release.yml` | the combined release pipeline |

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
  `new-chat`, `run-prompt`, `models`, `status` (+ status pushes). warp-agent
  owns the protocol; DevTools is the client — a missing warp-agent binary
  turns DevTools' AI features off, a closed one is launched on demand.
- **DevTools Windows companion**: `packaging/windows/build.ps1` builds the
  vendored `src/Tools.SnapIt` copy and stages it as `Tools.SnapIt.exe`
  alongside `devtools.exe`. The vendored copy is a deliberate variant of the
  SnapIt repo's app (they have diverged); the suite builds it from the
  DevTools pin, so suite deploys stay reproducible either way.
- **SnapIt settings**: the vendored .NET companion and the Rust
  [warp-snapit](https://github.com/enginkirmaci/warp-snapit) port read the
  same `~/.devtools/snapit` settings/layouts (string-typed numbers, `"x,y"`
  points, the misspelled `DeactivedScreens` field). DevTools launches the
  .NET companion by default; the `SnapIt.Executable` setting points it at the
  Rust `snapit.exe`, which is a portable-zip deliverable (no installer) and
  skipped in the Linux bundle (`linux_skip = true` in `apps.toml`).
- **Shared notes store** (`~/.warp/notes`, one folder per repo): DevTools'
  notes surfaces (titlebar toggle, tools menu, repo-row icons, global-search
  note hits) launch [warp-notes](https://github.com/enginkirmaci/warp-notes)
  with `--store` (DevTools' resolved `notes_store_path`) plus `--repo`/`--note`;
  standalone launches root at the shared store and a legacy
  `~/.devtools/notes` renames over on whichever app runs first. Missing
  warp-notes binary = the notes surfaces fail with a toast, nothing else
  breaks.
- **Shared settings**: the Rust `crates/snapit` port inside DevTools reads
  the same `~/.devtools/snapit` settings/layouts as the C# companion.

## Adding an app

1. Add a section to `apps.toml` (`repo`, `ref`, optional
   `linux_build`/`linux_skip`/`icon_prefix`/`scalable_svg` —
   `scripts/bundle-linux.sh` picks it up automatically; a Windows-only app
   sets `linux_skip = true` and needs a build + package block in
   `suite-release.yml` mirroring the existing ones).
2. Document its contracts in [Cross-app contracts](#cross-app-contracts).

That is exactly how warp-snapit joined (Windows-only: `linux_skip = true`,
portable zip from its own `packaging/windows/build.ps1`, no installer). The
C#/.NET SnapIt repo would join the same way if ever wanted — DevTools' Windows
build uses its vendored companion copy, so the suite doesn't need it.

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
  (Caveat kept in mind: DevTools' `main` is the C# project and warp-agent's
  `main` the Avalonia prototype — the Rust apps ship on the branches above.)
- The apt dependency list in `suite-release.yml` covers GPUI's Linux needs
  (wayland, xkbcommon, vulkan, fontconfig); expect to touch it when GPUI's
  requirements grow.
