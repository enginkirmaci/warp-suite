---
session: session-1250d2b8-8666-476a-9371-93a07c75cec2
harness: dsh
step: code
task: 02
attempt: 1
status: done
updated: 2026-11-22T12:43:35Z
---
# Session: code

## Task 02 report — hide the Worktrees tab until a listing has a row or an error

Implemented and committed. `main` in the app checkout is untouched (`afcad4e`), no task/spec/gate files were edited, and both worktrees are clean.

### Commit

`b9140ad` on `agent/02-hide-the-worktrees-tab-until-a-listing-has-ar` in the **warp-station** repository, based on task 01's tip (`e01d4eb`) so the blocked-by dependency lands with this task. The commit carries the repo's pre-commit PATCH bump (`0.1.98 → 0.1.99`).

```
feat(store): hide the Worktrees tab until it has rows
```

### Files changed

- [src/store/mod.rs](apps/warp-station/src/store/mod.rs#L6086): new `pub fn worktrees_tab_shown(&self) -> bool` (true only when the selected folder's `WorktreeLoad` has a row or an error), the `WorkspaceTab::Worktrees => self.worktrees_tab_shown()` arm in `detail_tabs()` ([line 6107](apps/warp-station/src/store/mod.rs#L6107)), and an updated doc comment saying why Worktrees waits for a listing while Pipelines keeps its empty-state tab. `loading` is deliberately not part of the predicate.
- [tests/worktrees_probe.rs](apps/warp-station/tests/worktrees_probe.rs): four new window probes plus the two-row version of the existing rows test.
- Version files from the pre-commit hook: `Cargo.toml`, `Cargo.lock`, `crates/warp-station-core/Cargo.toml`, `version.toml`.

### Tests added

- `worktrees_tab_follows_the_settled_listing` — store unit test over no selection, no entry, empty loading, rows, error, empty settled; also pins `detail_tabs()` = Overview, Changes, History, Pipelines with both platforms off ([line 7842](apps/warp-station/src/store/mod.rs#L7842)).
- `an_empty_worktree_listing_hides_the_tab` — empty settled answer: no tab, no page, Overview draws.
- `a_reload_keeps_a_drawn_worktrees_tab` — `load_worktrees()` in flight keeps the predicate true, `detail_tabs()` and the drawn strip.
- `a_listing_error_keeps_the_tab_until_an_empty_answer` — error draws the tab and `worktrees-error`; the following `Ok(vec![])` removes both and returns Overview.
- `a_restored_worktrees_tab_draws_overview` — `open_detail_tab(Worktrees)` on a settled empty listing draws Overview, never a tab-less Worktrees page.
- `the_worktrees_tab_draws_its_rows` now loads two rows with the existing page/row containment check.

### Verification

- `cargo test -p warp-station --test worktrees_probe` — **14 passed**.
- `cargo test -p warp-station -p warp-station-core` — exit 0 (all targets), including `store::tests::worktrees_tab_follows_the_settled_listing`.
- `cargo fmt --all --check` — clean; `cargo clippy --all-targets -p warp-station -p warp-station-core -- -D warnings` — clean.
- Non-vacuity check: forcing `Worktrees => true` makes the three hide/restore probes fail; adding `!loading` to the predicate makes the reload probe fail. Both were reverted before committing.

### Look first

The predicate and its `detail_tabs` arm ([mod.rs:6086](apps/warp-station/src/store/mod.rs#L6086)), then `a_reload_keeps_a_drawn_worktrees_tab`: the exclusion of `loading` is the subtle contract that keeps a tab from disappearing under the cursor.

### Run-wiring caveat (unchanged from task 01)

The suite worktree carries no `apps/` checkouts, so I created the app worktree at `/mnt/disk2/Developments/warp-station/.worktrees/agent-02-hide-the-worktrees-tab-until-a-listing-has-ar` and linked it as `apps/warp-station` for cargo. The suite task branch has **no commit**; the review, gate and merge steps must point at the warp-station branch above — otherwise the merge step's empty-suite-branch path will resolve task 02 without merging it, just as it did for task 01 (whose `e01d4eb` is still not on `main`).
