# warp-suite

Monorepo for Engin's connected apps. Each desktop app lives under `apps/<name>`,
imported with `git subtree` so its history stays intact; warp-suite builds
them together and publishes **one combined release**. The Android dictation
keyboard lives under `android-apps/` and is built and sideloaded separately.

Currently in the suite:

| App | Path | What it is |
|---|---|---|
| warp-station | `apps/warp-station` | Repository workbench (Rust/GPUI workspace) |
| warp-agent | `apps/warp-agent` | Chat client over `opencode serve` (Rust/GPUI) |
| warp-snapit | `apps/warp-snapit` | Rust port of the SnapIt zone snapper (Windows-only, portable zip) |
| warp-notes | `apps/warp-notes` | Per-repo markdown notes with a live editor (Rust/GPUI) |
| warp-android-scribe | `android-apps/warp-android-scribe` | Android dictation IME with a full QWERTY keyboard over Superwhisper S1-Voice (Kotlin/Compose + Rust bridge) |

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
suite. `android-apps/` is outside this pipeline: the Android APK is a
separate Gradle build and is sideloaded; see
[`android-apps/warp-android-scribe/README.md`](android-apps/warp-android-scribe/README.md).

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
| `AGENTS.md` | suite-wide agent instructions (pins, gates, contracts, traps) |
| `apps/<name>/` | the app sources, each with its own history (imported with `git subtree`) |
| `android-apps/warp-android-scribe/` | the Android IME and its Rust HTTP bridge; separate Gradle build, not in the suite release |
| `apps/<name>/AGENTS.md` | app-specific agent instructions |
| `plans/` | working design plans and proposals, one markdown file per topic |
| `apps.toml` | per-app build knobs (`linux_build`, `linux_skip`, `icon_prefix`, `scalable_svg`) and contract notes |
| `scripts/manifest.py` | apps.toml reader shared by the scripts + CI |
| `.cargo/config.toml` | one cargo target dir for the whole suite: `.build/target` for every app, `kits` and `scripts/build.sh` |
| `kits/` | the five `warpui-kit-*` crates, members of the suite workspace; their paths live in the root `[workspace.dependencies]` |
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

## Agent instructions

Agents start at [`AGENTS.md`](AGENTS.md) for the suite-wide rules, then the
matching `apps/<name>/AGENTS.md`; the deep guides are in `docs/agents/`,
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
  `snapit.executable` setting points at the Rust `warp-snapit.exe`, which is a
  portable-zip deliverable (no installer) and skipped in the Linux bundle
  (`linux_skip = true` in `apps.toml`).
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
  user service installed from `android-apps/warp-android-scribe/deploy/`.

## Adding an app

1. Import it with history: `git subtree add --prefix=apps/<name> <repo> <ref>`.
2. Add a section to `apps.toml` if it needs non-default build knobs
   (`linux_build`/`linux_skip`/`icon_prefix`/`scalable_svg`) — both build
   scripts read that one manifest, so `build.sh` and `build.ps1` pick it up
   automatically. Windows compiles an installer only when
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
- **Kit crates:** the apps depend on `warpui-kit-core`/`-diff`/`-markdown`/`-picker`/`-platform`
  through path entries in the root `[workspace.dependencies]`. They live in
  this repo as members of the one suite workspace (root `Cargo.toml`), so a
  release builds from this repo alone. The suite commit is the kits'
  provenance — no sync step, no
  second copy. Everything resolves from the one committed root
  `Cargo.lock`, so `cargo --locked` in CI resolves the same versions every
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
