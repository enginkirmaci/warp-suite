# 02: Hide the Worktrees tab until a listing has a row or an error

**Status:** ready-for-agent
**Category:** bug
**Blocked by:** 01

## What to build

Gate the strip on the selected repository's settled worktree listing, now that
01 asks for it on selection, Refresh and every auto-refresh pass.

- Add `pub fn worktrees_tab_shown(&self) -> bool` to `Store`: true when the
  selected folder has a `WorktreeLoad` with `!rows.is_empty() || error.is_some()`.
  No entry, or an entry that is still `loading` with empty rows and no error, is
  false. `loading` is deliberately not part of the predicate: a reload must not
  remove a tab under the cursor, and only a settled empty answer hides it.
- `detail_tabs()` gains `WorkspaceTab::Worktrees => self.worktrees_tab_shown()`
  and keeps `PullRequests | Issues => platforms` and the true arms for Overview,
  Changes, History and Pipelines. Update the doc comment so it says why:
  Worktrees has nothing to draw until a linked checkout exists, while Pipelines
  keeps its tab for its empty state.
- Change nothing else: `active_detail_tab()` already falls back to Overview when
  `detail_tab` is not in `detail_tabs()`, which is also the fallback for a
  restored Worktrees resting tab with nothing to list. `src/ui/detail.rs` keeps
  iterating `detail_tabs()` and `src/ui/worktrees.rs` keeps its rows, error card
  and "No worktrees yet" card as the fallback render of an empty `WorktreeLoad`.
  `open_detail_tab(Worktrees)` may still set `detail_tab`; the pane draws
  Overview until the listing settles with a row.
- No new selector and no row markup change.

## Where it lives

- `apps/warp-station/src/store/mod.rs`: `detail_tabs` (~6075),
  `active_detail_tab` (~6091), `WorktreeLoad` (~7667), `WorkspaceTab::ALL`
  (~7674), the `#[cfg(test)] mod tests` (~7756).
- `apps/warp-station/tests/worktrees_probe.rs`: the window probes
  (`the_worktrees_tab_draws_its_rows` ~280,
  `a_refused_removal_draws_its_force_confirm` ~461) and their `entry()` helper.
- `apps/warp-station/src/ui/detail.rs:161` and `src/ui/worktrees.rs` are read-only
  for this task; the strip and the page already follow the store.

## Done when

- `an_empty_worktree_listing_hides_the_tab`: select `/probe/repo`, deliver
  `UiEvent::WorktreesLoaded { result: Ok(vec![]) }` without opening the tab,
  draw the window; assert `debug_bounds("detail-tab-worktrees").is_none()`,
  `detail_tabs()` lacks `WorkspaceTab::Worktrees`, `active_detail_tab()` is
  Overview, `detail-tab-overview-panel` draws and `worktrees-page` does not.
- `the_worktrees_tab_draws_its_rows` still passes: a settled one-row listing
  draws `detail-tab-worktrees`, and opening it draws `worktrees-page` with the
  `worktree-row` inside it. Load two rows in that test and keep the existing
  page/row containment check: the row loop renders every entry of
  `worktree_load(folder).rows`, and this task changes no row markup, so the
  drawing path is unchanged — the assertion that the tab is present with a
  two-row listing is the new part.
- `a_reload_keeps_a_drawn_worktrees_tab`: after a one-row answer, call
  `store.load_worktrees(cx)` (sets `loading` and sends) and assert
  `worktrees_tab_shown()` is true, `detail_tabs()` still contains Worktrees, and
  the strip still draws the tab, before the next answer.
- `a_listing_error_keeps_the_tab_until_an_empty_answer`: deliver `Err(..)` and
  assert the tab draws and `worktrees-error` is present; then deliver
  `Ok(vec![])` and assert the tab is gone and `detail-tab-overview-panel` draws.
- `a_restored_worktrees_tab_draws_overview`: `open_detail_tab(Worktrees)` on a
  settled empty listing; assert `active_detail_tab()` is Overview,
  `detail-tab-overview-panel` draws, `worktrees-page` does not, and the strip
  carries no `detail-tab-worktrees` — never a tab-less page.
- A unit test `worktrees_tab_follows_the_settled_listing` in `src/store/mod.rs`
  covers `worktrees_tab_shown` for: no selected repo, no entry, empty loading,
  rows present, error present; and asserts `detail_tabs()` still lists Overview,
  Changes and History with GitHub and Azure off. The existing
  `pipelines_is_always_a_detail_tab` and
  `an_azure_only_setup_keeps_the_pull_request_tabs` tests pass unchanged.
- `cargo test -p warp-station --test worktrees_probe` and
  `cargo test -p warp-station -p warp-station-core` pass, and
  `cargo fmt --all --check` and
  `cargo clippy --all-targets -p warp-station -p warp-station-core -- -D warnings`
  pass.
