# warpui-kit-core

Theme palette, bundled fonts, type scale and shared control helpers for the **warpui-kit** family — the foundation the other family crates build on.

The palette is Copper, the warp-suite-v3 design: one lifted warm ramp plus one hue per entity, shared by the family's desktop clients; `apply_kit_theme` maps it onto [gpui-kit](https://crates.io/crates/gpui-kit)'s shadcn-style theme tokens (with the two-pass mode-flip that survives the kit's `Theme::update` reload).

## What's inside

| Module | Contents |
| --- | --- |
| `palette` | Surface/text ramps, accent presets, entity hues, diff washes, syntax highlight theme |
| `fonts` | The bundled Inter and JetBrains Mono faces, font resolution, the type scale (`FontTokens` plus one getter per step), kit field sizes |
| `scale` | The shared text size (`~/.warp/ui.json`), its `text_size_control()`, and `follow_ui_scale(cx, on_change)` for live changes |
| `shell` | The one window shape: `title_bar`, `rail`, `tab_strip`, `list_pane`, `detail_pane` and their tabs and rows |
| `motion` | Durations, easing and the `spinner` |
| `kit` | `apply_kit_theme(accent, fonts, sidebar, cx)` |
| `a11y` | The `A11y` role/label trait for hand-rolled controls |
| `icon` | Tinted svg icon builder |
| `tooltip` | Attribute-based tooltips (`with_tooltip`) |
| `tooltip_overlay` | Overlay-based tooltips (`TooltipHost` + `tip`) |

## Usage

```rust
use warpui_kit_core::{apply_kit_theme, font_tokens, normalize_accent, AccentColors, BACKGROUND, WINDOW_CHROME};

fn apply_theme(cx: &mut gpui_kit::App) {
    apply_kit_theme(
        AccentColors::for_preset(normalize_accent(None)),
        &font_tokens(None, 1.0),
        WINDOW_CHROME, // or BACKGROUND — the sidebar step is the caller's call
        cx,
    );
}
```

Every app scales the same way. Settle the text size before the first theme pass, then follow it; the callback re-runs the mapping, which refreshes every window:

```rust
warpui_kit_core::adopt_text_size(old_app_setting); // seeds ~/.warp/ui.json once
apply_theme(cx);
warpui_kit_core::follow_ui_scale(cx, apply_theme);
```

Put `text_size_control()` wherever the app shows its settings. A click applies at once and saves to `~/.warp/ui.json`; the other apps read it when they next start. The desktop text scaling is followed live. Both feed `font_factor()`, so `active_tokens()` and the getters (`font_body()`, `icon_small()`, …) stay on one ladder. Kit fields size their text in rem, off the ladder, so pass `.with_size(fonts::field_size())` to an `Input` or `Textarea`.

The faces register themselves the first time the mapping runs; nothing to load from the app. They are Inter and JetBrains Mono under the SIL Open Font License, see `assets/fonts/OFL-*.txt`.

## Family crates

- `warpui-kit-diff` — Zed-style diff and code renderer (tree-sitter)
- `warpui-kit-markdown` — live markdown parser, editor highlighter and formatting ops
- `warpui-kit-picker` — filtered select-style picker

## Dependencies

GPUI comes through gpui-kit; the matching `gpui-pre` snapshot rev is pinned there — bump gpui-kit and gpui-pre together.

## License

MIT, except the bundled fonts in `assets/fonts` (SIL OFL 1.1).
