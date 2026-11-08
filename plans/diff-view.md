# Plan: a diff view instead of a diff renderer

Status: revision 3 landed (split columns with their own horizontal
scrollbars, and a persisted word-wrap toggle for both layouts). Revisions 0–2
landed too (the document, the unified view, hunk rows removed, the split
view, states, full drag selection and the single-icon layout toggle). Phase 2
(virtualization and the pinned number column), phase 6 (consumers) and the
rest of phase 3 (counts, copy, `n`/`p`) are open. Decisions D1–D13 are
recorded in §5; D7–D12 came with revision 2, D8 is the single icon, and D13
is the wrap/per-column-scroll pair.

## 1. The problem

The starting state (phases 0–1 fixed the drawing rules below; revision 2
removed the hunk rows phase 1 added):

The diff pane in Changes is `render_diff`: a function that turns a patch into
a column of divs from scratch on every render. Nothing between the patch text
and the pixels remembers what a hunk is. That is why the pane cannot fold,
navigate, pair sides or stay cheap on a large patch — and why the visual
defects below are structural rather than a styling pass.

What the current output does:

- **Misaligned code column.** Context rows keep the patch's leading space,
  while added and deleted rows have their marker stripped. Context text
  therefore starts one character to the right of the code it belongs with. A
  unified diff's first column is a marker column; all three row kinds have to
  consume it.
- **No line discipline.** A row is a `StyledText` in a width-constrained div,
  and `StyledText` wraps by default. A long line becomes extra visual lines
  that carry no gutter number and no hanging indent, and the pane has no
  horizontal scroll to reach the rest.
- **Frame noise.** Each file patch opens with `diff --git`, `index`, `---` and
  `+++` rows, which repeat the path already in the 44px header. Hunk bands
  start at the pane edge, not on the code column.
- **One wash for both halves.** Added and deleted rows get the same 13%
  green/red background across gutter and content. The "brighter over the
  gutter, quieter over the content" the crate documents is not implemented,
  and intra-line changes are invisible: a renamed identifier in a long line
  looks exactly like the rest of the line.
- **No view state.** The header shows only the path: no counts, no copy, no
  view mode, no hunk navigation. `loading patch…` is passed through the
  renderer and becomes diff row 1; the 64KB truncation suffix becomes a
  numbered code row.
- **Whole-patch layout.** Every `ChangesView` render rebuilds and lays out
  every row, and the view re-renders on every store notify. The highlighter
  caches token styles by text, but the element tree and its layout are
  rebuilt each time.

That list is the diagnosis, not the plan. The plan is to give the kit a
document, give the app a view, and then fix the drawing.

What stays: monospace, tree-sitter colors, the dark `well` body, the
file-list/commit-box column, and the block following the app's text size.

## 2. Target

### 2.1 The document

`warpui-kit-diff` parses a patch once into `DiffDocument`:

- files, each with its path(s), status and add/delete counts;
- hunks with their ranges and function context;
- rows with `old: Option<u32>`, `new: Option<u32>`, kind and text;
- highlight runs per side, not per joined buffer (a hunk's deletions and
  additions are highlighted as two programs, so one side's brace or quote
  cannot miscolor the other).

Parsing stays pure — no `Window`, no `App` — so patch fixtures can test it
headlessly. `render_diff` remains as a thin one-shot wrapper for callers that
only want a block (unified), and `render_diff_with` takes a `DiffOptions`
when a caller has a mode; the migration does not break warp-agent.

### 2.2 The view

The app keeps a small view state on `ChangesView` (view mode, focus and
selection) and keeps the parsed document for the selected patch, keyed by
patch text. Switching files reuses a cached document; a store notify does not
reparse.

Anatomy (unified):

```text
path/to/file.rs                              [ wrap ] [ split icon ]
──────────────────────────────────────────────────────────────────────────
  533      next_ok && prev_ok
  534  }
       −   /// Emit the runs of an emphasis forest:
  541  +   /// Emit the runs of an emphasis forest:
```

Split:

```text
path/to/file.rs                             [ wrap ] [ unified icon ]
──────────────────────────────────────────────────────────────────────────
  533       next_ok && prev_ok            533       next_ok && prev_ok
  534   }                                 534   }
       −   /// Emit the runs …           541  +   /// Emit the runs …
```

- Header: the path on the left, truncating with an ellipsis; two icon
  buttons at the right edge: word wrap (active while on) and the layout
  toggle, whose glyph names the *other* mode. Status, counts, copy path and
  overflow wait for phase 3.
- **No hunk rows.** The `@@ … @@` line is parsed (the document keeps the
  header text and ranges) but never drawn. Hunks are separated by the three
  context lines git emits; a copied patch gives code, never `@@` text.
- **Unified numbers.** Context and additions show the new-file number;
  deletions show none. The − marker and the wash carry the deletion.
- **Split numbers.** The left column is the old file: context and deletions
  carry the old number, the − marker and the delete wash. The right column is
  the new file: context and additions carry the new number, the + marker and
  the add wash. Both columns share one gutter width so their code columns
  line up.
- A 3px change bar at each column's leading edge, a strong gutter wash and a
  quiet content wash; code keeps its tree-sitter colors. Intra-line emphasis
  marks the changed span on paired rows, on both sides.
- No wrap by default: one row is one line and the body scrolls horizontally;
  this is also what makes rows regular enough to virtualize. Pinning the
  number column is the follow-up once the virtualized list lands.
- Split splits the pane evenly and each column owns its horizontal viewport,
  so a long line on one side never pushes the other side off screen. The
  340px floor and the whole-body scrollbar are gone; there is no width-based
  fallback to unified.
- Word wrap is a second toggle. On, each source row is an element: the code
  wraps in its cell, the number stays on the first visual line, and a split
  row takes the taller side's height so the next row starts on the same line
  in both columns. Wrapped selection is row-scoped; the unwrapped body keeps
  the one whole-patch selection.
- Real states for loading, empty/binary, truncated and failed patches, in the
  same visual language as the clean-tree state. In split, a state-only,
  multi-file or truncated patch renders unified (D10).

### 2.3 Interaction

- The header toggle switches unified/split immediately and saves the pick;
  the next launch opens in that mode.
- `n` / `p` and header buttons move to the next/previous hunk (phase 3).
  There is no hunk strip to click; the planned fold affordance goes with the
  strip; a replacement fold control is out of scope.
- Unified: one selectable text over the whole patch; a drag crosses lines and
  copy gives the code. Split: one selectable text per column; a drag stays in
  its column and copy gives that side's code. The app mounts
  `TextSelectionLayer` as today.
- Nested scroll: the history detail's expanded patch and the pane's own
  scroller must not fight. The existing wheel chaining stays and gets a probe.

## 3. Non-goals

- No editing, staging or inline review; Changes stays a read-only view.
- No folding in revision 2. Hunk strips were going to be the fold control,
  so removing them removes that plan; a replacement is a later decision.
- No context expansion in v1 (needs a `-U` fetch).
- No web view, no second renderer, no per-app colors.

## 4. Phases

0. **Fixtures first.** Add patch fixtures for a hunk mid-file, an untracked
   pure-add, a pure-delete, a rename, a binary, a truncated patch, CRLF, a
   500-character line, and a patch without a trailing newline. Add pairing
   fixtures for split: more deletions than additions and the reverse, a
   change block that starts with an addition, and a hunk whose first row is a
   deletion or an addition (a leading filler on one side). Assert the parser
   and `split_rows` on these, not the pixels.
1. **Document + unified view (the look) — landed.** The marker column is
   consumed for every kind, rows do not wrap, the body scrolls horizontally,
   metadata takes no rows, the wash is two-tone, paired lines get word-level
   emphasis, and states are drawn. Phase 1 scrolls the row as a whole: GPUI's
   div scroll shares one offset across both axes, so the pinned number column
   waits for the virtualized list.

   **Hunk rows out — landed.** Drop `Line::hunk`, `LineKind::Hunk`, the
   hunk arm of `line_wash` and the hunk run in the highlight match; delete
   `DiffHunk::title` and its test, keeping the parsed header and ranges. Update
   `diff_view_probe` (one strip plus four rows → four rows) and
   `diff_size_probe` (four rows → three), and add a probe that the body height
   is exactly the code rows. Change the design doc's "hunk headers are quiet
   strips" rule to "hunk headers are not rows".
2. **Cache + virtualize.** The kit already caches the parsed document inside
   `render_diff`; the app-side document and the row list are what is left.
   Keep the parsed document in `ChangesView`; render rows through
   `uniform_list` with the header outside it. Rows need one common height for
   that. Target: a 2k-line patch scrolls without rebuilding the tree, and the
   number column can be pinned.
3. **Header and navigation.** Counts, copy path, next/prev, and the
   empty/truncated states wired to the store.
   A horizontal-scroll affordance belongs here too: the body scrolls, but
   nothing on screen says so.
4. **Split and the persisted toggle — landed.** Four steps, each independently
   testable:
   a. **Model.** `DiffMode { Unified, Split }` and `DiffOptions { mode }` in
      the kit; `DiffHunk::split_rows()` pairs a hunk into
      `SplitRow { left: Option<usize>, right: Option<usize> }` (indices into
      `hunk.rows`). Context pairs with itself; a change block pairs its
      deletions and additions by position, leftovers get one empty side.
      `emphasize_hunk` and `split_rows` share one block walker so the two
      pairings cannot drift. Pure tests, no window.
   b. **Renderer.** Extract the column builder (gutter + code + canvases +
      selection) from `render_document`; unified stays one column, split is
      old column + 1px divider + new column. Old cells use `row.old`, the
      delete wash and the old-side highlight runs; new cells use `row.new`,
      the add wash and the new-side runs; a context row appears in both with
      no wash. `highlight_hunk` returns both sides' runs, context included in
      both buffers, so a context row is styled from the side it is drawn on.
      A `Fill` line keeps the empty side's row count and height. One common
      gutter width comes from the document's largest old and new numbers.
      `render_diff` keeps its signature (unified) and
      `render_diff_with(..., DiffOptions, ...)` carries the mode; history
      detail and warp-agent do not change. Split requires one file, hunks and no truncation; a multi-file or
      truncated document renders unified (D10). The split columns expose
      `diff-left-*` / `diff-right-*` debug selectors so probes can address
      each side.
   c. **App header.** `ChangesView` gains `diff_view: DiffViewMode` seeded
      from settings in `new` and never reset by `sync_repo_reset`. The 44px
      diff header becomes path (left, truncating) + a single icon button at the
      right (`diff-view-toggle`, tooltip/a11y "Switch to split view" or
      "Switch to unified view", two small SVGs); a click updates the field,
      writes `changes.diff_view` through `Store`, and repaints.
      `render_diff_with` passes the mode with `theme::font_control()`.
   d. **Probes, demo and captures.** Kit: left/right alignment, per-side
      numbers and markers, filler rows, copy per side, long-line no-wrap,
      per-column viewport scrolling, wrap fits the pane, and a multi-file
      split call falling back to unified. App: both icon buttons draw, the
      layout and wrap toggles flip, the settings file holds both picks, and a
      second `ChangesView` over the same file starts with them. `examples/diff_demo` draws the same patch unified and
      split, and hidden-window captures of the Changes pane in both modes are
      taken with the recipe in `docs/agents/testing-and-captures.md`. Add
      `render_diff_with` to the `kits/README.md` export list.
5. **Selection (D6) — landed.** The body is one `SelectableText` over the whole
   patch, so a drag crosses lines and copy gives the code. The gutter is
   drawn beside that text, not inside it: copying a selection never carries
   line numbers or +/− markers. File headers and state lines are lines in the
   same text. The element subscribes to its selection handle
   and notifies the current view, so the highlight follows the drag instead
   of waiting for the next unrelated repaint. It also maps the drag's two
   endpoints with `TextLayout::index_for_position` and supplies copy through
   a callback: the window selection's own projection walks every character of
   the run, which took ~800 ms per paint on a 900-line patch. The same drag
   now paints in ~10 ms in the debug probe.
6. **Consumers.** Point the history detail and warp-agent's tile at the same
   document and renderer; the tile keeps single numbers and no header.

## 5. Decisions

D1. **Split default — settled: unified first, split opt-in, last pick
saved.** New installs open unified; the toggle writes the choice and the next
launch restores it. At the default 1500px window the pane is ~1180px, so a
split leaves ~360px per side with the repo list open (~510px once it is
hidden) — roughly 35–40 characters at 14px, usable for short lines, and the
default is still unified.

D2. **Long lines — settled: horizontal scroll by default, word wrap by
toggle.** The default preserves code shape and regular rows; wrap is for a
narrow pane and for reading a long line without scrolling. The gutter pin
follows with the virtualized list.

D3. **Diff size — settled: `font_control` (14).** Reading code beats matching
the path/hash step; the shared text size still scales it.

D4. **Numbers — settled: unified new-file only; split per side.** Unified
context and additions carry the new number; deletions carry none, because
their old-file number points at a line the file no longer has. Split shows
the old number on the left and the new one on the right (D10).

D5. **Keys — settled: `n`/`p` only; the clickable hunk strip is gone.** The
strip was going to be the fold control; removing the `@@` rows removes that
plan, so a replacement (header chevrons or a gutter control) is a later
decision, not part of revision 2. `n`/`p` still land on hunk starts when
phase 3 lands.

D6. **Text selection — settled: full drag selection and copy.** The body is
one selection participant over one `StyledText` covering every line, with the
gutter and row washes as a layer beside/behind it. A headless drag across
lines copies the code without the gutter. `warpui-kit-diff`'s
`SelectableText` is the adapted element; the app must mount
`gpui_kit::base::TextSelectionLayer` (Warp Station and warp-agent do).

D7. **Hunk rows — settled: not drawn.** No `@@` row and no separator row in
its place; the `-U3` context is the only gap between hunks. The parser keeps
`DiffHunk.header`, `old_start`, `new_start` and `new_count`;
`DiffHunk::title` is deleted. `DIFF_HUNK_BG` stays because the markdown fence
highlighter (`highlight.rs::diff_styles`) still paints `@@` lines inside
fenced diffs.

D8. **Toggle — settled: a single icon button.** The header's right edge gets
one ghost icon button. Its glyph names the other mode (a split-columns icon
while unified, a single-column icon while split), and its tooltip/a11y name
is the action ("Switch to split view" / "Switch to unified view"). The icon
keeps the 44px header quiet and needs no segmented trough; a plain mode
switch still never takes the accent fill. Clicking it flips the mode; there
is no separate "current mode" label.

D9. **Persistence — settled: `changes.diff_view` and `changes.diff_wrap`.**
`~/.warp/station/settings.json` gains a `changes` section with `diff_view`
(`unified` default, `split`) and `diff_wrap` (false). They are global, not per
repository, and additive: an absent section reads unified/unwrapped.
`DiffViewMode` deserializes leniently — unknown strings fall back to unified —
so a stale or hand-edited value does not invalidate the rest of the settings
file. `ChangesView::new` seeds both local fields; each pick calls
`Store::set_diff_view`/`set_diff_wrap`, which go through the existing atomic
`update_settings` (a write failure only logs; the session keeps the pick).

D10. **Split layout — settled.** Left is the old side (`row.old`), right the
new (`row.new`); context appears on both with no wash; a deletion only on
the left with `DIFF_DEL_*`; an addition only on the right with `DIFF_ADD_*`.
Paired rows share a line; unpaired cells are empty `Fill` lines so the
columns never drift. One common gutter width for both columns. A 1px
`palette::hairline()` divider between them. The two columns split the pane
evenly; each is a horizontal viewport, so its long lines scroll on their own
and the other column stays put. State-only, truncated and multi-file patches
render unified (the Changes pane, the history detail and warp-agent all show
one file per call); stacking per-file split blocks is a later change if a
caller needs it.

D11. **Selection in split — settled: per side.** Each column is its own
`SelectableText` with its own id and copy callback; a drag stays in one
column. Copy gives the old side's or the new side's code, never both and
never a mix. Unified keeps the single existing participant. This avoids a
fake joined buffer whose line pairing would not map back to one file.

D12. **Kit API — settled: additive.** `render_diff` keeps its signature and
stays unified, so warp-agent's tiles and the history detail do not change.
`render_diff_with(text, path_hint, DiffOptions { mode, wrap }, font_detail,
window, cx)` is the new entry for callers with a mode. `render_document`
takes the options and stays the one body builder; the two `render_diff*`
wrappers call it. Do not fork the renderer.

D13. **Wrap and split scrolling — settled.** The unwrapped body keeps the one
text + canvas renderer; split wraps each column in its own horizontal
viewport. The wrapped body is a deliberate row-per-line renderer: a row's two
cells live in one flex row, so the row is as tall as the taller wrapped side
and the columns stay aligned. Wrapped rows are separate selectable texts, so
a drag copies one row; the unwrapped body keeps the whole-patch selection.
The wrap flag is `DiffOptions::wrap` and the app persists it beside the
layout (`changes.diff_wrap`).

## 6. Risks

- **Kit API — additive.** `render_diff` keeps its signature and every current
  caller keeps compiling; only the new `render_diff_with` carries a mode. Do
  not fork the renderer or add a second diff body builder.
- **Row alignment.** Split's two columns must use the same pinned font size
  and line height as the unified body; the canvases address rows as
  `index × line_height`. A `Fill` line must still lay out as one text line
  (the existing blank-line probe covers that property in unified); the split
  probe asserts equal column heights and a leading/trailing filler.
- **Probe churn.** Removing the hunk strip changes every row count:
  `diff_view_probe` divides by 5 today and `diff_size_probe` by 4; both
  become the code-row count. Grep for `@@`-row assumptions before running.
- **Settings shape.** Per-app settings are strict (`deny_unknown_fields`), so
  `changes` is a one-way additive key like `notifications`: an older build
  cannot read it, and the schema version is not bumped. The lenient
  `DiffViewMode` parse only protects the new build from a bad value, not a
  downgrade. Do not bump `schema_version` for this.
- **Selection in split.** Two participants per body means the window
  selection belongs to one column at a time; the copy callback must return
  only that column's text. The existing no-selection `update_runs` refresh
  runs per participant and is cheap.
- **Two scroll axes.** A virtualized list owns vertical scrolling; horizontal
  scrolling must live on the code body with a synchronized gutter. Prove the
  combination with a probe in phase 2 before building the rest on it.
- **Selection under scroll.** The selection registration carries no ancestor
  scroll offset yet, so a selection held while the pane scrolls can drift.
  Fix it when the diff owns its scroller in phase 2 (pass the offset through
  `TextSelectionRegistration::with_scroll_offset`).
- **Patch shapes — guarded.** libgit2 emits CRLF, `\ No newline`, binary
  markers, renames and the 64KB truncation suffix; `document::tests` holds
  them. A deleted line starting with `---` followed by an added line starting
  with `+++` is still read as a file header — rare enough to document, not to
  solve with hunk counts.
- **Contrast — guarded.** `palette::tests::diff_washes_keep_their_ink_readable`
  holds the line numbers, comments and code to AA on the washes.
- **Dirty tree.** The settings migration in flight already edits this area's
  neighbors and the theme. Keep the diff work on top of the current files and
  do not reformat or revert them.
- **Dependency — settled.** Intra-line emphasis uses `similar 2.7`; it was
  already in the lock through gpui-kit's `gpui-component`, so no new crate.

## 7. Done when

Revisions 2–3 — landed:

- No `@@` row is drawn in unified or split; the body height is exactly the
  code rows; a copied selection contains no `@@` line.
- Split draws two columns aligned row for row: old numbers left, new right;
  a deletion only left, an addition only right, context on both; a paired
  change shares a line; each column's viewport scrolls its own long line;
  a multi-file split call renders unified.
- The header toggle sits at the right edge, switches both ways, and the
  settings file holds the last pick; a restarted view opens in that mode, and
  a repo switch keeps it.
- Wrap is a second header button: the wrapped body fits its pane in both
  layouts, a long line wraps, a split row takes the taller side's height, and
  a trailing filler keeps its row. Both choices are persisted and restored
  (`diff_wrap_probe`, `diff_mode_probe`).
- Copy from either split column gives only that side's code
  (`diff_split_probe`); unified selection still passes `diff_view_probe`.
- `cargo fmt --all --check`, the clippy selector for the touched crates, and
  `cargo test -p warpui-kit-diff -p warp-station-core -p warp-station
  -p warp-agent` are green.
- `examples/diff_demo` shows unified and split; the diff rules live in
  `docs/agents/design-system.md` and the key in
  `docs/agents/settings-schema.md`; hidden-window captures of both modes are
  checked; `kits/README.md` lists `render_diff_with`.

Whole plan:

- Every row kind draws its code on the same x within its column; in the
  unwrapped body no row grows past one line; the gutter and content washes
  are visibly different.
- Loading, binary, empty and truncated are drawn states; `n`/`p` land on
  hunk starts when phase 3 lands.
- The 2k-line fixture renders and scrolls without rebuilding every row, and
  `changes_probe` still passes.
- When phases 2, 3 and 6 also land, delete this plan or fold what is still
  useful into `docs/agents/`.
