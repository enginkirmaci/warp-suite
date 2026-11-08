# warpui-kit-diff

Zed-style unified diff and line-numbered code rendering for [gpui-kit](https://crates.io/crates/gpui-kit) applications, tree-sitter highlighted.

## Usage

```rust
use warpui_kit_diff::{render_code, render_diff, sniff_lang, DiffDocument};

// a patch: gutter, +/- marker, two-tone washes, token colors
let patch_view = render_diff(patch_text, Some(tool_file_path), font_detail, window, cx);

// the same drawing from an already parsed patch
let doc = DiffDocument::parse(patch_text);      // pure, unit-testable
let view = render_document(&doc, Some(path), font_detail, window, cx);

// plain code: gutter + token colors, no washes
let code_view = render_code(code_text, Some("rust"), color, font_detail, window, cx);

// language sniff for tool payloads (pretty-printed JSON)
let lang = sniff_lang(tool_input);
```

Colors come from `warpui-kit-core`'s palette; token colors from the kit's installed highlight theme (`gpui_kit::component::Theme::global(cx).highlight_theme`).

## Layout

- One line is one row: rows never wrap, and the body scrolls horizontally.
  The number column scrolls with the row until the virtualized list lands;
  GPUI's div scroll shares one offset across both axes and has no sticky
  position.
- The patch's marker column is consumed for context, additions and deletions,
  so all three start their text on the same x. Hunk strips and state rows use
  an empty gutter cell to land on that column.
- One number, and it is a new-file number: context and additions show it;
  deletions show none (their old number is not a line in the file).
- The gutter takes the strong wash (`DIFF_*_GUTTER_BG`), the code the quiet
  one (`DIFF_*_BG`), and a 3px bar in `ADD`/`DEL` runs down the row's edge.
- Paired deletions/additions get a word-level emphasis (`DIFF_*_EMPHASIS`);
  token colors are never recolored.
- `diff --git`, `index`, `---` and `+++` are hidden. A multi-file patch gets
  a file header row; the app's own header carries the path and counts for a
  single file.
- `DiffDocument` is pure: no `Window`, no `App`, no theme. `render_diff`
  parses and caches by patch text; callers with their own view state (folds,
  scroll) should hold the document and call `render_document`.

## Selection

The diff body is one selectable text: a drag crosses lines, and Copy gives
the patch content without the gutter numbers or the +/− markers. The gutter
and the row washes are drawn beside and behind that text, so selection never
reaches them.

The selection registers with the window selection system, so the host must
mount `gpui_kit::base::TextSelectionLayer` (Warp Station and warp-agent do).
`render_code` is still a plain block; only the diff participates.

The body is two text layouts (gutter and code) and one canvas per column, not
a div per row; a large patch re-renders without laying out thousands of row
elements. It subscribes to its selection handle and notifies the current
view, so the highlight follows a drag. The shaped text is retained across
frames, and the selection's endpoints are mapped with `index_for_position`
instead of the window selection's per-character walk; a drag on a 900-line
patch paints in ~10 ms rather than ~800 ms in the debug probe.

## Text size

`font_detail` is the size the block draws at, and it is the only size that
decides it: the renderer pins the code family and that size on the block's own
root, so the rows are shaped at it no matter what text size the surrounding
view set. GPUI shapes a `StyledText` at the *ambient* text style (a `TextRun`
carries a font, never a size), so without the pin the block silently followed
the caller's container instead — the bug behind "the diff's font size does not
follow the app's font size".

Pass the family step the app's text size resolves to, read at render time, so
the block follows the setting with everything around it:

```rust
// warp-station: a getter on the live ladder
render_diff(patch, None, theme::font_control(), window, cx);
// warp-agent: the FontTokens snapshot the view already holds
render_diff(diff, Some(&path), fonts.font_meta, window, cx);
```

## Grammars

Token highlighting needs the grammar features enabled; this crate's `tree-sitter-*` features forward to gpui-kit's:

```toml
warpui-kit-diff = { path = "../warpui-kit-diff", features = [
    "tree-sitter-bash", "tree-sitter-c", "tree-sitter-cpp", "tree-sitter-css",
    "tree-sitter-go", "tree-sitter-html", "tree-sitter-javascript",
    "tree-sitter-python", "tree-sitter-rust", "tree-sitter-toml",
    "tree-sitter-typescript", "tree-sitter-tsx", "tree-sitter-yaml",
] }
```

`render_code` also exposes the markdown code-block highlighter (`shared_highlighter`) re-hosted on the kit's public `SyntaxHighlighter` API, for hosts that need per-instance override hooks.

## Dependencies

GPUI comes through gpui-kit; the matching `gpui-pre` snapshot rev is pinned there — bump gpui-kit and gpui-pre together.

## License

MIT
