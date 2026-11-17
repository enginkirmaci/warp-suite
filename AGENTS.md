# warp-suite — agent instructions

This is the entry point for any agent working in this repo. Read it first, then
the app's own `AGENTS.md` when the work is in one app, then the deep guides in
`docs/agents/`.

## What this repo is

warp-suite is the kit and release repository: the shared `warpui-kit-*` crates
live under `kits/`, and each app lives in its own repository, pins a kit tag and
repeats the kit pins in its own manifest. Locally `apps/<name>` is a symlink to
the sibling checkout, and the root workspace builds the kits plus whatever apps
are linked. The Android IME has its own repository and Gradle build; its Rust
bridge is not a member here. Repositories and tags are in
[`suite.toml`](suite.toml).

| App | Package | What it is |
|---|---|---|
| warp-station | `warp-station` + `warp-station-core` | Repository workbench (Rust/GPUI) |
| warp-agent | `warp-agent` | Chat over `opencode serve` and `dsh web` (Rust/GPUI) |
| warp-snapit | `warp-snapit` | Windows-only zone snapper (Win32, no GPUI/kit) |
| warp-notes | `warp-notes` | Per-repo markdown notes with a live editor (Rust/GPUI) |
| warp-pomodoro | `warp-pomodoro` | Focus timer with a live Station rail page (Rust/GPUI) |
| warp-android-scribe | `warp-android-scribe-bridge` | Dictation IME over Superwhisper or Hex (Kotlin/Compose + Rust bridge) |

## Local setup

[`docs/agents/local-setup.md`](docs/agents/local-setup.md) carries the clone,
`scripts/dev/link-apps.sh`, `scripts/dev/refresh.sh` and the version hook
recipe. Two facts that trip agents: `link-apps.sh` never replaces a real
`apps/<name>` directory (that would hide a checkout), and app checkouts carry no
`[workspace]` table on purpose — the root `apps/*` member glob and the kit
`[patch]` table are what make the symlinked build work.

## Before you edit

1. Read the app repository's `AGENTS.md` when the work is in one app (the
   `apps/<name>/AGENTS.md` symlink points at it, or open the Android
   repository's own file). It wins over this file for app-specific facts.
2. Read the root [`Cargo.toml`](Cargo.toml), [`Cargo.lock`](Cargo.lock) and the
   touched crate's `Cargo.toml` before adding a dependency.
3. For a gpui or gpui-kit API you are not certain about, read the pinned
   registry source first. Do not guess GPUI from general Rust or web UI
   knowledge.
4. Reuse the shared kit crates instead of writing a second version of a shape,
   and match the existing module layout.
5. The working tree is often dirty. Never revert or reformat unrelated changes.

## Workspace, pins, toolchain

- The root `Cargo.toml` is the kit workspace: `kits/*` as members plus an
  `apps/*` glob that adds whatever app checkouts are linked. App repositories
  keep their own workspace, lockfile and version; do not add their crates as
  explicit members here.
- One [`Cargo.lock`](Cargo.lock) serves the kits and the linked apps. A linked
  app build extends it locally; do not commit that extension. Suite CI runs
  without `--locked` because the linked set is machine-local, while the app
  repositories keep `--locked` on their own lockfiles.
- [`rust-toolchain.toml`](rust-toolchain.toml) pins the channel CI reads. Patch
  releases flow; bump the channel deliberately, in one edit.
- [`.cargo/config.toml`](.cargo/config.toml) sends every build to
  `.build/target`. Keep it that way: one GPUI dependency tree is shared by every
  app and kit, so `dist/`, `.build/` and `target/` are build output, never
  edited by hand.
- Pin `gpui` (`gpui-pre`), `gpui-pre-platform` where an app uses it, and
  `gpui-kit` together, in the root `[workspace.dependencies]`, or the linked
  build will not resolve. The crates.io `gpui` is stale: never add it.
- [`suite.toml`](suite.toml) is the pin table: the newest published kit tag and
  one tag per app. Each app's `Cargo.toml` pins the kit tag it was built
  against and moves that pin when it needs new kit API — a kit addition does
  not force every app to rebuild, so the pin table may name a newer kit than
  some apps carry (the local `[patch]` build always uses `kits/`). App build
  knobs live in each app's `app.toml`; [`scripts/meta/manifest.py`](scripts/meta/manifest.py)
  reads both.

## Commands and gates

Use the package selector from the repo root; the app directory also works
because cargo walks up to the suite workspace. `cargo build -p <app>` needs
`scripts/dev/link-apps.sh` to have linked that app; with no links the workspace
is kit-only, which is what CI builds.

```sh
cargo build -p warp-notes      # one linked app, against the local kits
cargo test -p warp-station -p warp-station-core
cargo test -p warpui-kit-core  # one kit
cargo fmt --all
```

One package selector links every test target that package owns (Warp Station
has 23 probes): iterate with `--test <probe>`, and read
[`docs/agents/build-times.md`](docs/agents/build-times.md) before treating a
slow build as normal.

Kits have no CI workflow; run their gate locally:

- `cargo fmt --all --check`.
- Clippy with `-D warnings` on the six kit crates:

  ```sh
  cargo clippy --all-targets \
    -p warpui-kit-core -p warpui-kit-diff -p warpui-kit-markdown \
    -p warpui-kit-picker -p warpui-kit-platform -p warpui-kit-build \
    -- -D warnings
  ```

- `cargo test` on the six kit crates.

Each app's own `ci.yml` runs that app's fmt, clippy and tests. After a kit
change, `cargo test -p <app>` builds that linked app against the kit, which is
what catches a moved public API; with the checkouts linked,
`cargo test --workspace` covers the kits and the apps together. Match the app's
own CI for the crates you touched before calling a change done: a passing probe
is not the whole gate.

Fix clippy instead of silencing it; where a lint genuinely does not apply, use
`#[expect(clippy::..., reason = "...")]` on that one item. Run `cargo fmt --all`
(without `--check`) to fix formatting rather than reporting it. If `cargo` is
missing from an agent shell, prepend `PATH="$HOME/.cargo/bin:$PATH"` for that
command.

**Windows type-check.** Plain `cargo check --target x86_64-pc-windows-msvc`
dies in cc-rs on C dependencies here. The Rust skill carries the working stub
script; run it in the app repository checkout:

```sh
$HOME/.agents/skills/rust/scripts/check-windows.sh -p warp-station
```

It type-checks the Rust code, `#[cfg(windows)]` blocks included, compiles no C
and links nothing: report it as a type-check, not a Windows build. The real
Windows build is the app repository's `release` workflow.

**Android.** The Gradle project is built in its own repository:

```sh
# in the warp-android-scribe checkout
cd android
JAVA_HOME=/usr/lib/jvm/java-17-openjdk ANDROID_HOME=$HOME/tools/android-sdk \
  ./gradlew assembleDebug
```

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

The apps talk to each other across repositories, and the interfaces below must
move together: land both sides in the same change window, then let the suite pin
table and each app's CI carry the pair. The full text of each is in
[`README.md#cross-app-contracts`](README.md#cross-app-contracts); this file
keeps the index, so the detail is edited in one place.

- **warp-agent door** — `~/.warp/agent/ipc.port` + `ipc.token`, token-auth
  NDJSON: `ping`, `open-workspace`, `new-chat`, `run-prompt`, `cancel`,
  `models`, `status`. warp-agent owns the protocol; Warp Station is the client.
- **SnapIt settings** — `~/.warp/snapit`, versioned snake_case; the old
  .NET-shaped files are importer input only.
- **Notes store** — `~/notes` by default, launched with `--store` and an
  explicit `--root`/`--repo` scope; the hidden `<store>/.warp-notes/` is
  warp-notes' own and every walker skips dot directories.
- **Pomodoro** — `~/.warp/pomodoro` owns `settings.json`, `history.json` and
  `state.json`, publishes the family door and accepts `--start`.
- **Per-user layout** — `~/.warp/<app>/` beside `snapit/`, plus the shared
  `ui.json` and one `logs/`; `warpui_kit_platform::paths` owns the tree and the
  startup migrations.
- **Settings and caches** — versioned snake_case with a top-level
  `schema_version`; [`docs/agents/settings-schema.md`](docs/agents/settings-schema.md)
  is the contract.
- **Shared `ui.json`** — `text_size` and `palette`, written only by
  `warpui-kit-core`; both keys are additive, other apps read at next start.
- **Version facts** — one `version.toml` per repository, PATCH bumped by the
  pre-commit hook and stamped into each binary; the rulebook is
  [`docs/agents/versioning.md`](docs/agents/versioning.md).
- **Android bridge** — LAN `:8787` plus public HTTPS, `Authorization: Bearer`;
  two engines (`hex`, `superwhisper`) picked per request, both must keep
  working.

## GPUI Kit app conventions

- Run `warpui_kit_core::bootstrap::init(cx, legacy_text_size, reduce_motion,
  retheme)` before the first window: it is the only place that calls
  `gpui_kit::init`, `adopt_text_size`, `adopt_palette`, `follow_palette`,
  `follow_ui_scale` and `motion::set_reduced`. Bind app keybindings **after**
  it, so they win ties, and scope them with action contexts (`"Input"`,
  `"Composer > Input"`), not global remaps.
- Motion is a state signal: a running or loading surface shows a spinner and a
  word at its row, in its head/title, and in the window's title bar and OS
  title. Every spinner has a word and an accessible name; reduced motion keeps
  the word. See `docs/agents/design-system.md#motion`.
- Content text renders as markdown through `warpui-kit-markdown` and keeps its
  context menu; labels — buttons, rows, tabs, field and section labels, tool
  names, statuses — stay plain text. The named `TextView` exceptions and the
  interaction each waits on are in `docs/agents/design-system.md#text-surfaces`;
  a new exception names its constraint and ships a probe.
- Keyboard list navigation belongs to the focused pane: each list owns a
  `FocusHandle` and its own `.on_key_down`, never a window-level arrow handler.
  warp-notes' tree (`handle_tree_key`) is the reference. See
  `docs/agents/design-system.md#keyboard-navigation`.
- An app's `theme.rs` re-exports `warpui-kit-core` and adds only layout
  constants; app code holds no colour literal, font family or font size. Read
  the live palette through the reader helpers, never from cached constants, and
  pass `current_ramp().accent_colors()` to `apply_kit_theme` so the accent
  follows the palette. The family has one accent picker.
- Assets: use `warpui_kit_platform::assets::KitBackedAssets` and keep
  `rust-embed`'s `debug-embed` feature, or assets resolve from the process CWD
  in debug builds. Warp Station's generated `include_bytes!` table is the
  large-icon Windows exception, not the default.
- Paths: use `warpui_kit_platform::paths` rather than building `~/.warp` paths
  by hand, and run an app's legacy-layout migration before logging or settings
  read the moved folders.
- Logging: `logging::initialize()` returns a guard kept alive in `main`, plus a
  panic hook at startup so crashes reach the log.
- Stateful kit components own their state in `Entity<State>`; render from the
  entity and mutate through events. `cx.notify()` is required when the UI must
  show a change. A panel over a scrim needs `.occlude()` and a mouse-down
  handler that stops propagation. Render paths hold no `unwrap`.
- A long-lived OS child needs a killer from `cx.on_app_quit`; dropping a
  `std::process::Child` leaves it running. This gpui rev does not service
  cross-thread channel wakeups on foreground tasks: poll on a
  `background_executor().timer(..)` or drain on a timer where that matters.
- Prefer a `lib.rs` boundary so headless probes can build the real views; the
  binary stays a thin launcher (all three GPUI apps follow that split,
  `warp-agent` included).

## Testing and captures

- UI integration tests are headless probes: `tests/<name>_probe.rs` with
  `#[gpui_kit::gpui::test]` (or `#[gpui::test]`), `TestAppContext` /
  `VisualTestContext`, real views through the `lib.rs` boundary.
- `gpui-kit/test-support` goes in `[dev-dependencies]` only.
- Import explicitly in probe files. `use gpui_kit::*;` or `use gpui::*;`
  brings gpui's `test` attribute into scope over std's and breaks
  `#[gpui_kit::gpui::test]` with a recursion-limit error.
- A probe that writes shared state runs under a scratch `HOME`; process-wide
  state takes turns behind a lock. [`docs/agents/testing-and-captures.md`](docs/agents/testing-and-captures.md)
  carries the env hooks and the hidden-window capture recipe.
- Warp Station's `changes_probe` and `git_parity` read the app repository's own
  git history; its CI checks that job out with `fetch-depth: 0`. Do not
  shallow-clone the app repo locally.
- The Android bridge's four integration probes — `api_probe` (auth, limits,
  temp-file cleanup, engine routing, gateway errors), `settings_probe` (profile,
  vocabulary diff, processing download), `superwhisper_cli_probe` and
  `hex_cli_probe` — run in the Android repository's CI.

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
  and its `release` workflow: Linux tarball, Windows portable zip and, where a
  `.iss` exists, the Inno Setup installer. Without one the release is a
  portable zip.
- warp-suite owns the kit build ([`scripts/build/kit-build.sh`](scripts/build/kit-build.sh)),
  the suite assembler and the install scripts: [`scripts/build/build.sh`](scripts/build/build.sh)
  and [`build.ps1`](scripts/build/build.ps1) build the linked workspace, and
  [`scripts/deploy/`](scripts/deploy/) installs the result.
- Keep the desktop entry's `Exec=__WARP_SUITE_BIN__/...` placeholder; the
  build resolves it at install time.
- A suite release is the [`suite.toml`](suite.toml) pin table: `suite-release.yml`
  downloads those app releases, verifies their checksums and publishes one Linux
  tarball and one Windows ZIP. It never rebuilds an app from sources, and the
  app repositories are public, so `GITHUB_TOKEN` is enough.
- `warp-android-scribe` ships from its own repository: a sideloaded debug APK
  and a bridge installed as a systemd user service on the NAS; it is never in
  `suite.toml` or the desktop build.

## Adding an app

Own repository with `version.toml`, `app.toml`, `Cargo.toml` (no `[workspace]`
table), CI, release workflow and the kit git pins → tag in
[`suite.toml`](suite.toml) once that release exists → `scripts/dev/link-apps.sh`
→ contracts in [`README.md#cross-app-contracts`](README.md#cross-app-contracts).
The steps and the `app.toml` knobs are in
[`docs/agents/local-setup.md#adding-an-app`](docs/agents/local-setup.md#adding-an-app);
a crate moves to `kits/` only when more than one app needs it. Done means
`scripts/dev/link-apps.sh --check` reports the new link and
`cargo build -p <app>` succeeds.

## Plans and proposals

Write plans, proposals and migration checklists to `plans/<topic>.md`, one file
per topic in kebab-case; `plans/` is repo-wide, so app work files there too, and
never `PLAN*.md` at the repo root or inside `apps/<name>/`. A plan is a working
file, not shipped documentation: start with a status line and the open
decisions, and do not restate the code. Delete it when the work lands, or fold
what is still useful into `docs/agents/` or the app's `README.md`.

## Public history and secrets

- Never commit a token, PAT, password, private IP, Wi-Fi SSID, home path or
  personal email in file content. Use placeholders in docs and examples.
- A commit's author and committer are public. Use the public GitHub noreply
  address, never a private one.
- Pass credentials through GitHub Actions secrets only; never echo or log them.
- The Android bridge's bearer token is its only auth on the public route: keep
  the token, the UFW LAN rule and HTTPS in place, and add Cloudflare Access/WAF
  before exposing it further.
- Before publishing, force-pushing or adding a remote, run this over the current
  tree and every ref:

  ```sh
  git grep -n -i -E '192\.168\.[0-9]+\.[0-9]+|10\.[0-9]+\.[0-9]+\.[0-9]+|/home/[a-z]+|ghp_|gho_|github_pat_|ghs_' -- .
  ```

## Agent skills

### Issue tracker

Issues, tickets and specs live as local markdown under `.scratch/<feature>/`. See
`docs/agents/issue-tracker.md`.

### Triage labels

The five canonical triage roles keep their own names, recorded as a `Status:`
line in the ticket file. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: `GLOSSARY.md` and `docs/adr/` at the repo root. See
`docs/agents/domain.md`.

## Docs map

Read the entry your change lands in; each line names the branch that fires it.

- **UI, palette, type, scale, motion, text, keyboard** →
  [`docs/agents/design-system.md`](docs/agents/design-system.md), the design
  source of truth.
- **A new GPUI app** → [`docs/agents/gpui-app-recipe.md`](docs/agents/gpui-app-recipe.md).
- **Tests, probes, a capture** → [`docs/agents/testing-and-captures.md`](docs/agents/testing-and-captures.md).
- **Settings, a cache, an import** → [`docs/agents/settings-schema.md`](docs/agents/settings-schema.md).
- **A version bump, a release** → [`docs/agents/versioning.md`](docs/agents/versioning.md).
- **A slow build** → [`docs/agents/build-times.md`](docs/agents/build-times.md).
- **Cloning, links, app checkouts** → [`docs/agents/local-setup.md`](docs/agents/local-setup.md).
- **A shared crate, a kit shape** → [`kits/README.md`](kits/README.md).
- **Build, deploy, cross-app contracts** → [`README.md`](README.md).
- **Issues, triage, domain** → [`docs/agents/issue-tracker.md`](docs/agents/issue-tracker.md),
  [`triage-labels.md`](docs/agents/triage-labels.md),
  [`domain.md`](docs/agents/domain.md) and `GLOSSARY.md`.
- **The Android IME, the bridge API, NAS install** →
  [warp-android-scribe](https://github.com/enginkirmaci/warp-android-scribe).
