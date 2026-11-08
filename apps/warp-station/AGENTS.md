# warp-station — agent instructions

Read [`../../AGENTS.md`](../../AGENTS.md) first. This file adds only the
app-specific facts. The user-facing description is in
[`README.md`](README.md); the deep UI guides are in
[`../../docs/agents/`](../../docs/agents/).

## Shape

Warp Station is the repository workbench: a GPUI app over a headless service
crate. It is two packages:

| Package | Path | Role |
|---|---|---|
| `warp-station` | `crates/warp-station/` | lib + bin: store, net thread, UI |
| `warp-station-core` | `crates/warp-station-core/` | headless services, no gpui |

`crates/warp-station-core` must not depend on gpui or gpui-kit. It takes
`warpui-kit-platform` with `default-features = false` so its tests stay
display-free.

## Where things live

- `crates/warp-station/src/main.rs` — the startup order.
- `crates/warp-station/src/store/` — the UI state: repos, selection, tabs,
  notices and the event pump (`mod.rs`, `projection.rs`).
- `crates/warp-station/src/net.rs` — the net thread: its own tokio runtime and
  two async channels, commands in and `UiEvent` out.
- `crates/warp-station/src/ui/` — the window, rail, list, detail tabs,
  settings, dialogs, tools and the notes/SnapIt surfaces.
- `crates/warp-station/src/ipc.rs` — the single-instance door (`focus` only).
- `crates/warp-station/src/theme.rs`, `assets.rs`, `logging.rs`, `notices.rs`.
- `crates/warp-station-core/src/` — `git/`, `settings.rs`, `paths.rs`,
  `repo_store.rs`, `scan.rs`, `refresh.rs`, `snapit.rs`, `warp.rs`,
  `warp_notes.rs`, `opencode.rs`, `autostart.rs`, `hotkey.rs`, `keepawake.rs`,
  `desktop_notifications.rs`, `jsonio.rs`, `exe.rs`, `process.rs`, `terminal.rs`.
- `crates/warp-station/tests/` — headless probes; `examples/diff_demo.rs` is a
  demo, not a test.

## Startup order

Keep this order; several steps depend on the previous one:

1. On Linux, a `--copy-clipboard-password` launch pings the running instance's
   net thread and exits **before** any logging or GPUI startup.
2. `ipc::try_forward()` — a live instance gets the launch and this process
   exits before migrations or logging touch the same files.
3. `warp_station_core::paths::migrate_legacy_data()`,
   `migrate_legacy_ai_data()` and `migrate_suite_layout()` — before logging,
   so the fresh `~/.warp/logs` never collides with an absorbed one.
4. `logging::initialize()`; keep the guard in `main`.
5. `application().run(...)`: net thread and `Store`, then
   `warpui_kit_core::bootstrap::init`, then the Windows auto-start blocks,
   the event pump, the once-per-second beat and the window.

## App-specific contracts

- **warp-agent client.** The AI surface (commit-message/README wands, daily
  summary, harness/model/variant catalog) runs through warp-agent's loopback
  door. Warp Station does not spawn `opencode serve` itself. A missing
  warp-agent binary disables the AI features; a closed one is launched on
  demand. Wand runs are real warp-agent chats in the repo workspace; the daily
  summary runs in the General workspace.
- **Notes.** The notes surfaces launch warp-notes with `--store` (the resolved
  `notes_store_path`) and an explicit scope: the titlebar, the summary and the
  global-search note hits ask for `--root` (the whole store, never the
  selected repo), while a repo's launcher pins `--repo <name>`; a hit also
  passes `--note`. A missing binary shows a toast and nothing else breaks.
- **SnapIt.** Reads the same `~/.warp/snapit` files as warp-snapit. On Windows
  the auto-start setting launches the configured executable with
  `--parent <pid>`. The default executable name is historical; the Rust port is
  `warp-snapit.exe`.
- **Shared UI.** Text size and palette come from `~/.warp/ui.json` through
  `warpui-kit-core`; the app stores neither. This app still pins the Copper
  accent (see `../../docs/agents/design-system.md`); do not copy that signature
  into a new app.
- **Layout.** Everything lives under `~/.warp/station`, except the shared
  `~/.warp/ui.json`, `~/.warp/logs`, the SnapIt settings at `~/.warp/snapit`
  and the notes content at `~/notes`.

## Tests

```sh
cargo test -p warp-station -p warp-station-core
cargo build -p warp-station
```

- Unit tests live beside the code; integration probes are under
  `crates/warp-station/tests/`. `changes_probe`, `repos_probe`,
  `settings_probe`, `split_probe`, `theme_probe`, `tools_probe` and the rest
  drive real views in headless windows.
- `changes_probe` and `git_parity` read this repository's own git history.
  Run them from a full clone, not a shallow one.
- Probes that write settings use a scratch `HOME`/`USERPROFILE` and a lock.
  `WARP_STATION_IPC_DIR` redirects the single-instance endpoint for probes.
- The env hooks (`WARP_STATION_PAGE`, `_SHEET`, `_SELECT`, `_TOOLS_OPEN`,
  `_GROUPS`, `_NOTES`, `_IPC_DIR`) are read once at window construction; see
  [`../../docs/agents/testing-and-captures.md`](../../docs/agents/testing-and-captures.md).

## Platform notes

- Linux: the `--copy-clipboard-password` hotkey payload; `notify-send` desktop
  notifications; FreeDesktop autostart.
- Windows: SnapIt auto-start, keep-awake, WinRT toasts, the installer and the
  generated asset table. The packaged exe hides the console with the
  `windows_subsystem` attribute; debug builds keep it.

## Do not

- Add `gpui`/`gpui-kit` to `warp-station-core`.
- Move `ipc::try_forward` after logging or migrations; a second launch would
  then write the same files.
- Change the door protocol without updating warp-agent in the same commit.
- Hand-roll the theme bootstrap or add an accent picker. Read the live palette
  and pass `current_ramp().accent_colors()`.
- Add a per-app build script or installer; the suite's `scripts/` owns those.
