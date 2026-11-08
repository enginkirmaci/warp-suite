# Plan: a diff view instead of a diff renderer

Status: phases 0–1 and 5 landed (the document, the unified view, the states,
full drag selection and copy). Phase 2 (virtualization and the pinned number
column), phases 3–4 and 6 are open. Decisions D1–D6 are settled (see §5).

## 1. The problem

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
- highlight runs per side, not per joined buffer (today a hunk's deletions and
  additions feed one tree-sitter pass, so one side's brace or quote can
  miscolor the other).

Parsing stays pure — no `Window`, no `App` — so patch fixtures can test it
headlessly. `render_diff` remains as a thin one-shot wrapper for callers that
only want a block, so the migration does not break warp-agent.

### 2.2 The view

The app owns a small `DiffViewState` (mode, folded hunks, focus/selection) and
keeps the parsed document for the selected patch, keyed by patch text.
Switching files reuses a cached document; a store notify does not reparse.

Anatomy:

```text
path/to/file.rs   M   +42 −17        [Unified | Split]  copy  ···
────────────────────────────────────────────────────────────────────
  @@ +533,9 @@ fn flatten_pairs                     ⌄
  533      next_ok && prev_ok
  534  }
       −   /// Emit the runs of an emphasis forest:
  541  +   /// Emit the runs of an emphasis forest:
```

- Header: path, status letter, counts, copy path, view toggle, overflow.
- One line number, and it is always a new-file number: context and additions
  show it, deletions show none. An old-file number on a deletion points at a
  line the file no longer has; the − marker and the wash carry the deletion.
- A 3px change bar at the left edge, a strong gutter wash and a quiet content
  wash; code keeps its tree-sitter colors, red/green live in the bar, the
  marker and the emphasis. Intra-line emphasis marks the changed span on
  paired lines.
- No wrap: one row is one line. The code body scrolls horizontally; this is
  also what makes rows regular enough to virtualize. Pinning the number
  column is the follow-up once the virtualized list lands — GPUI's div scroll
  shares one offset across both axes, so the first pass scrolls the row as a
  whole.
- Hunk strips are quiet rows with the range, function context and a fold
  chevron; folding removes that hunk's rows. Expanding extra context is out of
  scope until the patch is requested with more than `-U3` — a fetch change,
  not a renderer change.
- Real states for loading, empty/binary, truncated and failed patches, in the
  same visual language as the clean-tree state.

### 2.3 Interaction

- `n` / `p` and header buttons move to the next/previous hunk.
- Clicking a hunk strip folds it; fold state lasts while the file stays
  selected.
- Clicking a line number copies that line; no drag-selection across rows in
  v1.
- Nested scroll: the history detail's expanded patch and the pane's own
  scroller must not fight. The existing wheel chaining stays and gets a probe.

## 3. Non-goals

- No editing, staging or inline review; Changes stays a read-only view.
- No syntax-aware folding (functions, imports); hunk folding only.
- No context expansion in v1 (needs a `-U` fetch).
- No web view, no second renderer, no per-app colors.

## 4. Phases

0. **Fixtures first.** Add patch fixtures for a hunk mid-file, an untracked
   pure-add, a pure-delete, a rename, a binary, a truncated patch, CRLF, a
   500-character line, and a patch without a trailing newline. Assert the
   parser on these, not the pixels.
1. **Document + unified view (the look) — landed.** The marker column is
   consumed for every kind, rows do not wrap, the body scrolls horizontally,
   metadata takes no rows, the wash is two-tone, paired lines get word-level
   emphasis, and hunk strips and states are drawn. Phase 1 scrolls the row as
   a whole: GPUI's div scroll shares one offset across both axes, so the
   pinned number column waits for the virtualized list.
2. **Cache + virtualize.** The kit already caches the parsed document inside
   `render_diff`; the app-side document and the row list are what is left.
   Keep the parsed document in `ChangesView`; render rows through
   `uniform_list` with the header outside it. Rows need one common height
   (code and hunk strips) for that; if the strips must be taller, use `list`
   instead. Target: a 2k-line patch scrolls without rebuilding the tree, and
   the number column can be pinned.
3. **Header and navigation.** Counts, copy path, fold, next/prev, and the
   empty/truncated states wired to the store.
   A horizontal-scroll affordance belongs here too: the body scrolls, but
   nothing on screen says so.
4. **Split.** The `Unified | Split` toggle, row pairing within a hunk,
   per-side emphasis and the width floor. Off by default until probes and a
   capture pass.
5. **Selection (D6) — landed.** The body is one `SelectableText` over the whole
   patch, so a drag crosses lines and copy gives the code. The gutter is
   drawn beside that text, not inside it: copying a selection never carries
   line numbers or +/− markers. Hunk headers, file headers and state lines
   are lines in the same text. The element subscribes to its selection handle
   and notifies the current view, so the highlight follows the drag instead
   of waiting for the next unrelated repaint. It also maps the drag's two
   endpoints with `TextLayout::index_for_position` and supplies copy through
   a callback: the window selection's own projection walks every character of
   the run, which took ~800 ms per paint on a 900-line patch. The same drag
   now paints in ~10 ms in the debug probe.
6. **Consumers.** Point the history detail and warp-agent's tile at the same
   document and renderer; the tile keeps single numbers and no header.

## 5. Decisions

D1. **Split default — settled: unified first, split opt-in.** The owner takes
the Zed-style unified view first; split stays behind the toggle. At the default
1500px window the pane is ~1180px, so a split leaves ~510px per side —
roughly 60 characters at 14px — and only pays off on a wide or maximized
window.

D2. **Long lines — settled: horizontal scroll, no wrap.** Scroll preserves
code shape and regular rows. The gutter pin follows with the virtualized list.

D3. **Diff size — settled: `font_control` (14).** Reading code beats matching
the path/hash step; the shared text size still scales it.

D4. **Numbers — settled: new-file numbers only.** Context and additions carry
the new number; deletions carry none, because their old-file number points at
a line the file no longer has. The − marker and the wash carry the deletion.
Split mode, when it lands, shows each side's own number on its own side.

D5. **Keys — settled: `n`/`p` in the pane plus clickable hunk strips.**
Buttons stay as the discoverable version of the same actions.

D6. **Text selection — settled: full drag selection and copy.** The body is
one selection participant over one `StyledText` covering every line, with the
gutter and row washes as a layer beside/behind it. A headless drag across
lines copies the code without the gutter. `warpui-kit-diff`'s
`SelectableText` is the adapted element; the app must mount
`gpui_kit::base::TextSelectionLayer` (Warp Station and warp-agent do).

## 6. Risks

- **Kit API — done.** `render_diff` kept its signature and is now a thin
  wrapper around the document, so every caller moved without an edit. Do not
  fork the renderer.
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

- Every row kind draws its code on the same x; no row grows past one line;
  the gutter and content washes are visibly different.
- Folding removes exactly a hunk's rows; `n`/`p` land on hunk starts; loading,
  binary, empty and truncated are drawn states.
- A drag across code rows copies the code without gutter numbers or markers
  (the `diff_view_probe` selection test).
- The 2k-line fixture renders and scrolls without rebuilding every row, and
  `changes_probe` still passes.
- `cargo fmt --all --check`, the clippy selector for the touched crates, and
  `cargo test -p warpui-kit-diff -p warp-station -p warp-agent` are green.
- `examples/diff_demo` shows unified and split; the diff rules live in
  `docs/agents/design-system.md`; this plan is deleted.
