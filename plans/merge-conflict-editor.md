# Plan: the merge conflict editor

Status: phases 1–3 built locally 2026-11-14 — the marker parser with
per-stage line ranges, whole-file Local/Incoming panes with the block wash,
jump-to-conflict scrolling, the diff3 Base pane, the Split/Columns layout, the
directly editable Result with range remapping, Take both in either order,
word-level highlights, the conflicted row's whole-file menu and its
double-click entry, Accept (write + `git add`), and the
F7/F10/F11/Ctrl+Enter keys are in, with core and headless UI probes. Open:
wheel-lockstep scrolling, the pinned `Conflicts` section, delete/modify or
binary conflicts (the reader returns `None`, so those files have no editor
yet), and an open-in-editor verb. The owner settled the shape: a custom
conflict view, not a resolve card, with the sides called **Local** and
**Incoming** and the reference tool's features.

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
| Head right | `Take all Local`, `Take all Incoming`, `L+I` / `I+L`, layout toggle, `Base`, `Reset`, `Accept Merge` (primary) |
| Panes | the whole Local and Incoming files side by side, the editable Result below |

- **Layout** defaults to Visual Studio's split: the two sides on top, the
  Result below. `Columns` puts Local, the optional Base, Incoming and Result
  in one row; it is offered on a pane wider than 1000 px and falls back to the
  split otherwise.
- **Whole files.** Each pane shows its whole stage file, with every block's
  lines washed and the current block's washed stronger; the block ranges come
  from the parser's stage projections, so no three-way alignment pass is
  needed. A numbered strip carries `2 of 5`; prev/next or a chip scrolls every
  pane to that block at once. Wheel scrolling stays per pane (lockstep wheel
  sync is still open).
- **Base** appears between Local and Incoming only when the repository uses
  `merge.conflictStyle=diff3`, because a merge-style file has no base section
  to map.
- **Checking a side** on the current block (Local or Incoming, mirroring
  Visual Studio's checkboxes) rewrites that block's range in the Result with
  the side's text; the pane wash says which side it took. `L+I` and `I+L`
  write both sides in either order. Checking the chosen side again puts the
  markers back. Prev/next walks the blocks.
- **The Result is directly editable.** It starts as the marker-bearing
  worktree text and is the text Accept writes. Every keystroke is reduced to
  its smallest changed span, and every block range is remapped through it;
  a block the edit touches loses its pick and is marked manual (the strip
  shows it), and a later take replaces the edited range. `Reset` restores the
  worktree text and clears every pick.
- **Word-level highlights.** In the current block, corresponding Local and
  Incoming lines mark their changed middle (common prefix and suffix); a line
  the other side does not have is marked whole. The mark is a tint behind the
  changed tokens, red on Local and green on Incoming.
- **Accept Merge** is disabled while the Result still holds a conflict marker
  line. It writes the file, stages it (which clears the unmerged entry), and
  returns to the list.
- **Double-click** a conflicted row to open its editor; a single press only
  selects it.

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

1. **Landed.** Parser and conflict model in core, the block strip, the
   assembled Result, per-block checkboxes, Take all, Accept, band integration
   and shortcuts.
2. **Landed.** Whole-file panes with the parser's stage line ranges, the Base
   pane for diff3, the Split/Columns layout, jump-to-conflict scrolling, and
   the conflicted row's whole-file Local/Incoming menu.
3. **Landed.** Direct Result editing with range remapping, Take both (Local
   first, Incoming first), word-level highlights, and double-click entry.
   Open: wheel-lockstep scrolling, the pinned `Conflicts` section,
   delete/modify and binary conflicts, and an open-in-editor verb.

The phases are ordered; phase 1 is shippable on its own because Accept Merge
and the band close the loop.

## 7. Risks

- **Alignment.** The parser maps a block to its stage lines by counting the
  projection (common lines plus that side's sections), which is exact for a
  content conflict because removing the markers and the other side leaves the
  stage file. Delete/modify and binary conflicts break the projection and are
  therefore outside the editor for now.
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

- A scratch repo with a two-block conflict: the pane's button (or Enter) opens
  the editor with `0 of 2 resolved`; checking Local on the first and Incoming
  on the second updates the Result; Accept writes the file, stages it, and the
  row leaves Conflicts.
- The whole-file panes show the stage files with the blocks washed; Columns
  keeps four panes in one row; Base appears only for a diff3 fixture; the
  conflicted row's menu resolves every block to one side in one press.
- `L+I` writes both sides in order and Accept sends that text; a two-press
  row opens the editor; typing in the Result remaps the following block
  ranges (unit-tested), and Accept refuses while markers remain.
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
  with the toggle for columns on a wide pane.
- **D4 — Result editing. Settled by this plan:** direct editing; the Result is
  the text Accept writes, and block actions remap its ranges.
- **D5 — Base pane. Settled by this plan:** phase 2 toggle, not phase 1.
