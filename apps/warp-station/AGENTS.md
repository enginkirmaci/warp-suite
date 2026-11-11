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
  `--parent <pid>`. An empty setting resolves to the packaged
  `warp-snapit.exe` next to the app; the retired .NET `Tools.SnapIt.exe` is
  never started.
- **Shared UI.** Text size and palette come from `~/.warp/ui.json` through
  `warpui-kit-core`; the app stores neither, and the accent follows the live
  palette like the other apps. Do not copy a fixed accent into a new app.
- **Layout.** Everything lives under `~/.warp/station` (versioned
  `settings.json` plus the disposable `cache/repos.json` and
  `cache/models.json`), except the shared `~/.warp/ui.json`, `~/.warp/logs`,
  the SnapIt settings at `~/.warp/snapit` and the notes content at `~/notes`.
- **Settings.** Repo tags live in `settings.json` (`repos.repo_tags`), not in
  the cache. The contract is `../../docs/agents/settings-schema.md`.
- **Folder groups.** `repos.group_repos_by_root` groups each repo under the
  folder `repos.group_root_depth` levels above it (1 = the folder containing
  it); scan roots only decide membership, never the header. A repo outside
  every root lands in "Other".
- **Automatic refresh.** `repos.auto_refresh_on_focus` (default on) rescans
  the list when the app regains focus; `repos.auto_refresh_minutes` (null/0 =
  off) rescans every N minutes. Both run only while a Warp Station window is
  focused, only a local scan (never `git fetch` or the GitHub/Azure eager
  set), and the once-a-second beat drives them through
  `Store::record_focus` / `Store::check_auto_refresh`.
- **Pipelines follow.** While the Pipelines tab is on screen and its run can
  still change, `Store::check_run_follow` (the once-a-second beat) re-reads the
  open run's detail — every 13 s on GitHub, 20 s on Azure — and asks the run
  list every 90 s. A finished run costs one last log call (GitHub only when it
  failed) and one list call, then the loop stops; a failed poll doubles the
  wait up to two minutes and keeps the detail already on screen.
  The request counts are measured, not one per poll: `gh run view --json` is 3
  core REST calls and `gh run list` 2; Azure's build record and timeline are
  one call each. Watching one run therefore costs about 150 GitHub or 67 Azure
  requests per ten minutes — roughly 18 % of GitHub's 5 000 an hour and 17 % of
  Azure DevOps' 200 TSTUs per five minutes. The cadence is the baseline: both
  platforms report `X-RateLimit-*`, and an answer that says the window is down
  to `RATE_RESERVE` parks the next poll until it refills (a 429's
  `Retry-After` included; Azure's are read straight off the response, GitHub's
  through the free `gh api rate_limit` probe when a call fails). Do not tighten
  the cadence or poll while the tab is hidden without redoing that budget;
  `follow_probe` drives the loop headlessly.
- **Keyboard and titles.** List keys belong to the focused pane (repo list,
  Changes files, History commits, PR/Issues); the window delegates only while
  it owns the focus itself. The OS title carries `Store::activity()` next to
  the titlebar chip. That chip also opens the running panel: `Store::running_jobs`
  lists every live job (scan, per-repo git work, generation, summary, clones)
  in the order it started, and the panel closes when the last one lands. The
  Refresh button is a local scan plus the eager activity set; it never runs a
  `git fetch`. The family rules are
  `../../docs/agents/design-system.md` (`## Keyboard navigation`, `## Motion`).

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
