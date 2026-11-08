# The family design

What a warpui-kit app looks like and where each part comes from. Everything
lives in `warpui-kit-core`, so the three GPUI apps — Warp Station, warp-agent
and warp-notes — draw the same colors, the same type at the same sizes, and
scale the same way. (`apps/warp-snapit` is the exception: a Windows-only Win32
binary that does not link the kit.) This file is the design source of truth for
the suite; the constants themselves are
`kits/warpui-kit-core/src/{palette,fonts,scale,kit}.rs`.

**The rule:** app code holds no color literal, no font family and no font size.
An app's `theme.rs` re-exports core and adds layout constants. A new color or
size goes into core, where every app gets it, and never into one app.

**Current state:** all three GPUI apps run the shared `bootstrap::init`, carry
the text-size and palette pickers, and have a per-app Reduce motion switch. A
palette click moves every surface in all three. The accent family follows the
palette in warp-agent only; warp-notes and warp-station still pin Copper (see
[The accent](#the-accent)).

| Part | Module | Read this when |
| --- | --- | --- |
| Palettes | `warpui_kit_core::palette` | anything gets a fill, border or text color |
| Type | `warpui_kit_core::fonts` | anything gets a font or a size |
| UI scale | `warpui_kit_core::scale` | the shared text size or desktop scaling |
| Startup | `warpui_kit_core::bootstrap` | wiring a new app's first frame |
| Motion | `warpui_kit_core::motion` | an entrance, spinner, or reduced motion |
| Settings | `warpui_kit_core::settings` | a settings page needs a row, switch or segment |
| Controls | `warpui_kit_core::{controls,field}` | a button, menu row, popover, field or keycap |
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
a longer grey ladder. Surface steps run four to eleven points apart, so a panel
needs a border and not just a tone.

**Read, don't store.** The palette is switchable at runtime, so drawing code
reads `palette::current_ramp()` (or the reader helpers: `surface()`,
`window_chrome()`, `panel()`, `raised()`, `popup()`, `well()`, `border_light()`,
`hairline()`, `line_strong()`, `text()`, `text_dim()`, `text_muted()`,
`text_faint()`, `accent()`, `accent_hover()`, `accent_ink()`, `accent_soft()`,
`on_accent()`, `input()`, `chip()`, `hover_bg()`, `selected()`), never a `const`
copied into the app. `pub use warpui_kit_core::palette::{…}` in `theme.rs` is
how an app keeps its own vocabulary.

`palette_control()` is the one picker, shown in every app's Appearance settings.
A click calls `set_palette(key, cx)`: it switches the live ramp, writes
`palette` to `~/.warp/ui.json`, and runs the host installed by
`follow_palette`, so the app re-maps the kit theme and repaints. The other
running apps read the file at their next start. `adopt_palette(legacy)` settles
the value at startup (the bootstrap calls it); `set_ramp` is the low-level
switch the probes use.

`normalize_ramp` falls back to Copper for an unknown or missing key. The
published `SURFACE`, `TEXT`, `ACCENT`, `GHOST_BG`, `SEG_BG`, … constants remain
as the Copper reference (and for fixed-accent paths), but they are not what the
live UI draws with: `palette_follows_probe` source-scans the kit's drawing code
so a fixed ramp constant cannot creep back into `controls`, `shell`, `scale`,
`kit` or the picker.

### The accent

`AccentColors` is still the struct the kit consumes, but the accent is not its
own setting anymore. `Ramp::accent_colors()` maps the live palette onto it, so a
palette switch moves surfaces and accent together.

warp-agent reads it that way: its Accent picker and the preset normalization
behind it are gone, and its store re-derives the accent with the fonts whenever
the palette re-maps the kit theme. warp-notes and warp-station still pass a
fixed family, so their accent does not follow the ramp yet: warp-station passes
`AccentColors::default_colors()` (Copper), and warp-notes passes
`for_preset(normalize_accent(Some("blue")))`, which now normalizes to Copper too
— the eight `ACCENT_PRESETS` have no blue. That is the remaining step of the
palette refactor; do not add a second accent picker on top of them.

The rules that are easy to get wrong:

- **`accent()` is a fill first.** It takes `.bg()`, `.border_color()` and
  `tint(accent())`, with `on_accent()` on it. Text, links, active pills and
  small glyphs take `accent_ink()`, or `AccentColors::text` on the app's accent
  family. The kit's `link` and `ring` already ride `accent.text`.
- **Text on a fill has its own token.** `on_accent()` is near-black on the
  accent fill. `ON_DANGER` is dark on `DANGER`: white on that soft red is too
  faint.
- **A panel needs an edge.** A surface one or two steps off its parent needs
  `.border_1()` with `hairline()`, or `border_light()` when something inside it
  has one already. warp-agent's main panel vanished into the chrome without it.
- **Two hovers.** `HOVER` is a translucent cool wash that works on any surface
  and is what the kit's `Theme.accent` token maps to, so the kit's "accent" is
  not the brand accent. `hover_bg()` (the live `raised()` step) is opaque, for
  list rows on the canvas; `HOVER_BG` is the Copper value of that step.
- **`tint(color)`** is the 18% wash behind chips and destructive hovers.
  `SCRIM` is the dim layer under dialogs and sheets.

Contrast is a test, not a habit: `palette::tests` holds every ink at AA (4.5:1)
up to the overlay step, filled actions, and each preset's `text`. A new ink goes
into that list. It is why `text_faint()` returns the same step as
`text_muted()` — the design settled on three text steps, and a fourth grey was
what made quiet text unreadable.

## Type

The family is set in Inter, with JetBrains Mono for code, paths and hashes. The
faces ship in `warpui-kit-core/assets/fonts` (SIL OFL, `OFL-Inter.txt` and
`OFL-JetBrainsMono.txt` beside them). Core registers them the first time
`apply_kit_theme` runs, so apps load nothing and keep no font files. `ui_font()`
and `code_font()` return what resolved.

**gpui does not slant on Linux.** A family with no italic face draws `.italic()`
upright, silently, in the live editor too. The bundled set includes italics for
the weights that ship with one; using a new weight in italics means adding its
italic face to `assets/fonts`.

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
`icon_large` 20, `icon_xl` 22. `icon_nano` and `icon_xl` exist only on the
`FontTokens` snapshot (there are no getters); the other four have both.

Two ways to read a step, both on one factor:

- `theme::font_body()`: a getter, for views that read at render time (Warp
  Station, warp-notes, `pub use warpui_kit_core::fonts::*` in `theme.rs`).
- `FontTokens`, from `active_tokens()`: a snapshot passed by value (warp-agent's
  `store.fonts`, `PickerData.fonts`).

Never do arithmetic on a step (`font_body() - 1.`): pick the step that exists.
The steps are role names, so code written for the old 8 to 11.5 ladder compiles
and simply draws larger; a hand-sized control around such text needs its height
checked.

**Fields go through `warpui_kit_core::field`.** `field::text` and `mono` build
the complete box and seat the kit input at `field_size()` (the body step);
`area` and `mono_area` build the multi-line box on the XSmall control with body
or mono text. `field_size_at(px)` is for another step. A raw `Input` takes
`.with_size(field_size())`; a raw `Textarea` sets its own text size (see
`bare_area`). A custom `Size` sets the text at 7/8 of its value and keeps the
small control's height and padding. The warp-agent composer seats
`field::bare_area` (body text) inside its own frame.

**Leave the rem base alone.** `apply_kit_theme` keeps `t.font_size` at 16 × the
factor (and `mono_font_size` at 13 ×). Every `.px_2()` and `.gap_2()` in every
app is in rem, so lowering it to shrink kit text shrinks all the spacing too.
`TextView` inherits 1rem, which is why markdown reads at 16 — the notes editor
instead passes an explicit `.text_size(px(theme::font_note()))` to `LiveEditor`.

## UI scale

The factor is the text size times the desktop text scaling. The bootstrap runs
the whole sequence for an app (see [Startup](#startup)); this section is what the
pieces mean.

- **Text size** is one setting, `~/.warp/ui.json`: `small` (×0.875), `normal`,
  `large` (×1.12), `larger` (×1.25) — the exact strings are
  `fonts::FONT_SMALL`, `FONT_NORMAL`, `FONT_LARGE`, `FONT_LARGER`. Anything
  else normalizes to normal, including the older `compact`.
- **`text_size_control()`** is the one picker: a trough with Small / Normal /
  Large / Larger. A click applies at once, so it needs no buffer, field or Save
  path. Put it in Appearance (warp-notes keeps it under General in
  `ui/settings.rs`; Warp Station and warp-agent put it under Appearance). The
  palette picker sits beside it.
- **Other running apps do not follow live.** They read the file at their next
  start. That is the owner's decision: do not add a file watcher.
- **`adopt_text_size(legacy)`** seeds the file once from an app's old setting.
  A new app passes `None`. Never call `set_font_scale` from an app, and never
  add a per-app text size.
- **`follow_ui_scale`'s callback** re-runs the kit theme. An app holding a
  `FontTokens` snapshot re-reads `active_tokens()` there too (warp-agent's
  retheme in `main.rs` refreshes both `store.fonts` and `store.accent`).
- **The desktop factor** belongs to core: a `gsettings monitor` child killed
  from `on_app_quit`, polled from the main thread because this gpui rev does not
  wake foreground tasks from other threads. Only Linux with gsettings gets it,
  and elsewhere the desktop factor stays 1.0.

`~/.warp/ui.json` holds `text_size` and `palette` as separate keys. Both are
additive: a file that lacks one keeps the other, and a writer must not drop the
key it does not own. Writes take a sidecar lock and go through an atomic
rename, so two apps saving at once do not lose each other's key. Reduced motion
is **not** in this file; each app stores its own switch.

## Startup

Every GPUI app runs one sequence before its first window, from
`warpui_kit_core::bootstrap::init(cx, legacy_text_size, reduce_motion, retheme)`:

1. `gpui_kit::init(cx)`.
2. `adopt_text_size(legacy_text_size)` and `adopt_palette(None)`, both reading
   the shared `~/.warp/ui.json`.
3. `follow_palette`: installs the hook that re-runs `retheme` and refreshes
   every window after a switch.
4. `retheme(cx)` once before anything draws; this is the app's
   `theme::apply_kit_theme`.
5. `follow_ui_scale`: reads the desktop factor (re-running `retheme` when it is
   not 1.0) and installs the gsettings monitor.
6. `motion::set_reduced(reduce_motion || motion::system_prefers_reduced())`.

`legacy_text_size` is the app's old `font_scale` setting (`Option<&str>`); it
seeds the shared file on the first run and is ignored after that. A new app
passes `None, false`. Bind app keybindings *after* this call, so they win ties
against the bindings `gpui_kit::init` installs. An app that keeps a
`FontTokens` snapshot or an `AccentColors` in its store re-derives them inside
`retheme` before re-mapping the kit; warp-agent does both in `main.rs`, the
other two just call `theme::apply_kit_theme`.

## Motion

Motion is short, and only ever opacity or a transform. It says that something
changed or that work is running; it never decorates. `motion.rs` owns the
durations: `FAST` 120 ms (hover and press), `BASE` 180 ms (entrances, switches,
toasts, dialogs), `SPIN` 800 ms, `PROGRESS` 1200 ms.

- `spinner(id, path, size, color)` turns a glyph; always pair it with a word.
- `enter(id, element)`, `pop(id, glyph)`, `progress_line(id, color)` and
  `grow_mark(id, color)` cover the family's entrances and running marks.
- `set_reduced(on)` turns all of it off, spinners included; the busy words stay,
  so nothing depends on movement. Call `cx.refresh_windows()` after a runtime
  change.
- The bootstrap seeds `reduced || system_prefers_reduced()` (gsettings
  `enable-animations` on Linux, `MinAnimate` on Windows). Each GPUI app has a
  Reduce motion switch in Appearance and stores it in its own settings.

## Controls

The shapes the family repeats. Copy the numbers, since they come from the
mockup, and use the shared helper where one exists.

- **Radius:** controls 6, panels 8 (`t.radius`, `t.radius_lg`).
- **Card:** `panel()`, `.border_1()`, `hairline()`, `.rounded_lg()`.
- **Popover:** `controls::popover_card()` — `popup()`, `line_strong()` edge,
  8px radius, `.shadow_lg()`. warp-agent uses it; Warp Station still draws its
  own menu chrome.
- **Primary action:** one to a surface. Fill with `.bg(accent())` and
  `.text_color(on_accent())`; Warp Station's primary paths hover
  `accent_hover()`, while warp-notes' primary button and warp-agent's shared
  `controls::button(…, BtnKind::Primary(accent), …)` dim to 0.9.
- **Ghost action:** `ghost_bg()` / `ghost_border()` (station) read the live
  ramp; `GHOST_BORDER_HOVER` is still a Copper constant (station pins Copper).
- **Segmented control:** a trough 34 to 36 high, padding 3, radius 8, on
  `well()` (the shared pickers add a `hairline()` or `line()` edge); segments 28
  to 30 high, radius 6, `font_row_meta`, active on `raised()` with `text()`, the
  rest `text_muted()`/`text_dim()` and `text()` on hover. `settings::segmented`
  and `text_size_control`/`palette_control` are the 34/28 ones; Warp Station's
  36/30 `segment_row`/`segment` in `ui/repos.rs` is the list's.
- **Settings rows:** `settings::{row, row_with_desc, stack_row, switch_row,
  switch_row_with_desc, subhead, rows}` — a 60-high row, label and optional
  description left, control right, `border_light()` rule between rows, no card
  around them. Switches use the kit's 36×20 `Switch`.
- **Keycap:** `field::kbd()` — 20 high, 4px radius, `line_strong()` edge,
  `font_mono_meta`; station's draw call is the same shape.
- **Search field:** `search_bg()` is `well()`, `search_border()` is
  `hairline()`; the `SEARCH_*` constants are the Copper reference.
- **Accessibility:** every hand-rolled control goes through `A11y`
  (`.a11y_button`, `.a11y_toggle`, `.a11y_tab`), plus a `debug_selector` so a
  probe can find it.

Adoption of the shared helpers is partial: `field`, `settings` and `motion` are
used by all three apps, while `controls::{button, menu_row, popover_card}` are
warp-agent's so far and notes/station still draw some of those shapes locally.
Use the core helper for new code rather than adding another copy.

## Where a change goes

- A new color or tone: `palette.rs` — add it to `Ramp` (with a value in all four
  presets) and a reader if it is part of the live surface system, or to the
  fixed semantic block if it means one thing. Update the contrast test.
- A new size: `fonts.rs`, and only if no existing step fits.
- A new shared shape: `warpui-kit-core` (see `kits/README.md`), not an app —
  `controls.rs` for a button/menu, `field.rs` for a field, `settings.rs` for a
  settings row, `motion.rs` for anything that moves.
- Startup, text size or palette plumbing: `bootstrap.rs` / `scale.rs`.
- App-only layout constants: the app's `theme.rs`.
