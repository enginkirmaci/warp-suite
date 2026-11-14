# Plan: the merge conflict editor

Status: phase 1 built locally 2026-11-14 — the marker parser and index-stage
read, the one-block Local/Incoming/Result editor, per-side take checkboxes,
take-alls, the manual Result toggle, Accept (write + `git add`) and the
F7/F10/F11/Ctrl+Enter keys are in, with core and headless UI probes. Open:
phase 2 (whole-file panes, layout toggle, Base, synchronized scrolling, bulk
resolution) and phase 3 (direct Result editing, Take both, word-level
highlights). A delete/modify or binary conflict reads as no conflict (the
reader returns `None`), so those files have no editor yet; phase 2 owns them. The owner settled the shape: a custom conflict view, not a
resolve card, with the sides called **Local** and **Incoming** and the
reference tool's features.

## 1. The reference

Visual Studio's Merge Editor, from its
[Resolve merge conflicts](https://learn.microsoft.com/en-us/visualstudio/version-control/git-resolve-conflicts?view=vs-2022)
page: conflicts walked line by line with a checkbox for each side, **Take
Incoming** / **Take Current** for the whole file, a manually editable
**Result**, **Accept Merge** per file, a layout switch, and the Unmerged
Changes list where a double-click opens the editor and a right-click offers
bulk Keep All Current / Take All Incoming.

Taken: all of it, with the owner's naming — **Local** for the current branch's
side, **Incoming** for the branch being merged. Current, ours and theirs do not
appear in the UI.

Not taken: the gear menu's full layout matrix (one toggle ships) and LSP-aware
line merging. The bulk commands are phase 2.

## 2. Where it lives

- A full-pane state in Changes, the same shape as the History drill-down: it
  replaces the toolbar and columns, keeps the merge band above it, and leaves
  with Back or Esc.
- Entry: double-click (or Enter on) a conflicted row in the `Conflicts`
  section. A single click keeps the right pane's summary — file, `3 conflicts`,
  `Open merge editor` — so the list keeps its selection semantics.
- One file at a time. Accept Merge writes and stages that file and returns to
  the list; the band's Continue enables once no unmerged path is left.

## 3. The screen

A 52-high head, then the panes:

| Part | Content |
|---|---|
| Head left | Back, the path, `2 of 5 resolved`, prev/next conflict |
| Head right | `Take all Local`, `Take all Incoming`, layout toggle (phase 2), `Accept Merge` (primary) |
| Panes | the current block's Local and Incoming sides side by side, the assembled Result below |

- **Default layout** is Visual Studio's split: the two sides on top, the
  Result below. The toggle flips to three columns on a wide pane and stacks
  everything on a narrow one.
- **One block at a time in phase 1.** The panes show the block under the
  cursor, not the whole file: that needs no three-way alignment and stays
  readable at the app's pane widths. A numbered conflict strip above the panes
  carries `2 of 5` and jumps to a block; phase 2 grows the same two panes into
  the full files with the block highlighted.
- **Checking a side** on the block (Local or Incoming, mirroring Visual
  Studio's checkboxes) resolves that block and the Result updates in place;
  the block's tint — green for Local, blue for Incoming — says which side it
  took. Prev/next walks the blocks.
- **Result**: assembled from the blocks, read-only until `Edit manually`, which
  swaps in an editor seeded with the current assembly; block actions disable
  while manual, and `Reset choices` returns to the assembled view. Visual
  Studio edits the Result directly; a mode switch is the honest first cut
  because a free edit has to remap every block range.
- **Accept Merge** is disabled until every block has a choice or the Result is
  manually edited. It writes the file, stages it (which clears the unmerged
  entry), and returns to the list.

## 4. Data and plumbing

- `read::merge_conflict(folder, path) -> Option<ConflictFile>` reads the three
  index stages through `git2::Index::conflicts()` (ancestor, local, incoming)
  plus the worktree file.
- The panes highlight with the file's language, chosen from the path the same
  way the diff body chooses it.
- The worktree is parsed for conflict markers: seven or more `<`, optional
  `|` (diff3), `=`, `>`. Each block carries its result range and the Local /
  Incoming texts. A file with no markers, an unknown style, or a binary has no
  blocks; the screen falls back to whole-file `Take Local` / `Take Incoming`.
- `Choice::{Unresolved, Local, Incoming}` per block; the Result is assembled by
  replacing the block ranges in order. A manual edit stores the text instead.
- `NetCommand::ResolveConflict { folder, path, text }` writes the worktree file
  and runs `git add`; the existing post-mutation reload sends the fresh
  snapshot, so the row leaves `Conflicts` and the band flips to Ready.
- Delete/modify conflicts: one side has no text. The block shows `(deleted)`
  on that side and Accept writes the chosen side, deleting the file when that
  side is the deletion.
- Settings: none. The layout choice is view state, not a preference.

## 5. Shortcuts

A new `Merge conflicts` group in the shortcut sheet:

| Key | Action |
|---|---|
| `F7` / `Shift+F7` | Next / previous conflict |
| `F10` | Take all Incoming |
| `F11` | Take all Local |
| `Ctrl+Enter` | Accept Merge |

`F10`/`F11` follow Visual Studio; the sheet proves they are free.
`Ctrl+Enter` is contextual: the commit box owns it, and the merge editor owns
it while the editor is open.

## 6. Phases

1. **Usable editor.** Parser and conflict model in core; the one-block screen
   with the block strip, Local/Incoming sides, the assembled Result, per-block
   checkboxes, Take all, Accept, band integration and shortcuts.
2. **Full three-way view.** Whole-file panes with a Base toggle, block ranges
   mapped by a diff pass, synchronized scrolling, the layout toggle, and bulk
   Keep All Local / Take All Incoming from the `Conflicts` section's context
   menu.
3. **Polish.** Direct Result editing with range remapping, Take both (Local
   first, Incoming first), word-level highlights, and an "open in editor" verb.

The phases are ordered; phase 1 is shippable on its own because Accept Merge
and the band close the loop.

## 7. Risks

- **Alignment.** Mapping a block onto the Local/Incoming files needs a diff
  pass (Base→Local, Base→Incoming); repeated blocks make text search a
  heuristic. Phase 1 avoids whole-file alignment, so the risk lands in phase 2.
- **Editor adoption.** Phase 1's block panes use the diff kit's read-only
  `render_code` (line numbers, no syntax coloring) and a `TextareaState` for
  the manual Result; phase 2's whole-file panes want the pinned
  `EditorState`/`Editor` from gpui-base for decorations and synchronized
  scrolling — the suite's first use, with theme, font, focus and keybinding
  wiring as new work.
- **Scroll sync.** The editor owns its scroll handle; phase 2 needs a shared
  handle or an exposed `scroll_to`.
- **Marker styles.** `merge.conflictStyle=diff3` adds a base section and
  zdiff3 changes its shape; anything unrecognized falls back to whole-file
  takes rather than guessing.
- **Large files.** Three full copies per screen; the editor virtualizes and
  the parser is linear, but a 100k-line file still costs a full parse per open.
- **Binary conflicts** have no text; the screen must say so and offer the two
  whole-file sides.

## 8. Acceptance

- A scratch repo with a two-block conflict: double-clicking the row opens the
  editor with `0 of 2 resolved`; checking Local on the first and Incoming on
  the second updates the Result; Accept writes the file, stages it, and the
  row leaves Conflicts.
- `Take all Local` / `Take all Incoming` resolve every block in one press;
  `Ctrl+Enter` accepts when the Result is complete.
- Manual mode: edit the Result, Accept writes exactly that text; Reset returns
  to the block choices.
- Open: a delete/modify conflict and a binary conflict each render the
  fallback screen and accept their whole-file side (phase 2).
- The band survives: after the last conflict is resolved, Continue commits
  with `MERGE_MSG`; Abort still restores the tree from the editor.
- Esc and Back leave the editor with the file's choices intact in the view;
  reopening the same file returns to the same block and the same choices.

## 9. Decisions

- **D1 — Labels. Settled 2026-11-14:** Local and Incoming; no "current",
  "ours" or "theirs" in the UI.
- **D2 — Shape. Settled 2026-11-14:** a custom conflict editor with the
  reference tool's feature set, not a resolve card and not a plain diff.
- **D3 — Default layout. Settled by this plan:** Local | Incoming over Result,
  with the toggle for three columns in phase 2; phase 1 shows one block at a
  time.
- **D4 — Result editing. Settled by this plan:** read-only assembly plus an
  `Edit manually` toggle in phase 1; direct editing is phase 3.
- **D5 — Base pane. Settled by this plan:** phase 2 toggle, not phase 1.
