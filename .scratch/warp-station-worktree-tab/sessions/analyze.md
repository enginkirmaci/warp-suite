---
session: session-7969955c-cdd5-46ab-a86d-3ca562eaabff
harness: dsh
step: analyze
attempt: 1
status: done
updated: 2026-11-22T11:40:25Z
---
# Session: analyze

Wrote `spec.md` (125 lines) for the issue folder.

**Report:** Replaced `.scratch/warp-station-worktree-tab/spec.md` — the old file was a one-line request with no criteria, and all three prior analyze attempts were harness timeouts, so I built the spec from the code. The decision it records: gate `WorkspaceTab::Worktrees` in `Store::detail_tabs()` (via a new `worktrees_tab_shown()` predicate keyed on the selected folder's settled `WorktreeLoad`: rows non-empty or an error), and move the fetch earlier — a `kick_worktrees()` factored out of `load_worktrees`, called from `load_repo_workspace` (covering `select_repo` and the startup `WARP_STATION_SELECT` hook), `refresh_panel`, and unconditionally in `refresh_active_tab` — because today rows load only when the tab is opened, so hiding it without that would make it unreachable. Verified against the actual code that `core::worktree::rows` filters out the main checkout (`!entry.main`), that the strip draws straight from `detail_tabs()` in `ui/detail.rs:161`, and that `active_detail_tab()` already supplies the Overview fallback for a restored Worktrees resting tab; two alternative designs (scan-time count on `RepoEntry`, persisted last answer) are named and rejected with reasons. Acceptance criteria are probe-shaped (tab absent on a settled empty listing, tab plus `worktrees-page` with rows, `NetCommand::Worktrees` sent on select/Refresh, reload keeps a drawn tab, error keeps it, no tab-less page, plus the app's fmt/clippy/test gates), and two genuine product calls (idle discovery latency, removing the last checkout jumping to Overview) are under Open questions with stated defaults so the run isn't blocked. No source file, gate, `workflow.toml` or `run.md` was touched — the app checkout is still clean.
