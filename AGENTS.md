# warp-suite — agent instructions

This is the entry point for any agent working in this repo. Read it first, then
the app's own `AGENTS.md` when the work is in one app, then the deep guides in
`docs/agents/`.

## What this repo is

warp-suite is the kit and release repository. The shared `warpui-kit-*` crates
live under `kits/`; each app lives in its own repository and pins a kit tag.
Locally `apps/<name>` is a symlink to the sibling checkout, and the root
workspace builds the kits plus whatever apps are linked. The Android app has
its own repository and Gradle build; its Rust bridge is not a member here.

| App | Package | Repository | What it is |
|---|---|---|---|
| warp-station | `warp-station` + `warp-station-core` | [warp-station](https://github.com/enginkirmaci/warp-station) | Repository workbench (Rust/GPUI) |
| warp-agent | `warp-agent` | [warp-agent](https://github.com/enginkirmaci/warp-agent) | Chat client over `opencode serve` and `dsh web` (Rust/GPUI) |
| warp-snapit | `warp-snapit` | [warp-snapit](https://github.com/enginkirmaci/warp-snapit) | Windows-only zone snapper (Win32, no GPUI/kit) |
| warp-notes | `warp-notes` | [warp-notes](https://github.com/enginkirmaci/warp-notes) | Per-repo markdown notes with a live editor (Rust/GPUI) |
| warp-android-scribe | `warp-android-scribe-bridge` | [warp-android-scribe](https://github.com/enginkirmaci/warp-android-scribe) | Android dictation IME with a full QWERTY keyboard over Superwhisper or Hex (Kotlin/Compose + Rust bridge) |

## Local setup

Each app lives in its own repository; `apps/` holds symlinks to the sibling
checkouts. The full recipe is
[`docs/agents/local-setup.md`](docs/agents/local-setup.md); the short version:

```sh
gh repo clone enginkirmaci/warp-suite && cd warp-suite
scripts/dev/link-apps.sh --check      # report the links, change nothing
scripts/dev/link-apps.sh              # create apps/<name> symlinks (junctions on Windows)
python3 scripts/meta/version.py install-hooks
cargo build -p warp-notes         # proves the linked workspace
```

`scripts/dev/link-apps.sh` never replaces a real `apps/<name>` directory (that
would hide a checkout). App checkouts carry no `[workspace]` table on purpose;
the root `apps/*` member glob and the kit `[patch]` table are what make the
symlinked build work.

## Before you edit

1. Read this file and, for a UI or kit change, the matching guide in
   [`docs/agents/`](docs/agents/): design system, new-app recipe, and testing
   and captures.
2. Read the app repository's `AGENTS.md` (the `apps/<name>/AGENTS.md` symlink
   points at it, or open the Android repository's own file). It wins over this
   file for app-specific facts.
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

- The root `Cargo.toml` is the kit workspace. It has `resolver = "3"`, edition
  2024, `kits/*` as members and an `apps/*` glob that adds whatever app
  checkouts are linked locally. App repositories have their own workspace,
  lockfile and version; do not add their crates as explicit members here.
- One [`Cargo.lock`](Cargo.lock) serves the kits and the linked apps. A linked
  app build extends it locally with that app's dependencies; do not commit that
  extension. The suite CI runs without `--locked` because the linked set is
  machine-local; the app repositories keep `--locked` on their own lockfiles.
- [`rust-toolchain.toml`](rust-toolchain.toml) pins channel `1.99`. Patch
  releases flow; bump the channel deliberately, in one edit, because CI reads
  it from this file.
- [`.cargo/config.toml`](.cargo/config.toml) sends every build to
  `.build/target`. Keep it that way: one GPUI dependency tree is shared by
  every app and kit.
- The kit workspace pins `gpui` (`gpui-pre =0.3.11`) and `gpui-kit 0.9` in the
  root `[workspace.dependencies]`. Each app repository repeats the same pins in
  its own manifest. Never add the real crates.io `gpui`; it is stale. Bump
  `gpui`, `gpui-pre-platform` where an app uses it, and `gpui-kit` together, or
  the linked build will not resolve.
- [`suite.toml`](suite.toml) is the pin table: the kit tag and one tag per app.
  Each app's `app.toml` carries its build knobs (`package`, `linux_skip`,
  `icon_prefix`, `scalable_svg`).
  [`scripts/meta/manifest.py`](scripts/meta/manifest.py) reads both.

## Commands and gates

Use the package selector from the repo root. The app directory also works
because cargo walks up to the suite workspace.

`cargo build -p <app>` needs `scripts/dev/link-apps.sh` to have linked that
app. With no links the workspace is kit-only, which is what CI builds.

```sh
cargo build -p warp-notes
cargo test -p warp-notes
cargo test -p warp-station -p warp-station-core
cargo test -p warpui-kit-core
cargo fmt --all
cargo clippy --all-targets -p warpui-kit-core -- -D warnings
```

There is no kit CI workflow; run the gate locally before changing a kit:

- `cargo fmt --all --check`.
- Clippy with `-D warnings` on the six kit crates.
- `cargo test` on the six kit crates.

Each app's own `ci.yml` runs that app's fmt, clippy and tests. The Android
Gradle project is built in its own repository:

```sh
# in the warp-android-scribe checkout
cd android
JAVA_HOME=/usr/lib/jvm/java-17-openjdk ANDROID_HOME=$HOME/tools/android-sdk \
  ./gradlew assembleDebug
```

Match that locally for the crates you touched before calling a change done. A
passing probe is not the whole gate. Run `cargo fmt --all` (without `--check`)
to fix formatting rather than reporting it.

The kit clippy selectors are:

```sh
cargo clippy --all-targets \
  -p warpui-kit-core -p warpui-kit-diff -p warpui-kit-markdown \
  -p warpui-kit-picker -p warpui-kit-platform -p warpui-kit-build \
  -- -D warnings
```

`cargo test -p <app>` after a kit change builds the linked app against that kit
directly; use it when a kit's public API moves. With the app checkouts linked,
`cargo test --workspace` covers the kits and the apps together.

If `cargo` is missing from an agent shell, prepend
`PATH="$HOME/.cargo/bin:$PATH"` for that command.

### Windows type-check

Plain `cargo check --target x86_64-pc-windows-msvc` dies in cc-rs on C
dependencies here. The Rust skill carries the working stub script:

```sh
# in the app repository checkout, e.g. ../warp-station
$HOME/.agents/skills/rust/scripts/check-windows.sh -p warp-station
```

It type-checks the Rust code, `#[cfg(windows)]` blocks included, compiles no C,
and links nothing. Report it as a type-check, not a Windows build. The real
Windows build is the app repository's `release` workflow.

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

The apps talk to each other across repositories. These are the interfaces that
must move together: land both sides in the same change window and let the
suite pin table and each app's CI carry the pair.

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
- **Version and build facts**: each repository owns its `version.toml`. The kit
  line is warp-suite's; each app line is the app repository's. A pre-commit
  hook bumps that line's PATCH on every commit (never resets); the
  repository's build script bumps its build counter. `kits/warpui-kit-build`
  stamps the app's own `WARP_BUILD` family into each binary (About,
  `--version`, the Windows file version, the bridge's `/health`); the APK
  reads its repository's file. The `Copy version` line is the family contract:
  change the apps, the bridge, the build scripts and
  `docs/agents/versioning.md` together.
- **warp-android-scribe bridge** (the `warp-android-scribe` repository): LAN
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
  The APK is sideloaded; the bridge is installed by hand on the NAS from that
  repository's `deploy/` directory.

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
- Warp Station's `changes_probe` and `git_parity` read the app repository's own
  git history; its CI checks that job out with `fetch-depth: 0`. Do not
  shallow-clone the app repo locally.
- The Android bridge has four integration probes: `api_probe` (auth, limits,
  temp-file cleanup, engine routing, gateway errors), `settings_probe` (a fake
  `superwhisper` shell script for the profile, vocabulary diff and processing
  download), `superwhisper_cli_probe` (session polling, refusal, warming retry,
  timeout) and `hex_cli_probe` (the Hex argument line, language narrowing, exit
  4, refusal, timeout). The probes run in the Android repository's CI; the
  Gradle project builds with `assembleDebug`.

## Platform rules

- Windows and Linux are both required for the GPUI apps; `warp-snapit` is
  Windows-only by design and skips Linux in its `app.toml`.
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

- Each app repository owns its packaging assets
  (`packaging/linux/{*.desktop,icons}`, `packaging/windows/{*.ico,*.iss,settings}`)
  and its own `release` workflow: Linux tarball, Windows portable zip and,
  where a `.iss` exists, the Inno Setup installer.
- warp-suite owns the kit build ([`scripts/build/kit-build.sh`](scripts/build/kit-build.sh)),
  the suite assembler and the install scripts. The local
  [`scripts/build/build.sh`](scripts/build/build.sh) /
  [`scripts/build/build.ps1`](scripts/build/build.ps1) build the linked
  workspace; [`scripts/deploy/deploy.sh`](scripts/deploy/deploy.sh),
  [`scripts/deploy/deploy.ps1`](scripts/deploy/deploy.ps1) and
  [`scripts/deploy/bundle-install.sh`](scripts/deploy/bundle-install.sh) /
  [`scripts/deploy/bundle-install.ps1`](scripts/deploy/bundle-install.ps1)
  install the result.
- A Windows installer is built only when the app ships
  `packaging/windows/<app>.iss`; without one the app release is a portable zip.
  The suite release does not republish those per-app assets; it combines the
  portable zips into one Windows ZIP with `bundle-install.ps1`.
- Keep the desktop entry's `Exec=__WARP_SUITE_BIN__/...` placeholder; the
  build resolves it at install time.
- A suite release is the [`suite.toml`](suite.toml) pin table:
  `suite-release.yml` downloads those app releases, verifies their checksums
  and publishes one Linux tarball and one Windows ZIP. The app repositories
  are public, so `GITHUB_TOKEN` is enough; it never rebuilds an app from
  sources.
- `warp-android-scribe` ships from its own repository: a sideloaded debug APK
  (`./gradlew assembleDebug`) and a bridge installed as a systemd user service
  on the NAS. Never add it to `suite.toml` or the desktop build.

## Adding an app

1. Create the app repository with its own `version.toml`, `app.toml`,
   `Cargo.toml` (no `[workspace]` table), CI and release workflow, and the kit
   git pins.
2. Add an `[apps.<name>]` section to [`suite.toml`](suite.toml) with its repo
   and tag.
3. Run `scripts/dev/link-apps.sh`; `apps/<name>` appears next to the others.
4. Add a shared crate to `kits/` only when it is needed by more than one app;
   otherwise keep it in the app.
5. Document the cross-app contracts in the README and in this file.

## Plans and proposals

- Write plans, proposals and migration checklists to `plans/<topic>.md`, one
  file per topic in kebab-case. Do not create `PLAN*.md` at the repo root or
  inside `apps/<name>/`; `plans/` is repo-wide, so app work files there too.
- A plan is a working file, not shipped documentation: start with a status line
  and the decisions that still need settling. Do not restate the code.
- When the work lands, delete the plan or fold anything still useful into
  `docs/agents/` or the app's `README.md`. Do not leave finished plans behind.

## Public history and secrets

- Never commit a token, PAT, password, private IP, Wi-Fi SSID, home path or
  personal email in file content. Use placeholders in docs and examples.
- A commit's author and committer are public. Use a public identity (the
  GitHub noreply address), never a private address.
- Pass credentials through GitHub Actions secrets only; never echo or log
  them.
- Before publishing, force-pushing or adding a remote, run this over the
  current tree and every ref:

  ```sh
  git grep -n -i -E '192\.168\.[0-9]+\.[0-9]+|10\.[0-9]+\.[0-9]+\.[0-9]+|/home/[a-z]+|ghp_|gho_|github_pat_|ghs_' -- .
  ```

## Traps and do-nots

- Do not copy app sources into `apps/`; the directory holds symlinks and the
  link script is the only writer. App code lives in its own repository.
- Do not add an app to `suite.toml` before its release tag exists.
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
- Do not add `warp-android-scribe` to `suite.toml`, `scripts/build/build.sh` or
  the suite release; the Gradle build and the APK are separate.
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
- [`docs/agents/local-setup.md`](docs/agents/local-setup.md) — the sibling
  checkouts, the `apps/` symlinks and the one-workspace local build.
- [`plans/`](plans/) — working design plans and proposals, one file per topic;
  delete a plan once its work lands.
- [`kits/README.md`](kits/README.md) — what each kit crate provides and where a
  shared shape belongs.
- [`warp-android-scribe`](https://github.com/enginkirmaci/warp-android-scribe)
  — the Android IME, the bridge API, NAS install and the decisions.
- Each app's `README.md` and `AGENTS.md` — app-specific detail.
