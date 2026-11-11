# warp-suite — agent instructions

This is the entry point for any agent working in this repo. Read it first, then
the app's own `AGENTS.md` when the work is in one app, then the deep guides in
`docs/agents/`.

## What this repo is

warp-suite is the monorepo for desktop apps and the Android dictation keyboard.
The desktop apps live under `apps/<name>/`, imported with `git subtree` so each
keeps its history; the shared `warpui-kit-*` crates live under `kits/`; one
Cargo workspace builds all of it and one release pipeline publishes it. The
Android app lives under `android-apps/` and has its own Gradle build; only its
Rust bridge is a workspace member.

| App | Package | Path | What it is |
|---|---|---|---|
| warp-station | `warp-station` + `warp-station-core` | `apps/warp-station/` | Repository workbench (Rust/GPUI) |
| warp-agent | `warp-agent` | `apps/warp-agent/` | Chat client over `opencode serve` and `dsh web` (Rust/GPUI) |
| warp-snapit | `warp-snapit` | `apps/warp-snapit/` | Windows-only zone snapper (Win32, no GPUI/kit) |
| warp-notes | `warp-notes` | `apps/warp-notes/` | Per-repo markdown notes with a live editor (Rust/GPUI) |
| warp-android-scribe | `warp-android-scribe-bridge` | `android-apps/warp-android-scribe/` | Android dictation IME with a full QWERTY keyboard over Superwhisper or Hex (Kotlin/Compose + Rust bridge) |

## Before you edit

1. Read this file and, for a UI or kit change, the matching guide in
   [`docs/agents/`](docs/agents/): design system, new-app recipe, and testing
   and captures.
2. Read the app's `AGENTS.md` (`apps/<name>/AGENTS.md` or
   `android-apps/<name>/AGENTS.md`). It wins over this file for app-specific
   facts.
3. Read the root [`Cargo.toml`](Cargo.toml), [`Cargo.lock`](Cargo.lock) and the
   touched crate's `Cargo.toml`. Note the pins and what is already a
   dependency before adding anything.
4. Match the existing module layout; reuse the shared kit crates instead of
   writing a second version of a shape.
5. For a gpui or gpui-kit API you are not certain about, read the pinned
   registry source first (see *Workspace, pins, toolchain*). Do not guess GPUI
   from general Rust or web UI knowledge.
6. The working tree is often dirty. Never revert or reformat unrelated
   changes.

## Workspace, pins, toolchain

- The root `Cargo.toml` is the only workspace. It has `resolver = "3"`, edition
  2024, and every app and kit crate as a member. Do not create a per-app
  workspace or a second lockfile.
- One [`Cargo.lock`](Cargo.lock) serves the whole suite. CI runs with
  `--locked`; keep it committed and do not hand-edit it.
- [`rust-toolchain.toml`](rust-toolchain.toml) pins channel `1.99`. Patch
  releases flow; bump the channel deliberately, in one edit, because CI reads
  it from this file.
- [`.cargo/config.toml`](.cargo/config.toml) sends every build to
  `.build/target`. Keep it that way: one GPUI dependency tree is shared by
  every app and kit.
- The GPUI stack is `gpui-kit 0.9`, `gpui-pre =0.3.11` and
  `gpui-pre-platform =0.3.11`, declared once in the root
  `[workspace.dependencies]`. Never add the real crates.io `gpui`; it is
  stale. Bump `gpui-kit`, `gpui-pre` and `gpui-pre-platform` together there,
  or the workspace will not resolve.
- [`apps.toml`](apps.toml) carries per-app build knobs (`package`,
  `linux_build`, `linux_skip`, `icon_prefix`, `scalable_svg`) and contract
  notes. [`scripts/manifest.py`](scripts/manifest.py) is the reader both build
  scripts and CI use. Add an `[apps.<name>]` section only when an app needs a
  non-default knob.

## Commands and gates

Use the package selector from the repo root. The app directory also works
because cargo walks up to the suite workspace.

```sh
cargo build -p warp-notes
cargo test -p warp-notes
cargo test -p warp-station -p warp-station-core
cargo test -p warpui-kit-core
cargo test -p warp-android-scribe-bridge
cargo fmt --all
cargo clippy --locked --all-targets -p warp-agent -- -D warnings
```

The CI gate is [`.github/workflows/kits.yml`](.github/workflows/kits.yml):

- `cargo fmt --all --check`.
- Clippy with `-D warnings` on all six kit crates, all app crates and the
  Android bridge.
- `cargo test --locked` on the kit crates, the app crates and the Android
  bridge.

The Android Gradle project is built separately and is not in that workflow:

```sh
cd android-apps/warp-android-scribe/android
JAVA_HOME=/usr/lib/jvm/java-17-openjdk ANDROID_HOME=$HOME/tools/android-sdk \
  ./gradlew assembleDebug
```

Match that locally for the crates you touched before calling a change done. A
passing probe is not the whole gate. Run `cargo fmt --all` (without `--check`)
to fix formatting rather than reporting it.

The clippy selectors are:

```sh
cargo clippy --locked --all-targets \
  -p warpui-kit-core -p warpui-kit-diff -p warpui-kit-markdown \
  -p warpui-kit-picker -p warpui-kit-platform -p warpui-kit-build \
  -p warp-station-core -p warp-station -p warp-agent -p warp-notes -p warp-snapit \
  -p warp-android-scribe-bridge \
  -- -D warnings
```

`cargo test` after a kit change builds the apps against that kit directly; use
it when a kit's public API moves.

If `cargo` is missing from an agent shell, prepend
`PATH="$HOME/.cargo/bin:$PATH"` for that command.

### Windows type-check

Plain `cargo check --target x86_64-pc-windows-msvc` dies in cc-rs on C
dependencies here. The Rust skill carries the working stub script:

```sh
$HOME/.agents/skills/rust/scripts/check-windows.sh -p warp-station
```

It type-checks the Rust code, `#[cfg(windows)]` blocks included, compiles no C,
and links nothing. Report it as a type-check, not a Windows build. The real
Windows build is the `windows-latest` job in
[`.github/workflows/suite-release.yml`](.github/workflows/suite-release.yml).

## Commits

- When the task you were given is finished, stop and ask whether to commit.
  That means: gates green on the crates you touched, no work left in the task,
  then one short question — "commit?" — not a commit.
- Never run `git commit` on your own initiative. Approving a plan, asking to
  keep going, or a follow-up task is not a yes to commit; only an explicit
  answer in the conversation is.
- Name what the commit would hold when the tree is not clean. It usually
  carries other sessions' work, and the owner decides whether it rides along.
- Stage the paths the task touched, not `git add -A`, and leave every other
  dirty file as it is.
- Once approved, follow the repo's existing style: `feat(station): ...`,
  `fix(agent): ...`, an English imperative subject.

## Cross-app contracts

The apps talk to each other. These are the interfaces that must move together;
when one side changes, update the other side in the **same commit**.

- **warp-agent door** (`~/.warp/agent/ipc.port` + `ipc.token`):
  token-authenticated NDJSON on `127.0.0.1`. Commands: `ping`,
  `open-workspace`, `new-chat`, `run-prompt`, `cancel`, `models`, `status`,
  plus status pushes.
  `new-chat`/`run-prompt` take a folder or `general` for warp-agent's built-in
  Tasks workspace, and take `harness`/`model`/`variant`. `models` returns the
  harnesses and each model's variants; `refresh: true` re-reads the backend
  instead of answering from the in-memory catalog. warp-agent owns the
  protocol; Warp Station is the client. A missing warp-agent binary disables
  Warp Station's AI surface; a closed one is launched on demand.
- **SnapIt settings** (`~/.warp/snapit`): warp-snapit and Warp Station read
  the same versioned snake_case `settings.json`/layouts. The old .NET-shaped
  `Settings.json`, `ExcludedApplicationSettings.json` and PascalCase layouts
  are importer input only. Points stay `"x,y"` and colours `"#AARRGGBB"`.
  Warp Station's `snapit.executable` setting names the Rust `warp-snapit.exe`
  (the default: the packaged `warp-snapit.exe` next to the app), never the
  retired .NET `Tools.SnapIt.exe`; the Rust port ships as a portable zip
  (`linux_skip = true`), not an installer.
- **Notes store** (default `~/notes`, `notes_store_path` overrides): one store
  root, repo notes under `projects/<repo>`, notes outside a repo at the root.
  Warp Station launches warp-notes with `--store` (its resolved
  `notes_store_path`) and an explicit
  scope: `--root` from the titlebar and the cross-repo surfaces (the whole
  store), `--repo` from a repo's launcher (that folder); a hit also passes
  `--note`. A standalone launch roots at the shared store. A store an older
  release kept in `~/.warp/notes` (or older `~/.devtools/notes`) moves on
  whichever app runs first. A missing warp-notes binary makes the notes
  surfaces fail with a toast; nothing else breaks. The hidden
  `<store>/.warp-notes/` folder is warp-notes' own (Trash, later templates);
  it is not a note folder and every walker skips dot directories.
- **Per-user layout** (`~/.warp`): each app has its own folder (`station/`,
  `agent/`, `notes/`) beside `snapit/`, the shared `ui.json` at the root and
  one shared `logs/`. Regenerable station/agent caches live under
  `<app>/cache/`; deleting one loses no user setting or repo tag. The notes
  content stays outside at `~/notes`.
  `warpui_kit_platform::paths` owns the tree and the startup migrations that
  adopt the older folders.
- **Settings and caches**: suite-owned JSON is versioned snake_case with a
  top-level `schema_version`; `docs/agents/settings-schema.md` is the contract.
  Station, agent, notes and SnapIt import their pre-versioning files once; the
  live store never normalizes keys.
- **Shared text size and palette** (`~/.warp/ui.json`): `text_size` is `small`,
  `normal`, `large` or `larger` (the old `compact` reads as normal); `palette`
  is `copper`, `plum`, `pine`, `blue` or `gray`. `warpui-kit-core` owns the
  file and the one control that writes each key. A click applies in that app
  at once; other apps read the value when they next start. Both keys are
  additive; a writer must not drop the key it does not own. Writes use a
  sidecar lock and an atomic rename. Reduced motion is per-app, not in this
  file.
- **Version and build facts**: `version.toml` is the one source of the suite
  version and the build counter; `scripts/version.py` is its only writer.
  `.githooks/pre-commit` bumps PATCH on every commit (never resets) and keeps
  `Cargo.toml`/`Cargo.lock` in step; `scripts/build.sh` and `scripts/build.ps1`
  bump build on every run. `kits/warpui-kit-build` stamps `WARP_SUITE_BUILD`,
  `WARP_SUITE_COMMIT`, `WARP_SUITE_CHANNEL` and `WARP_SUITE_RELEASED` into
  every binary (About, `--version`, the Windows file version, the bridge's
  `/health`); the APK derives its version from the same file. The env names and
  the `Copy version` line are a family contract: change the apps, the bridge,
  the build scripts and `docs/agents/versioning.md` together.
- **warp-android-scribe bridge** (`android-apps/warp-android-scribe`): LAN
  `http://<nas-lan>:8787` (UFW `<nas-subnet>`) and public
  `https://<nas-host>` through Cloudflare Tunnel -> Nginx Proxy
  Manager -> 8787. `POST /transcribe?engine=hex|superwhisper&lang=tr|auto&tone=…`
  with raw `audio/wav` and `Authorization: Bearer <token>` answers
  `{"text","language","duration"}`; `GET /health` is the phone's token-checked
  server test and `GET /engines` reports the two engines, their readiness and
  the default. Superwhisper keeps its CLI flow (transcribe → poll `status` for
  the same session id → text) and its profile endpoints; Hex is
  `hex model transcribe --model whisper_large_v3_turbo --language …`, one call
  per request with no profile, and the phone's tone is ignored for it. The
  phone picks the engine per request, so both must keep working.
  `GET /settings` returns the Superwhisper profile (language
  list, tone, vocabulary, processing modes and download progress),
  `PUT /settings/vocabulary` replaces the word/replacement lists, and
  `POST /settings/processing` selects Cloud/Offline/Offline Lite and downloads
  missing models in the background. The bridge owns the Superwhisper CLI flow
  (transcribe → poll `status` for the same session id → text) and serializes
  requests because the daemon has one active session; the language and tone
  pins are set inside that session mutex. The phone sends its tone on every
  request, so it overrides the NAS profile's tone.
  The bearer scheme, JSON field names and status codes are the Android client's
  contract: change both sides in the same commit. Android 17 LAN sockets need
  `ACCESS_LOCAL_NETWORK`; the app requests it and offers Local/Remote modes.
  The APK is sideloaded; the bridge is installed by hand on the NAS
  from `android-apps/warp-android-scribe/deploy/`.

## GPUI Kit app conventions

- Run `warpui_kit_core::bootstrap::init(cx, legacy_text_size, reduce_motion,
  retheme)` before the first window. It is the only place that calls
  `gpui_kit::init`, `adopt_text_size`, `adopt_palette`, `follow_palette`,
  `follow_ui_scale` and `motion::set_reduced`. Do not hand-roll the sequence.
- Bind app keybindings **after** that bootstrap, so they win ties against the
  bindings `gpui_kit::init` installs. Scope with action contexts (`"Input"`,
  `"Composer > Input"`), not global remaps.
- Motion is a state signal: a running or loading surface shows a spinner and a
  word at its row, in its head/title, and in the window's title bar and OS
  title. Every spinner has a word and an accessible name; reduced motion keeps
  the word. See `docs/agents/design-system.md#motion`.
- Content text renders as markdown through `warpui-kit-markdown` (read-only
  viewer or the live editor) and keeps its context menu; labels — buttons,
  rows, tabs, field and section labels, tool names, statuses — stay plain
  text with no markdown and no selection menu. The named `TextView`
  exceptions and the interaction each waits on are recorded in that section;
  a new exception must name its constraint and ship a probe. See
  `docs/agents/design-system.md#text-surfaces`.
- Keyboard list navigation belongs to the focused pane: each list owns a
  `FocusHandle` and its own `.on_key_down`; never add a window-level arrow
  handler. warp-notes' tree (`handle_tree_key`) is the reference. See
  `docs/agents/design-system.md#keyboard-navigation`.
- An app's `theme.rs` re-exports `warpui-kit-core` and adds only layout
  constants. App code holds no color literal, font family or font size. Read
  the live palette through the reader helpers; do not cache palette values in
  constants.
- Pass `current_ramp().accent_colors()` to `apply_kit_theme` so the accent
  follows the palette. Do not add another accent picker. `docs/agents/design-system.md`
  records the current per-app accent status.
- Assets: use `warpui_kit_platform::assets::KitBackedAssets` and keep
  `rust-embed`'s `debug-embed` feature. Without it, assets resolve from the
  process CWD in debug builds. Warp Station's generated `include_bytes!` table
  is the large-icon Windows exception, not the default.
- Paths: use `warpui_kit_platform::paths`; do not build `~/.warp` paths by
  hand. Run an app's legacy-layout migration before logging or settings read
  the moved folders.
- Logging: `logging::initialize()` returns a guard kept alive in `main`;
  install a panic hook at startup so crashes reach the log.
- Stateful kit components own their state in `Entity<State>`; render from the
  entity and mutate through events. `cx.notify()` is required when the UI must
  show a change. A panel over a scrim needs `.occlude()` and a mouse-down
  handler that stops propagation. Do not `unwrap` in render paths.
- A long-lived OS child needs a killer from `cx.on_app_quit`; dropping a
  `std::process::Child` leaves it running.
- This gpui rev does not service cross-thread channel wakeups on foreground
  tasks. Poll on a `background_executor().timer(..)` or drain on a timer where
  that matters.
- Prefer a `lib.rs` boundary so headless probes can build the real views. The
  binary stays a thin launcher; all three GPUI apps follow that split now,
  `warp-agent` included.

## Testing and captures

- UI integration tests are headless probes: `tests/<name>_probe.rs` with
  `#[gpui_kit::gpui::test]` (or `#[gpui::test]`), `TestAppContext` /
  `VisualTestContext`, real views through the `lib.rs` boundary.
- `gpui-kit/test-support` goes in `[dev-dependencies]` only.
- Import explicitly in probe files. `use gpui_kit::*;` or `use gpui::*;`
  brings gpui's `test` attribute into scope over std's and breaks
  `#[gpui_kit::gpui::test]` with a recursion-limit error.
- A probe that writes shared state runs under a scratch `HOME`; process-wide
  state takes turns behind a lock. See
  [`docs/agents/testing-and-captures.md`](docs/agents/testing-and-captures.md)
  for the full rules, the env hooks and the hidden-window capture recipe.
- Warp Station's `changes_probe` and `git_parity` read this repository's own
  git history; CI checks that job out with `fetch-depth: 0`. Do not shallow-
  clone it locally.
- The Android bridge has four integration probes: `api_probe` (auth, limits,
  temp-file cleanup, engine routing, gateway errors), `settings_probe` (a fake
  `superwhisper` shell script for the profile, vocabulary diff and processing
  download), `superwhisper_cli_probe` (session polling, refusal, warming retry,
  timeout) and `hex_cli_probe` (the Hex argument line, language narrowing, exit
  4, refusal, timeout). The Android Gradle project has no CI job; build it
  locally with `assembleDebug`.

## Platform rules

- Windows and Linux are both required for the GPUI apps; `warp-snapit` is
  Windows-only by design and skips Linux in `apps.toml`.
- Gate at the smallest scope that works. Use paired `cfg` lets when only the
  command differs, whole-function gates when the algorithm differs.
- Use `PathBuf`; never hardcoded separators or absolute Unix paths in shared
  code. Resolve a home from `HOME` then `USERPROFILE`.
- Gate OS-only dependencies with `[target.'cfg(windows)'.dependencies]` or
  `[target.'cfg(unix)'.dependencies]`. Never an unconditioned OS-only crate.
- Windows process spawns use `CREATE_NO_WINDOW` (`0x0800_0000`) so no console
  flashes. Packaged Windows exes hide the console with the `windows_subsystem`
  cfg attribute; debug builds keep it for logs.
- Set `app_id` on the GPUI window so Hyprland and launchers match the desktop
  entry's `StartupWMClass`.

## Packaging and release

- Apps ship **assets only**: `packaging/linux/{*.desktop,icons}` and
  `packaging/windows/{*.ico,*.iss,settings}`. There is no per-app `install.sh`
  or `build.ps1`. The suite owns every build and install step.
- [`scripts/build.sh`](scripts/build.sh) builds the Linux bundle and tarball;
  [`scripts/build.ps1`](scripts/build.ps1) builds Windows zips and Inno Setup
  installers; [`scripts/deploy.sh`](scripts/deploy.sh) and
  [`scripts/deploy.ps1`](scripts/deploy.ps1) install/uninstall;
  [`scripts/bundle-install.sh`](scripts/bundle-install.sh) runs inside an
  extracted bundle.
- A Windows installer is built only when `packaging/windows/<app>.iss` exists;
  without one, `build.ps1` ships a portable zip.
- Keep the desktop entry's `Exec=__WARP_SUITE_BIN__/...` placeholder; the
  build resolves it at install time.
- A release is a tag on a commit whose `version.toml` says that version,
  dispatched with
  `gh workflow run suite-release.yml --ref v<version> -f version=<version>`.
  The workflow refuses a mismatch or a non-tag ref. One release builds every
  app at that commit and records it in the release body and the Linux
  `REVISIONS.txt`; see `docs/agents/versioning.md`.
- `android-apps/` is outside that pipeline. `warp-android-scribe` ships as a
  sideloaded debug APK (`./gradlew assembleDebug`); its bridge is a systemd
  user service on the NAS. Never add the Android app to `apps.toml` to make the
  desktop build pick it up.

## Adding an app

1. Import it with history: `git subtree add --prefix=apps/<name> <repo> <ref>`.
2. Add an `[apps.<name>]` section to [`apps.toml`](apps.toml) only for
   non-default build knobs. Add `package = "..."` when the directory name is
   not the cargo package name.
3. Add it to the root workspace `members` (both build scripts loop over
   `apps.toml`, but cargo needs the member) and, for every dependency more
   than one member uses, to `[workspace.dependencies]`.
4. Add a shared crate to `kits/` only when it is needed by more than one app;
   otherwise keep it in the app. A new kit is a member and a
   `[workspace.dependencies]` path entry like the others.
5. Document the cross-app contracts in the README and in this file.
6. Add `apps/<name>/AGENTS.md` for the app-specific facts.

## Plans and proposals

- Write plans, proposals and migration checklists to `plans/<topic>.md`, one
  file per topic in kebab-case. Do not create `PLAN*.md` at the repo root or
  inside `apps/<name>/`; `plans/` is repo-wide, so app work files there too.
- A plan is a working file, not shipped documentation: start with a status line
  and the decisions that still need settling. Do not restate the code.
- When the work lands, delete the plan or fold anything still useful into
  `docs/agents/` or the app's `README.md`. Do not leave finished plans behind.

## Traps and do-nots

- Do not `git subtree pull` or `git subtree push`. warp-suite is the source of
  truth; the old per-app repos are read-only.
- Do not add a second workspace, lockfile, target dir, palette, accent picker or
  `gpui` dependency.
- Do not edit generated output (`dist/`, `.build/`, `target/`, `out/`).
- Do not add a file watcher for `~/.warp/ui.json`; other apps read it at their
  next start by design.
- Do not run `hypruse launch` on these apps for a capture; `misc:focus_on_activate`
  pulls the owner's screen. Use the hidden launch recipe in
  `docs/agents/testing-and-captures.md`.
- Do not "fix" a UI by adding a color or size literal. Add it to
  `warpui-kit-core` if the family needs it.
- Do not silence clippy. Fix the source, or use
  `#[expect(clippy::..., reason = "...")]` on that one item.
- Do not add `android-apps/*` to `apps.toml`, `scripts/build.sh` or the suite
  release; the Gradle build and the APK are separate.
- Do not remove the bearer token, the UFW LAN rule or HTTPS from the bridge.
  The public endpoint is `https://<nas-host>`; the token is the sole
  auth there and must not be logged, committed or shared. Add Cloudflare
  Access/WAF before exposing it further.

## Docs map

- [`README.md`](README.md) — the suite, build/deploy, cross-app contracts.
- [`docs/agents/design-system.md`](docs/agents/design-system.md) — palettes,
  type, scale, startup, motion, controls; the design source of truth.
- [`docs/agents/gpui-app-recipe.md`](docs/agents/gpui-app-recipe.md) — the
  annotated skeleton for a new GPUI app in this suite.
- [`docs/agents/testing-and-captures.md`](docs/agents/testing-and-captures.md)
  — probes, env hooks and the hidden-window capture recipe.
- [`docs/agents/settings-schema.md`](docs/agents/settings-schema.md) — the
  versioned snake_case settings/cache contract.
- [`docs/agents/versioning.md`](docs/agents/versioning.md) — `version.toml`,
  the PATCH/build rules, the hook, Windows/Android facts and the release flow.
- [`plans/`](plans/) — working design plans and proposals, one file per topic;
  delete a plan once its work lands.
- [`kits/README.md`](kits/README.md) — what each kit crate provides and where a
  shared shape belongs.
- [`android-apps/warp-android-scribe/README.md`](android-apps/warp-android-scribe/README.md)
  — the Android IME, the bridge API, NAS install and the decisions.
- Each app's `README.md` and `AGENTS.md` — app-specific detail.
