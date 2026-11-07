# The family design

What a warpui-kit app looks like and where each part comes from. Everything
lives in `warpui-kit-core`, so Warp Station, warp-agent and warp-notes draw the
same colors, the same type at the same sizes, and scale the same way. This file
is the design source of truth for the suite; the constants themselves are
`kits/warpui-kit-core/src/{palette,fonts,scale,kit}.rs`.

**The rule:** app code holds no color literal, no font family and no font size.
An app's `theme.rs` re-exports core and adds layout constants. A new color or
size goes into core, where every app gets it, and never into one app.

| Part | Module | Read this when |
| --- | --- | --- |
| Palettes | `warpui_kit_core::palette` | anything gets a fill, border or text color |
| Type | `warpui_kit_core::fonts` | anything gets a font or a size |
| UI scale | `warpui_kit_core::scale` | bootstrapping an app, or touching text size |
| Kit mapping | `warpui_kit_core::kit` | a kit component renders the wrong tone |

## Palettes

Four ramps, in `palette::RAMPS`: **Copper** (the default), **Plum**, **Pine**
and **Blue**. A `Ramp` carries six surfaces, three lines, three inks and the
accent family; entity hues (`BRANCH`, `CHANGES`, `PR`, `ISSUE`, `BEHIND`,
`AHEAD`) are fixed across ramps because they mean one thing each.

| Surface | Copper | Use |
| --- | --- | --- |
| well | `1C1A18` | fields, code and diff bodies |
| chrome | `201E1C` | title bar, rail |
| base | `252321` | window, list and detail |
| panel | `2B2926` | tiles, cards |
| raised | `34312E` | pills, hover, the selected row |
| overlay | `3A3733` | menus, dialogs, tooltips |

Lines are `line_soft` (quiet), `line` (default), `line_strong`. Text is
`text`, `text_dim`, `text_muted`, and hierarchy comes from weight and size, not
a longer grey ladder. The steps are two or three points apart, so a panel needs
a border and not just a tone.

**Read, don't store.** The palette is switchable at runtime, so drawing code
reads `palette::current_ramp()` (or the reader helpers: `surface()`, `panel()`,
`well()`, `popup()`, `text()`, `text_dim()`, `text_muted()`, `hairline()`,
`accent()`, `accent_ink()`, `on_accent()`, …), never a `const` copied into the
app. `pub use warpui_kit_core::palette::{…}` in `theme.rs` is how an app keeps
its own vocabulary. `set_ramp(key)` is the settings page's writer; the value is
saved as `palette` in `~/.warp/ui.json`.

`normalize_ramp` falls back to Copper for an unknown or missing key, and
`tests`/probes enforce that the kit's own chrome follows the ramp rather than
the published constants.

### The accent

`AccentColors` is still the struct the kit consumes, but the accent is not its
own setting anymore. `Ramp::accent_colors()` maps the live palette onto it, so a
palette switch moves surfaces and accent together; warp-agent reads it that way,
and its Accent picker and the preset normalization behind it are gone from the
app.

warp-notes and warp-station still pass a fixed family — `for_preset("blue")` and
`default_colors()` (copper) — so their accent does not follow the ramp yet.
`ACCENT_PRESETS` (eight families) stays in core for those fixed paths and for
the palette probes; do not add another picker on top of it.

The rules that are easy to get wrong:

- **`ACCENT` is a fill first.** It takes `.bg()`, `.border_color()` and
  `tint(ACCENT)`, with `ON_ACCENT` on it. Text, links, active pills and small
  glyphs take `ACCENT_INK`, or `AccentColors::text` where an app still passes a
  fixed family. The kit's `link` and `ring` already ride `accent.text`.
- **Text on a fill has its own token.** `ON_ACCENT` is near-black on `ACCENT`.
  `ON_DANGER` is dark on `DANGER`: white on that soft red is too faint.
- **A panel needs an edge.** A surface one or two steps off its parent needs
  `.border_1()` with the hairline color, or `border_light()` when something
  inside it has one already. warp-agent's main panel vanished into the chrome
  without it.
- **Two hovers.** `HOVER` is a translucent cool wash that works on any surface
  and is what the kit's `accent` token maps to, so `theme.accent` is not a
  brand color. `HOVER_BG` (the live `raised()` step) is opaque, for list rows on
  the canvas.
- **`tint(color)`** is the 18% wash behind chips and destructive hovers.
  `SCRIM` is the dim layer under dialogs and sheets.

Contrast is a test, not a habit: `palette::tests` holds every ink at AA (4.5:1)
up to the overlay step, filled actions, and each preset's `text`. A new ink goes
into that list. It is why `TEXT_FAINT` shares `text_muted()` — the design
settled on three text steps, and a fourth grey was what made quiet text
unreadable.

## Type

The family is set in Inter, with JetBrains Mono for code, paths and hashes. The
faces ship in `warpui-kit-core/assets/fonts` (SIL OFL, `OFL-Inter.txt` and
`OFL-JetBrainsMono.txt` beside them). Core registers them the first time
`apply_kit_theme` runs, so apps load nothing and keep no font files. `ui_font()`
and `code_font()` return what resolved.

**gpui does not slant on Linux.** A family with no italic face draws `.italic()`
upright, silently, in the live editor too. Using a new weight in italics means
adding its italic face to `assets/fonts`.

There is one scale with a floor of 12, and these are the only sizes:

| Step | px | Use |
| --- | --- | --- |
| `font_micro` | 12 | captions, avatars |
| `font_mono_meta` | 13 | mono metadata: paths, hashes, shortcuts |
| `font_row_meta`, `font_meta`, `font_sub`, `font_label` | 13 | a row's second line, hints, captions |
| `font_body`, `font_control` | 14 | rows, menus, buttons, field text |
| `font_name`, `font_brand` | 15 | a list row's name, the product name |
| `font_heading`, `font_note` | 16 | headings, a note's text |
| `font_list_title` | 20 | a list's title |
| `font_screen_title` | 22 | a page's title |

Icons: `icon_nano` 12, `icon_tiny` 14, `icon_small` 16, `icon_medium` 18,
`icon_large` 20, `icon_xl` 22.

Two ways to read a step, both on one factor:

- `theme::font_body()`: a getter, for views that read at render time (Warp
  Station, warp-notes, `pub use warpui_kit_core::fonts::*` in `theme.rs`).
- `FontTokens`, from `active_tokens()`: a snapshot passed by value (warp-agent's
  `store.fonts`, `PickerData.fonts`).

Never do arithmetic on a step (`font_body() - 1.`): pick the step that exists.
The steps are role names, so code written for the old 8 to 11.5 ladder compiles
and simply draws larger; a hand-sized control around such text needs its height
checked.

**Kit fields sit off the ladder.** The kit sizes field text in rem, and
`.small()` is still 14px, so every `Input` and `Textarea` takes
`.with_size(theme::field_size())`, or `field_size_at(px)` for another step (the
warp-agent composer uses `font_brand`). A custom `Size` sets the text at 7/8 of
its value and keeps the small control's height and padding.

**Leave the rem base alone.** `apply_kit_theme` keeps `t.font_size` at 16 × the
factor (and `mono_font_size` at 13 ×). Every `.px_2()` and `.gap_2()` in every
app is in rem, so lowering it to shrink kit text shrinks all the spacing too.
`TextView` inherits 1rem, which is why markdown reads at 16 — the notes editor
instead passes an explicit `.text_size(px(theme::font_note()))` to `LiveEditor`.

## UI scale

The factor is the text size times the desktop text scaling. Every app boots the
same way:

```rust
gpui_kit::init(cx);
warpui_kit_core::adopt_text_size(old_setting); // before the first theme pass
theme::apply_kit_theme(cx);
warpui_kit_core::follow_ui_scale(cx, theme::apply_kit_theme);
```

- **Text size** is one setting, `~/.warp/ui.json`: `small` (×0.875), `normal`,
  `large` (×1.12), `larger` (×1.25) — the exact strings are
  `fonts::FONT_SMALL`, `FONT_NORMAL`, `FONT_LARGE`, `FONT_LARGER`. Anything
  else normalizes to normal, including the older `compact`.
- **`text_size_control()`** is the one picker, a trough with Small / Normal /
  Large / Larger. Put it in the app's settings; every current app has a
  Settings section for it (`warp-notes` keeps it in `ui/settings.rs` under
  Appearance). A click applies at once, so it needs no buffer, field or Save
  path.
- **Other running apps do not follow live.** They read the file at their next
  start. That is the owner's decision: do not add a file watcher.
- **`adopt_text_size(legacy)`** seeds the file once from an app's old setting.
  A new app passes `None`. Never call `set_font_scale` from an app, and never
  add a per-app text size.
- **`follow_ui_scale`'s callback** re-runs the kit theme. An app holding a
  `FontTokens` snapshot re-reads `active_tokens()` there too
  (`Store::rescale_fonts` in warp-agent).
- **The desktop factor** belongs to core: a `gsettings monitor` child killed
  from `on_app_quit`, polled from the main thread because this gpui rev does not
  wake foreground tasks from other threads. Only Linux with gsettings gets it,
  and elsewhere the desktop factor stays 1.0.

`~/.warp/ui.json` holds `text_size` and `palette` as separate keys. Both are
additive: a file that lacks one keeps the other, and a writer must not drop the
key it does not own.

## Controls

The shapes the family repeats. Copy the numbers, since they come from the
mockup.

- **Radius:** controls 6, panels 8 (`t.radius`, `t.radius_lg`).
- **Card:** `panel()`, `.border_1()`, hairline, `.rounded_lg()`.
- **Popover:** `popup()`, hairline, `.rounded_lg()`, `.shadow_lg()`
  (`popover_card` in warp-agent).
- **Primary action:** `.bg(ACCENT)`, `.text_color(ON_ACCENT)`, hover
  `ACCENT_HOVER`. One to a surface.
- **Ghost action:** `GHOST_BG`, `GHOST_BORDER`, hover `GHOST_BORDER_HOVER`.
- **Segmented control:** a trough 34 to 36 high, padding 3, radius 8, `SEG_BG`
  (plus `SEG_BORDER` on the shared picker); segments 28 to 30 high, radius 6,
  `font_row_meta`, active `SEG_ACTIVE` with `SEG_ACTIVE_FG`, the rest
  `TEXT_MUTED` and `TEXT` on hover. `text_size_control` is the 34/28 one
  (`scale.rs`), and Warp Station's 36/30 `segment_row`/`segment` in
  `ui/repos.rs` is the list's.
- **Keycap:** `KEY_BG`, `KEY_BORDER`, `KEY_FG`, `font_mono_meta`.
- **Search field:** `SEARCH_BG`, `SEARCH_BORDER`.
- **Accessibility:** every hand-rolled control goes through `A11y`
  (`.a11y_button`, `.a11y_toggle`, `.a11y_tab`), plus a `debug_selector` so a
  probe can find it.

## Where a change goes

- A new color or tone: `palette.rs` — add it to `Ramp` (with a value in all four
  presets) if it is part of the surface system, or to the fixed semantic block
  if it means one thing. Update the contrast test.
- A new size: `fonts.rs`, and only if no existing step fits.
- A new shared shape: `warpui-kit-core` (see `kits/README.md`), not an app.
