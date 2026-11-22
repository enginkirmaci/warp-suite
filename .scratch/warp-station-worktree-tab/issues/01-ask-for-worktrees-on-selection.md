# 01: Ask for a repository's worktrees as soon as it is selected

**Status:** ready-for-agent
**Category:** bug
**Blocked by:** None

## What to build

Move the worktree listing from "fetched when the tab is opened" to "fetched
whenever a repository's workspace is loaded or refreshed", without changing what
the strip draws. This task alone is invisible to a person; it exists so the gate
in 02 cannot make the tab unreachable.

- Split the "mark loading, send `NetCommand::Worktrees`" half of
  `Store::load_worktrees` into `fn kick_worktrees(&mut self, folder: &str)`.
  It inserts or reuses the `WorktreeLoad` for `folder`, returns early when that
  load is already `loading`, sets `loading = true`, and sends
  `NetCommand::Worktrees { folder }`. It takes the folder as an argument
  instead of reading `selected_repo`, because the startup hook below loads a
  workspace before the selection is set. It does **not** take `cx` and does
  **not** notify: every caller already notifies for its own reason.
- `load_worktrees(cx)` becomes: read `selected_repo`, `kick_worktrees` for it,
  `cx.notify()`. Keep it public and keep its existing callers (the removal and
  prune handlers, `open_detail_tab`, the tab's re-read button) working through
  it.
- `kick_worktrees` leaves `error` alone. Today `load_worktrees` clears it before
  asking; the settled `UiEvent::WorktreesLoaded` owns the result, and keeping the
  old error until the new answer stops a retry from blinking a tab away that an
  error keeps visible (02 relies on this).
- Make `fn load_repo_workspace` take `&mut self` and kick for the folder it was
  given, after it sends `LoadChanges`. That covers both of its call sites:
  `select_repo` and the startup `WARP_STATION_SELECT` hook in the constructor,
  which calls it before setting `selected_repo`.
- `refresh_panel` kicks for the selected folder, so the pane's Refresh re-checks
  a listing whose tab is hidden. Keep the worktree read out of the
  `pending`/`panel_refresh` count (the worktree read is not counted today, and
  `UiEvent::WorktreesLoaded` has no `settle_panel_refresh` path); the refresh
  button's busy state must not wait on it.
- `refresh_active_tab` kicks for the selected folder before its `match`, and its
  `WorkspaceTab::Worktrees` arm folds into `Overview | Changes`. Every
  auto-refresh (`start_auto_refresh`) and focus regain then refreshes the
  listing whatever tab is on screen, not only when Worktrees is active.
- Leave `detail_tabs()`, `active_detail_tab()` and `src/ui/*` untouched in this
  task: the strip still carries Worktrees for every repository.

## Where it lives

- `apps/warp-station/src/store/mod.rs`: `load_worktrees` (~2762),
  `refresh_active_tab` (~3208), `select_repo` (~3367), `load_repo_workspace`
  (~3385), `open_detail_tab` (~3415), `refresh_panel` (~6885), the constructor's
  `WARP_STATION_SELECT` hook (~767), `WorktreeLoad` (~7667).
- `apps/warp-station/tests/worktrees_probe.rs`: new send assertions next to the
  existing no-window listing tests; it already owns `entry()`, `scratch_home()`
  and the `Store::new(cx, net_tx)` pattern.
- `apps/warp-station/tests/repos_probe.rs`: `auto_refresh_reloads_the_active_tab`
  (~1341) already drains the net lane per regain and per periodic tick.

## Done when

- A new `tests/worktrees_probe.rs` test —
  `selecting_a_repo_asks_for_its_worktrees` — builds a store on an unbounded
  net channel, calls `replace_repos` + `select_repo` and asserts a
  `NetCommand::Worktrees { folder }` for the new folder is on the lane although
  no tab was opened; a `load_worktrees(cx)` call while the first ask is still
  in flight does not queue a duplicate.
- A new test — `refresh_asks_for_worktrees_with_another_tab_showing` — sets the
  resting tab to Changes, calls `refresh_panel(cx)` and asserts a
  `NetCommand::Worktrees` for the selected folder is on the lane. It then
  settles the changes answer with `UiEvent::ChangesLoaded` and asserts
  `panel_refreshing()` is false although no `WorktreesLoaded` arrived: the
  worktree read never holds the button's busy state open.
- `auto_refresh_reloads_the_active_tab` in `tests/repos_probe.rs` also asserts a
  `NetCommand::Worktrees` for `/probe/alpha` on a focus regain while Changes is
  active and on the periodic tick, so the unconditional kick is pinned.
- `cargo test -p warp-station --test worktrees_probe --test repos_probe` passes.
- `cargo fmt --all --check`, `cargo clippy --all-targets -p warp-station -p
  warp-station-core -- -D warnings` and
  `cargo test -p warp-station -p warp-station-core` pass.
