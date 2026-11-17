# A merge editor for stopped operations

Status: phases 1–5 landed, then the owner asked for the Visual Studio 2026
shape; revision 2 is in progress and supersedes the rail page. The merge
editor is now a **detail mode**, not a rail section (D12 retired): Changes
shows a conflict banner with an Open merge editor button, and the editor takes
the repository detail body. Per-line checkboxes, the icon toolbar, word-level
highlighting, three-pane scroll sync, a configurable size cap and
"Merge into current branch" are the remaining revision-2 work.

Revision 2 so far: the editor lives in the repository detail (a mode, not a
tab), Changes shows the conflict banner and opens it, both source panes carry a
pick box per conflict line, and the Result is composed from the picks over the
file's original marker text (Current line first per row, then Incoming). The
toolbar is icon buttons — previous/next conflict, reload, save — with Stage as
the one filled action. The stale header count is fixed by a notify after the
pane seeds.

The Current/Incoming columns are now `warpui-kit-diff`'s aligned view
(`render_aligned`, `AlignedRow`, `AlignedOptions`) instead of the page's own
row control: the kit draws the gutters, the change washes and the intra-line
word emphasis, and the pick boxes are its opt-in `lead` property — with
`lead: None` the kit draws no leading column, so every other diff surface is
untouched. The app pins the kit by tag (`kits-v0.2.49`), so shipping this
needs a fresh kit tag and a pin bump in the same window.

The rest of the revision-2 list landed with it: the file-size cap moved into
`repos.merge_max_kb` (2048 KiB by default, clamped to at least 64) and reaches
`merge::session_with_cap` through `LoadMerge`; a conflicted "Merge into
current" opens the editor when the probe reports the stop; and conflict
navigation (Alt+↑/↓, the toolbar arrows, a strip row) scrolls the aligned
panes to the block's row and the Result editor to the block's line in the
composed text. Wheel-scrolling the three surfaces together is not wired.

## 1. What the tool is, and what the app has today

The app already knows an operation is stopped; it cannot act on it.

- libgit2 status marks unmerged paths `CONFLICTED`, and Changes already lists
  them with a `U` letter (`apps/warp-station/crates/warp-station-core/src/git/read.rs`).
  No view resolves them.
- Mutations go through the git CLI wrapper
  (`apps/warp-station/crates/warp-station-core/src/git/cli.rs`): `add`, `reset`,
  `commit`, `checkout`, `fetch`, `pull`, `push`. There is no `merge`, `rebase`,
  `cherry-pick`, `--continue` or `--abort`, so a stopped operation always begins
  and ends outside the app.
- `git2 0.21` is already a dependency; `Index::conflicts()` yields base, ours
  and theirs without shelling out.
- The diff kit (`kits/warpui-kit-diff`) parses a unified patch into a two-way
  document and renders it; it has no three-way model and no editing.
- The editor layer re-exported by `gpui-component` is a real code editor with
  range decorations, gutter decorations and an overview ruler; Settings already
  drives it as `EditorState`.
- `similar` 2 is already in the dependency tree (the diff kit diffs with it), so
  line diffing needs no new dependency family.

The reference is Visual Studio 2022's merge editor: Incoming and Current above
an editable Result, per-block take actions, and a file counted resolved only
when no conflict remains in it.

## 2. Non-goals

- Starting, committing, aborting or continuing the operation. Station resolves
  files and stages them; `git commit`, `rebase --continue`,
  `cherry-pick --continue` and `* --abort` stay where they are.
- Word- or line-level shading inside a conflict block: one tint per block.
- Partial staging, submodules and symlinks; whole-file actions cover whatever
  has no text form.
- A general three-way comparison of arbitrary revisions: the page exists only
  while the index is unmerged.

## 3. The contract

- **D1 — three panes, full file, aligned.** Incoming and Current above, Result
  below. Every pane shows the whole file, regions align row for row across
  panes, and the two source panes scroll together. The base (`:1:`) is an input
  to alignment, never a fourth pane.
- **D2 — the Result is editable text, and the buffer is the resolution state.**
  Take actions rewrite a region; after that the user types freely and undo/redo
  is the editor's. A file is resolved when a re-parse of the buffer finds no
  complete conflict block and no orphan marker line; an edit that leaves a
  half-deleted marker therefore stays unresolved rather than counting clean.
- **D3 — a block offers take incoming, take current, take both**, with both
  orderings ("both" respects which side comes first), applied to the block's
  region immediately and undoably.
- **D4 — the Result seeds from the worktree file.** Opening a file reads it from
  disk and parses its markers, so an edit made in another editor survives; the
  side panes take ours and theirs from the index stages. A path whose stages are
  gone (already staged by hand) is not offered for editing.
- **D5 — labels follow the operation.** Merge: Current / Incoming. Rebase: Onto /
  Your commit. Cherry-pick: Current / Picked commit. Revert: Current / Reverted
  commit. Git's stage-2 and stage-3 names sit in a tooltip.
- **D6 — the page exists only while an operation is stopped**, for any unmerged
  index: merge, rebase, cherry-pick or revert. One page shows one repository at
  a time, with a switcher row when several are stopped. The operation comes from
  git2's repository state, not from sniffing `.git` paths.
- **D7 — saving is unconditional, staging is gated.** Save writes the worktree.
  Stage is disabled while the file has an unresolved block, showing the count;
  when it runs it is `git add -- <path>`, which also clears the unmerged entry.
- **D8 — non-text conflicts are whole-file rows.** Binary, delete/modify, add/add
  and rename conflicts get a row with Take ours / Take theirs; the pick checks
  out that side (or removes the path for a delete) and stages immediately.
- **D9 — recovery is reload only.** Reload re-reads the worktree and discards the
  buffer; there is no abort button.
- **D10 — no completion card.** When the last unmerged entry clears, the page
  disappears; committing or continuing stays with the user.
- **D11 — block tint only.** One background per block; no word or line diff
  inside it.
- **D12 — the surface is a rail page**, not a replacement for the diff pane.
  Changes keeps its two-way diff and gains a row that jumps to the Merge page for
  a `U` file.

## 4. Read model and surface

### 4.1 Core model

`MergeState` per repository: the operation kind and its label pair, and the
files. A text file carries its path, its conflict blocks (base/ours/theirs/
result ranges), and aligned rows for the whole file. A non-text file carries a
kind and two actions.

Alignment is two pairwise line maps, base↔ours and base↔theirs, composed into
one row map; marker regions fix block boundaries, and the composition decides
which clean regions sit beside each other. `similar` supplies the sequence
diff. The model lives in `warp-station-core` so probes run headless, with the
diff kit untouched — it stays the two-way renderer it is.

Detection uses `Index::conflicts()` for the stages and git2's repository state
for the operation; precedence when several markers coexist is proof 5.
Parsing covers git's marker grammar: `merge`, `diff3` and `zdiff3` styles,
`merge.conflictStyle`, `merge.markerSize`, and custom labels.

### 4.2 Surface

- Rail section `Merge`, present only while some repository is stopped.
- Header: repository, operation label, unresolved count, switcher when several
  repositories are mid-operation.
- File list: text files with per-file unresolved counts; non-text rows below.
- Body: Incoming and Current as read-only editors with line numbers and block
  tints, one shared vertical scroll; Result below as an editable editor with
  each block's actions pinned at its first row.
- Toolbar: Reload, Save, Stage with the gate and count; the file path.
- States: dirty buffer on file switch asks before discarding; the page closes if
  the operation ends elsewhere; a worktree change on disk prompts when the
  buffer is dirty and reloads silently when it is not; a failed stage says why
  and refreshes instead of retrying.

### 4.3 Keyboard

The focused pane owns its navigation, per the suite rule. Next/previous conflict
moves within the Result and the source panes follow it; nothing is bound at the
window level.

## 5. Proofs before the first slice

1. **Decorations.** In the pinned `gpui-base`, how a range tint is added,
   whether it survives edits to the buffer, and whether a decoration can carry a
   click action or the actions need a gutter or an overlay.
2. **Marker grammar.** Fixtures for `merge`, `diff3`, `zdiff3`, a custom
   `markerSize`, and custom labels; the orphan-marker policy from D2.
3. **Alignment cost.** Two diffs plus composition on a 10k-line file; decide the
   cap or the virtualization before the panes are drawn.
4. **Staging.** `git add` on a conflicted path clears the unmerged entry; the
   error shape when the worktree changed under us.
5. **Operation detection.** git2's repository-state variants and their
   precedence when a rebase directory and a cherry-pick head coexist.
6. **Probe harness.** Store and view through `lib.rs`, fixture repositories
   under a scratch `HOME`.

Proofs 2, 3, 5 and 6 closed with phase 1: the marker and operation fixtures are
`tests/merge_conflicts.rs`, a 10k-line file reads in about 16 ms in a debug
build, `MergeOperation::of_state` maps git2's repository states, and the
fixtures build real repositories under the system temp dir. Proofs 1
(decorations) and 4 (staging) clear with phases 3 and 4, where they are used.

## 6. Phases

1. Core read model: operation detection, text and non-text conflict listing,
   marker parsing, blocks, aligned rows; unit and probe tests over fixture
   repositories. No UI.
2. Page shell: rail visibility, repository switcher, file list, the two source
   panes from the model, the vanished and empty states.
3. Result editor: seeding, block tints, take actions, unresolved count, Save,
   reload, dirty guard.
4. Staging: the gate, `git add`, non-text rows, disappearance, external-change
   handling.
5. Polish: conflict navigation keys, accessible names, motion, a capture.

## 7. Open work

- Word or line shading inside blocks if the tint proves too coarse.
- An optional base pane.
- Shortcut-sheet entries and a `?` row.
- A size cap for very large files, decided by proof 3.

## 8. Landed (phase 1)

`warp-station-core/src/git/merge.rs`: `MergeSession` and `MergeOperation`
(state-driven, with the operation-aware side labels), `TextMerge` with the
parsed blocks and the block-by-block source row alignment, and whole-file
conflicts for binary, delete/modify, too-large, special and unreadable paths.
`tests/merge_conflicts.rs` drives real merge, rebase, cherry-pick,
delete/modify, binary, diff3, zdiff3, custom marker size and 10k-line
fixtures; the in-module tests cover the marker grammar and span location.
`similar`, already in the tree through the diff kit, is the line diff.

Phase 2: `src/ui/merge.rs` holds the rail page — a section that exists only
while a probe reports a stopped operation, the repository switcher, the
conflict list and the aligned Incoming/Current panes drawn from
`TextMerge.rows` with one tint per conflict block. `src/ui/window.rs` gains
`Section::Merge`, the conditional rail item and the section-fallback guard;
`Probe.merge` and `StatusProbe.merge` feed `Store::merge_ops`; the page loads
through `NetCommand::LoadMerge` / `UiEvent::MergeLoaded`, and
`tests/merge_probe.rs` covers the page draw and the rail's appear/disappear
rule. The `WARP_STATION_PAGE=merge` hook opens it for a capture.

Phase 3: the Result pane is an `EditorState` seeded from the worktree file and
tinted per remaining block with range decorations. The conflict strip carries
take-current / take-incoming / take-both (with a both-order toggle) and rewrites
the buffer in place; the buffer is re-parsed after every edit through
`buffer_conflicts` and `unresolved_count` — orphan marker fragments included —
so the header, the file row and the toolbar follow the text. Save writes the
worktree, Reload re-reads it, and a file or repository switch is refused while
the buffer is dirty. `tests/merge_probe.rs` drives a take and a save against a
fixture file.

Phase 4: `Probe.merge` names the operation only while the index still has
unmerged paths, so the rail item and the page leave as soon as the last path is
staged. `cli::take_side` resolves a whole-file conflict — `checkout --ours` /
`--theirs` followed by `git add` (checkout alone leaves the index conflicted),
or `git rm` for the side that deleted the path. The page stages a resolved
buffer through `NetCommand::MergeAction`; the store reloads the session and
re-probes that repository, and the page shows a resolved state between the
last stage and the probe. A worktree change on disk is followed: a clean buffer
reloads silently, a dirty one shows a Reload note. Whole-file rows carry
take-ours / take-theirs buttons.

Phase 5: the conflict strip owns a focus handle, and Alt+Up / Alt+Down walk its
blocks with the active row tinted; the index is clamped after every re-parse.
The take buttons carry accessible names that say which conflict they act on
(`neutral_button_named`), the shortcut sheet gained a Merge group, and the merge
probe exercises the keyboard walk. No capture was taken: the documented
hidden-window recipe drives the live desktop, which this session leaves alone.
