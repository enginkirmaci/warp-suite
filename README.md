# warp-suite

The kit and release repository for Engin's connected apps. The six
`warpui-kit-*` crates live under `kits/`; every app lives in its own repository
and pins a kit tag. warp-suite builds and releases the kits, assembles the
combined suite release from pinned app releases, and keeps `apps/` as local
symlinks to the sibling checkouts so one workspace still builds everything
against the local kits.

Currently in the suite:

| App | Repository | What it is |
|---|---|---|
| warp-station | [warp-station](https://github.com/enginkirmaci/warp-station) | Repository workbench (Rust/GPUI) |
| warp-agent | [warp-agent](https://github.com/enginkirmaci/warp-agent) | Chat client over `opencode serve` and `dsh web` (Rust/GPUI) |
| warp-snapit | [warp-snapit](https://github.com/enginkirmaci/warp-snapit) | Rust port of the SnapIt zone snapper (Windows-only, portable zip) |
| warp-notes | [warp-notes](https://github.com/enginkirmaci/warp-notes) | Per-repo markdown notes with a live editor (Rust/GPUI) |
| warp-android-scribe | [warp-android-scribe](https://github.com/enginkirmaci/warp-android-scribe) | Android dictation IME with a full QWERTY keyboard over Superwhisper or Hex (Kotlin/Compose + Rust bridge) |

The C#/.NET SnapIt repo ([enginkirmaci/SnapIt](https://github.com/enginkirmaci/SnapIt))
stays excluded — the Rust port above is the suite's zone snapper.
See [Adding an app](#adding-an-app).

## Deploy

A suite release is a pin table ([`suite.toml`](suite.toml)): the kit tag plus
one tag per app. Each app release is built and published in its own repository;
the suite job downloads the pinned releases and assembles one Linux tarball and
one Windows ZIP.

Tag this repo with the suite version and dispatch the assembler:

```sh
git tag -a "v0.1.0" -m "warp-suite 0.1.0"
git push origin "v0.1.0"
gh workflow run suite-release.yml --ref v0.1.0 -f version=0.1.0
```

The job downloads the app releases pinned in `suite.toml`, verifies their
`SHA256SUMS`, and publishes the combined Linux bundle and Windows ZIP. All app
repositories are public, so `GITHUB_TOKEN` is enough; the job never rebuilds an
app.

## Install on a Linux machine

```sh
git clone https://github.com/enginkirmaci/warp-suite && cd warp-suite
scripts/dev/link-apps.sh        # link the sibling app checkouts
./install.sh --local            # build the linked workspace, then install
```

Binaries go to `~/.local/bin`, desktop entries + hicolor icons to
`~/.local/share`. Everything is per-user; no root needed.

## Install on Windows

Download `warp-suite-<version>-windows-x64.zip` from the suite release,
extract it, and run `install.cmd`. It installs under
`%LOCALAPPDATA%\Programs\warp-suite`, adds a `warp-suite` Start Menu folder
with one shortcut per app, and records what it wrote for `uninstall.cmd`.

The same ZIP is portable: run the executables under `apps\` directly without
installing. It contains Warp Station, Warp Agent, Warp Notes and Warp SnapIt.

## Build and deploy locally

```sh
scripts/dev/link-apps.sh                  # once per checkout
scripts/build/build.sh                    # Linux bundle + tarball
scripts/build/build.sh --app warp-notes   # one app, fast loop
scripts/deploy/deploy.sh --local          # build, then install into ~/.local
scripts/deploy/deploy.sh --uninstall      # remove exactly what it installed
```

```powershell
scripts\dev\link-apps.ps1          # junctions on Windows
scripts\build\build.ps1            # Windows: dist\windows\ zips + installers
scripts\build\build.ps1 -App warp-notes -NoPackage
scripts\deploy\deploy.ps1 -Local   # build, then install a portable copy
scripts\deploy\deploy.ps1 -Release # install the published Windows ZIP
scripts\deploy\deploy.ps1 -Uninstall
```

Both builds default the version to `git describe --tags --always
--first-parent`; CI passes `--version` / `-Version` explicitly.

## Layout

| Path | Purpose |
|---|---|
| `AGENTS.md` | suite-wide agent instructions (pins, gates, contracts, traps) |
| `apps/<name>` | local symlinks to the sibling app checkouts (gitignored); `scripts/dev/link-apps.sh` creates them |
| `plans/` | working design plans and proposals, one markdown file per topic |
| `suite.toml` | the pin table: the kit tag and one tag per app |
| `scripts/meta/manifest.py` | suite.toml + app.toml reader shared by the scripts + CI |
| `.cargo/config.toml` | one cargo target dir for the whole suite: `.build/target` for every app, `kits` and `scripts/build/build.sh` |
| `kits/` | the six `warpui-kit-*` crates, members of the kit workspace; their paths live in the root `[workspace.dependencies]` |
| `scripts/build/build.sh` | Linux build: `cargo build --release` each app → `dist/linux/warp-suite-<v>-linux/` + tarball |
| `scripts/build/build.ps1` | Windows build: `dist/windows/` portable zips + Inno Setup installers |
| `scripts/deploy/deploy.sh` | Linux install/uninstall: local bundle or published release |
| `scripts/deploy/deploy.ps1` | Windows install/uninstall: portable locally, the combined ZIP from a release |
| `scripts/deploy/bundle-install.sh` | runs inside an extracted bundle; installs into `~/.local` and records what it wrote |
| `install.sh` | shim over `scripts/deploy/deploy.sh` for the documented one-liner |
| `.github/workflows/suite-release.yml` | the combined release pipeline |
| `design/v3/` | the committed design contract (DESIGN.md + prototype source); the live source is the OpenDesign project `warp-suite-v3` |

Each app repository owns its packaging assets and its own build/release
workflows. warp-suite owns the kit build, the suite assembly and the install
scripts.

## Agent instructions

Agents start at [`AGENTS.md`](AGENTS.md) for the suite-wide rules, then the
app repository's `AGENTS.md`; the deep guides are in `docs/agents/`,
including [`settings-schema.md`](docs/agents/settings-schema.md). Design plans
and proposals go in [`plans/`](plans/), one file per topic, and are deleted
once the work lands.

## Cross-app contracts

The apps talk to each other; these are the interfaces that must move
together. When one changes, update the other side in the **same** commit.

- **warp-agent loopback door** (`~/.warp/agent/ipc.port` + `ipc.token`,
  token-authenticated NDJSON on `127.0.0.1`): commands `open-workspace`,
  `new-chat`, `run-prompt`, `models`, `status` (+ status pushes).
  `new-chat`/`run-prompt` take a folder, or `general` for warp-agent's
  built-in Tasks workspace, where the AI wands run: a wand prompt carries its
  own diff or file tree, so it must not add the repo to warp-agent's sidebar
  as a workspace.
  `new-chat`/`run-prompt` accept `harness`/`model`/`variant`, and `models`
  answers with the harnesses and each model's variants. `models` also takes
  `refresh: true`: the door re-reads that harness's backend (starting it when
  needed) instead of answering from its in-memory catalog, which is what the
  settings refresh buttons ask for; without the flag the answer may be the
  catalog loaded earlier in the session. warp-agent owns the protocol; Warp
  Station is the client — a missing warp-agent binary turns Warp Station's AI
  features off, a closed one is launched on demand.
- **SnapIt settings**: [warp-snapit](https://github.com/enginkirmaci/warp-snapit)
  and Warp Station read the same versioned snake_case `~/.warp/snapit`
  `settings.json`/layouts. The old .NET-shaped files are importer input only;
  `"x,y"` points and `"#AARRGGBB"` colours stay. Warp Station's
  `snapit.executable` setting names the Rust `warp-snapit.exe` (empty = the
  packaged exe next to the app), which is a portable-zip deliverable (no
  installer) and skipped in the Linux bundle (`linux_skip = true` in
  `warp-snapit/app.toml`).
- **Shared notes store** (default `~/notes`, `notes_store_path` overrides; one
  repo per folder under `projects/`):
  [warp-notes](https://github.com/enginkirmaci/warp-notes) always gets
  `--store` (Warp Station's resolved `notes_store_path`) and an explicit
  scope — the titlebar toggle and the global-search note hits ask for
  `--root` (the whole store), a repo's launcher pins its folder with
  `--repo` — plus `--note` when a hit names one. Standalone launches root at
  the shared store; a store an older release kept in `~/.warp/notes` (or the
  older `~/.devtools/notes`) moves to `~/notes` on whichever app runs first.
  Missing warp-notes binary = the notes surfaces fail with a toast, nothing
  else breaks.
- **Per-user layout** (`~/.warp`): every app's own data sits in its own
  folder — `station/`, `agent/`, `notes/` — beside the SnapIt settings in
  `snapit/`, the shared `ui.json` at the root and one `logs/` for every
  app's rolling file. Station and agent keep regenerable caches in
  `<app>/cache/`; deleting one loses no user setting or repo tag. The notes
  store is user content, not app data, so it stays outside at `~/notes`.
  `warpui_kit_platform::paths` owns the tree and the startup migrations that
  adopt the older folders.
- **Settings and caches**: suite-owned JSON is versioned snake_case; the
  contract and the importer rules are in
  [`docs/agents/settings-schema.md`](docs/agents/settings-schema.md). Every app,
  SnapIt included, imports its pre-versioning files once at the edge.
- **Shared text size and palette** (`~/.warp/ui.json`: `text_size` is `small`,
  `normal`, `large` or `larger` — the old `compact` spelling reads as normal —
  and `palette` is `copper`, `plum`, `pine`, `blue` or `gray`):
  `warpui-kit-core` owns the file and the one control that writes each key. A
  click applies in that app at once; the other apps read the value when they
  next start. The apps' old per-app `text_size`/`font_scale` settings only seed
  the file on its first run, so every app must be on the same
  `warpui-kit-core`. Both keys are additive: a file that lacks one keeps the
  other.

- **warp-android-scribe bridge** (LAN `<nas-lan>:8787`, public
  `https://<nas-host>`): the Android IME posts a 16 kHz mono WAV to
  `POST /transcribe?lang=tr|auto&tone=semi-formal|formal|…` with
  `Authorization: Bearer <token>` and receives
  `{"text","language","duration"}`. `GET /health` is the token-checked server
  test; `GET /settings`, `PUT /settings/vocabulary` and
  `POST /settings/processing` read and edit the NAS profile (language list,
  tone, words, processing mode and model downloads). The bridge turns the
  request into `superwhisper transcribe` plus `status` polling on the NAS and
  serializes requests because the daemon has one active session; language and
  tone are set inside that mutex. The public route is Cloudflare Tunnel ->
  Nginx Proxy Manager -> 8787. The bearer scheme, field names and status codes
  are one contract: change the bridge and the Android client in the same
  commit. Android 17 needs `ACCESS_LOCAL_NETWORK` for LAN; the app requests it
  and has Local/Remote modes. The APK is sideloaded; the bridge is a systemd
  user service installed from `warp-android-scribe/deploy/`.

- **Version and build facts**: each repository owns its `version.toml`. The
  kit line moves PATCH on every warp-suite commit through its pre-commit hook;
  each app line does the same in its own repository. Build counters move on
  every build-script run in that repository, and `kits/warpui-kit-build`
  stamps the app's own facts into each binary (About, `--version`, the Windows
  file version, the bridge's `/health`). The APK reads its repository's file.
  One `Copy version` line
  (`Warp Station 0.1.8 (build 42 · d904edc) · dev`) is the family contract;
  `docs/agents/versioning.md` is the rulebook.

## Adding an app

1. Create the app repository; it carries its own `version.toml`, `app.toml`,
   CI and release workflows, and pins the kit tag in `Cargo.toml`.
2. Add its tag to [`suite.toml`](suite.toml).
3. Run `scripts/dev/link-apps.sh`; `apps/<name>` appears next to the others.
4. Document its contracts in [Cross-app contracts](#cross-app-contracts).

The app's `app.toml` carries the build knobs (`package`, `linux_skip`,
`icon_prefix`, `scalable_svg`); both suite build scripts read it through the
symlink. Windows compiles an installer only when
`packaging/windows/<app>.iss` exists.

## Notes

- The apps live in their own repositories. `warp-suite` is the kit source and
  the release pin table; it is not a second source of truth for app code.
- `suite.toml` pins one tag per app and the kit tag. A suite release names
  exactly those tags; `REVISIONS.txt` records them.
- The apt dependency list in `suite-release.yml` covers GPUI's Linux needs
  (wayland, xkbcommon, vulkan, fontconfig); expect to touch it when GPUI's
  requirements grow.
- **Kit crates:** the apps depend on `warpui-kit-core`/`-diff`/`-markdown`/`-picker`/`-platform`
  through path entries in the root `[workspace.dependencies]`. They live in
  this repo as members of the one suite workspace (root `Cargo.toml`), so a
  release builds from this repo alone. The suite commit is the kits'
  provenance — no sync step, no
  second copy. Everything resolves from the one committed root
  `Cargo.lock`, so CI resolves the same versions every
  run; `cargo test` after changing a kit builds the apps against it
  directly.

## CI caching

Both workflows use `Swatinem/rust-cache`, and both must name the target dir
explicitly (`workspaces: <workspace> -> <target>`):

- `.cargo/config.toml` sends every workspace to `.build/target`, so the
  action's default `<workspace>/target` does not exist. Without the
  explicit path the action caches only the registry and every run compiles
  from scratch.
- The whole suite is one workspace now, so each job names it once
  (`workspaces: . -> .build/target`) with its own `shared-key`
  (`release-linux`/`release-windows` in the release workflow, `kits`/`apps`
  in the gate). Earlier schemes that listed several workspaces hit a prune
  war — the action prunes a target dir down to the owning workspace's
  dependency closure, several workspaces meant several prune passes over
  the same tree and the last one won, deleting everything the others had
  built. One workspace retired the problem; the workflow comments keep the
  history.

The target dir is ~4.5 GB release / ~2.9 GB debug, so the caches are pruned
rather than uploaded whole (the repo cache limit is 10 GB).
