# Plan: a local merge, and where its state shows

Status: built locally 2026-11-14 — the core merge CLI and its outcomes, the
repository-backed merge state, the Changes band, the branch list's right-click
menu (the picker's `set_on_secondary` hook), the flow-strip mirror and the
conflict editor's entry are in; the app and kit gates are green. Open: the
conflicted rows are still the `U` rows in Unstaged (no pinned `Conflicts`
section yet), the editor opens from the pane's button or `Enter` (not a
double-click), and the app repository needs a kit tag/pin bump before it builds
standalone. The conflict screen is `plans/merge-conflict-editor.md`.

## 1. What exists today

- Merging is the platform's. `Store::press_merge` arms the button and
  `github::merge_pull_request` runs `gh pr merge --merge`; Azure completes
  through its own call. Both are irreversible, both are one confirmed press,
  and both live on the pull request.
- The flow strip's Merge chip points at that pull request. Its detail says
  "`#n` open · ready to merge" for every open pull request — `Store::flow_view`
  reads `FlowPullState::Open` alone. `GitHubPull::merge_state`
  (`mergeStateStatus`) is parsed and never read, so BLOCKED, BEHIND or UNSTABLE
  reads as ready.
- There is no local merge. `GitAction` has no Merge, nothing reads
  `MERGE_HEAD`, and no surface can say a merge is in progress. Unmerged paths
  reach the file list as `U` only because git's status reports them
  (`read::describe_status_entry`).
- Pull runs `git pull`, so merging the tracked upstream already exists. The
  gap is merging a *different* branch — normally the base — into the current
  one, and living with the conflict state that follows.

## 2. The states, and where each one shows

One vocabulary, read from the repository and carried by
`GitChangeSnapshot` rather than kept in a session flag, so a restart lands on
the same screen:

| State | Meaning | Changes tab | Flow strip | Toast |
|---|---|---|---|---|
| Idle | no merge | nothing | unchanged | – |
| Available | a branch row can be merged into the current one | branch list, right-click → `Merge into <current>` | unchanged | – |
| Running | `git merge` in flight | band reads `Merging main into feature…` with a spinner | unchanged | – |
| Conflicts | `MERGE_HEAD`, unmerged paths | band + Conflicts section; the pane's **Open merge editor** (or `Enter`) opens the editor | `Resolve 2 conflicts` → Changes | info on stop |
| Ready | `MERGE_HEAD`, no unmerged paths | band, Continue enabled | `Merge ready` → Changes | – |
| Merged | the merge commit landed | band gone, list refreshed | back to the PR vocabulary | success |
| Failed | git refused (dirty tree, bad source) | nothing changes | unchanged | error |

Rules:

- **Changes is the home.** It is where the git verbs live, where the files to
  resolve are listed, and where the merge commit lands.
- **The strip is a mirror, never a runner.** It states the merge for the
  current branch, and its click opens Changes. The strip only exists for a
  branch with a push/PR/run flow, so a local-only branch keeps the band as its
  only surface. While a merge is in progress the strip must not say "ready to
  merge", which is what it says today.
- **The repo row and the title bar say nothing.** A merge belongs to the open
  tab; the strip already carries the cross-tab fact.

## 3. The verb: a right-click menu on the branch list

- **Where.** The branch dropdown (`ChangesView`'s `branch_picker`) is where a
  branch is chosen. A right-click on a row opens a context menu at the
  pointer; its first item is `Merge into <current>`. The row's left click
  stays checkout, so the toolbar does not grow a sixth verb and the two paths
  do not compete.
- **The menu.** `Merge into <current>` first, then `Checkout`, `Copy branch
  name`, and `Delete branch` last in danger ink with the existing two-click
  arm inside the menu (first press arms the row, second deletes). The branch
  rows lose their trailing trash: the menu is the one delete path now and owns
  the armed state. On the current branch's row Merge and Delete are disabled;
  a remote-tracking row (`origin/main`) merges like any other and has no
  Delete.
- **The layer.** The picker closes on the right-click and the menu opens at
  the pointer in the app's own anchored layer (the pull-request head menu's
  catcher plus `anchored` card). It is not a GPUI `context_menu` child of the
  row: that child sits inside the scrolling list and would be clipped by the
  picker's panel.
- **The kit needs one hook.** The picker builds its own rows, so
  `warpui-kit-picker` gains `set_on_secondary`: a right-click on a row calls
  back with the row's value and the pointer position, and the app opens its
  menu. No menu drawing moves into the kit.
- **Picking Merge runs `git merge <source>` at once.** No arm and no confirm:
  the merge is local and unpushed, and a stopped merge's band carries Abort.
  The band owns the state from there.
- **Target is always the current branch.** Merging into main means checking
  main out first (the same menu's Checkout), and a second direction is not in
  this cut.
- **Sources**: local and remote-tracking branches, `origin/main` included; no
  implicit fetch, Fetch owns that. The base (the pull request's
  `base_ref_name`, else the default branch) needs no special row: the list is
  already ordered local-then-remote and the user picks by name.

## 4. The band, the conflicts, and the two collisions

The band (52 high, `surface()`, a hairline bottom) is the first row of the
Changes body, above the toolbar and columns — and above the merge editor when
that is open, so the merge's state never leaves the screen:

- left: a warning icon, `Merging main into feature`, and `2 files need
  attention`, or `Ready to commit` when nothing is unmerged;
- right: `Continue` (primary, disabled while any path is unmerged) and `Abort`
  (ghost).

It also covers the flight: the merge has no button to spin, so the band opens
on the press from the store's job label (`Merging main into feature…`, a
spinner, no buttons) and settles into conflicts, ready or gone when the
refreshed snapshot lands. A fast-forward flashes it for a frame; that is the
cost of not having a verb to carry the progress.

Continue runs `git merge --continue` with `GIT_EDITOR=true`, so `MERGE_MSG`
stands and hooks still run. Abort runs `git merge --abort`.

**Collision 1, the clean state.** A resolved merge can leave nothing unstaged,
and `clean_state()` would say "Working tree clean" while `MERGE_HEAD` is still
there. The band outranks the clean state; the tree is only clean once the merge
commit is no longer pending.

**Collision 2, the commit card.** While `MERGE_HEAD` exists the commit card
steps aside: the band's Continue is the way to finish, so there is no
empty-message commit.

Conflicted paths pin above Staged in a `Conflicts` section: the `U` badge, the
same 34-high rows, the file list's own keyboard handling. A single click shows
the file's summary (its conflict count and `Open merge editor`); a
double-click, or Enter, opens the Local/Incoming/Result editor described in
`plans/merge-conflict-editor.md`. That editor's Accept Merge writes the file
and stages it, which is what clears the unmerged entry.

## 5. Plumbing

- `GitAction::Merge { source }`, `MergeContinue`, `MergeAbort`; words in
  `git_working_word`; arms in `net::run_git_action`.
- Four outcomes, not two: `Clean | Stopped | UpToDate | Failed`. A stopped
  merge exits 1 with `CONFLICT` lines — a state, not a failure — and an
  already-merged source exits 0 without moving HEAD. The toasts read "Merged
  main into feature", "Merge stopped: 2 conflicts", "Already up to date", or
  git's own line.
- `GitMergeState { source, target, conflicted, phase }` on
  `GitChangeSnapshot`, read from `Repository::state()` plus the index's
  conflicts, produced by `read::change_snapshot`. That read already runs only
  for the selected repo and is reloaded after every mutation
  (`workspace_loaded`), so there is no new net command and no session flag.
- The kit picker's `set_on_secondary` hook feeds a `ChangesView` menu state
  (`branch`, pointer position, `delete_armed`) drawn as an anchored card over
  the tab: Merge calls `run_git`, Checkout the existing action, Copy the
  clipboard, Delete arms then dispatches. Opening the menu dismisses the
  picker, so only one overlay is live. The branch rows stop passing
  `trailing_delete`; the kit's trailing support stays for the other pickers
  and its probe.
- Store: `run_git` stays the one dispatcher; `run_flow_action` gains one
  action that opens Changes. There is no merge state machine beyond `run_git`
  and the snapshot.
- Settings: none. A merge is not a preference.

## 6. Not in this plan

- No Ours/Theirs vocabulary anywhere; the conflict editor and its Local /
  Incoming sides are `plans/merge-conflict-editor.md`. Staging happens only on
  the editor's Accept Merge press.
- No auto-push, no auto-PR, no rebase/squash choice; Pull keeps its semantics.
- No second merge direction (feature → main) and no merge from the PR screen:
  the PR keeps `gh pr merge` and Azure keeps its completion. A local merge
  never substitutes for a PR merge, which would skip the policies the PR
  surface exists to report.
- No toolbar merge verb and no second picker: the branch list's right-click
  menu is the one entry. The branch rows lose their trailing trash; deleting
  lives in the menu.
- No keyboard path to the context menu (`Shift+F10`, the menu key) in this
  cut; the list's `Enter` still picks, and the right-click is mouse-only.
- The PR chip's own vocabulary stays as it is; only the in-progress mirror is
  added. Teaching it `merge_state` (BEHIND, BLOCKED, DIRTY) is a separate
  change.

## 7. Decisions

- **D1 — Entry point. Settled 2026-11-14:** a right-click context menu on the
  branch dropdown's rows — `Merge into <current>`, `Checkout`, `Copy branch
  name`, `Delete branch` — with the row's trailing trash removed. Revised
  twice: the base-only toolbar verb, then the toolbar picker, both dropped
  after the owner asked for the branch list's context menu.
- **D2 — Conflicted pane. Settled 2026-11-14:** a custom Local/Incoming/Result
  merge editor with Visual Studio's feature set, not a resolve card and not a
  plain diff; the design is `plans/merge-conflict-editor.md`.
- **D3 — Fast-forward. Settled 2026-11-14:** let git fast-forward when it can;
  no forced `--no-ff`.
- **D4 — Merge message. Settled 2026-11-14:** git's `MERGE_MSG` default; no
  templated message.

## 8. Acceptance

- Scratch repo: right-click `main` in the branch list → the menu opens at the
  pointer, `Merge into feature` lands the merge with a toast naming both
  branches; Esc and a click outside close the menu without merging.
- The current branch's row shows Merge disabled; `origin/main` merges like a
  local branch.
- Delete from the menu: first press arms it, second removes the branch; the
  rows show no trash on hover, and a remote row has no Delete item.
- Picking a branch that is already contained: "Already up to date", nothing
  moves, and the band never appears.
- Conflict: the merge stops, the band reads `2 files need attention`, Continue
  is disabled, both paths sit in Conflicts. Resolving each in the merge editor
  (Accept stages it) flips the band to Ready; Continue commits with
  `MERGE_MSG`; the band is gone.
- Abort restores the tree and removes the band; a dirty tree fails with git's
  line and changes nothing.
- Kill the app mid-conflict, relaunch: the band is back from the snapshot, not
  from memory.
- Probes: `merge_probe` in core (fast-forward, merge commit, conflict, abort,
  continue, dirty tree, detached HEAD) and a headless UI probe for the band's
  three states. Both run under a scratch `HOME`; the UI probe rides the
  `lib.rs` boundary like the others.
- The kit hook carries the six-crate gate — `cargo fmt --all --check`, clippy
  with `-D warnings`, and the kit tests — plus `cargo test -p warp-station`
  with the app checkouts linked.
