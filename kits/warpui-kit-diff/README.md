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
