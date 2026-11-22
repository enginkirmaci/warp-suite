---
session: session-1dcd8a86-36b7-4e09-9753-c78cf9260cf8
harness: dsh
step: split
attempt: 1
status: done
updated: 2026-11-22T12:18:44Z
---
# Session: split

Step `split` is written. Three task files, no source, gates, `flow.toml`/`workflow.toml`, `run.md` or `spec.md` touched; the warp-station checkout is still clean.

## ⚠️ Spec state

`spec.md` today is the original 3-line request — it was overwritten at 15:04, after analyze. The 128-line spec that `sessions/analyze.md` reports writing exists only in the analyze session transcript; I recovered it (`/tmp/recovered-spec.md`) and split against **that** contract, not the stub. Restore it before approving: the `code` and `review` steps read `spec.md`, and the task files cannot carry the whole contract on their own. The analyze report's decisions match the recovered spec exactly (gate in `detail_tabs()`, eager `kick_worktrees`, no view change, probe-shaped criteria, the two open questions' defaults).

## Plan

- **01: Ask for a repository's worktrees as soon as it is selected** (blocked by None) — `kick_worktrees(folder)` factored out of `load_worktrees`, called from `load_repo_workspace` (`&mut`, covering `select_repo` and the `WARP_STATION_SELECT` startup hook), `refresh_panel`, and unconditionally at the top of `refresh_active_tab`; error kept until the settled answer. Invisible to a person, so it can merge alone.
- **02: Hide the Worktrees tab until a listing has a row or an error** (blocked by 01) — `worktrees_tab_shown()`, the `Worktrees` arm in `detail_tabs()`, no `active_detail_tab()`/view change; probes for empty/rows/loading/error/restore and the still-hidden fallback to Overview.
- **03: Document when the Worktrees tab appears** (blocked by 02) — the condition sentence in the app `AGENTS.md` `Worktrees` bullet and the `README.md` `Worktrees` tab paragraph, with grep-able done-whens.

All spec acceptance criteria are covered by named probes or unit tests in the tasks; 02 is blocked by 01 because gating without the eager ask makes the tab unreachable for every repository, and 03 is blocked by 02 because it documents that gate.

Files: `.scratch/warp-station-worktree-tab/issues/01-ask-for-worktrees-on-selection.md`, `02-hide-empty-worktrees-tab.md`, `03-document-worktrees-tab-condition.md`.
