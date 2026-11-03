# warpui-kit-diff

Zed-style unified diff and line-numbered code rendering for [gpui-kit](https://crates.io/crates/gpui-kit) applications, tree-sitter highlighted.

## Usage

```rust
use warpui_kit_diff::{render_code, render_diff, sniff_lang};

// a patch: gutter, +/- marker, two-tone washes, token colors
let patch_view = render_diff(patch_text, Some(tool_file_path), font_detail, window, cx);

// plain code: gutter + token colors, no washes
let code_view = render_code(code_text, Some("rust"), color, font_detail, window, cx);

// language sniff for tool payloads (pretty-printed JSON)
let lang = sniff_lang(tool_input);
```

Colors come from `warpui-kit-core`'s palette; token colors from the kit's installed highlight theme (`gpui_kit::component::Theme::global(cx).highlight_theme`).

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
render_diff(patch, None, theme::font_mono_meta(), window, cx);
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
