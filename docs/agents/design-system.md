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
palette click moves every surface and the accent family in all three (see
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
| Text menu | `warpui_kit_core::text_menu` | a selectable read-only surface needs a right-click Copy |
| Text surfaces | `warpui-kit-markdown`, `text_menu` | text should be a document or a label |
| Keyboard | the app's list pane (notes' tree is the reference) | arrows, focus or a list cursor |
| Kit mapping | `warpui_kit_core::kit` | a kit component renders the wrong tone |

## Palettes

Five ramps, in `palette::RAMPS`: **Copper** (the default), **Plum**, **Pine**,
**Blue** and **Gray**. A `Ramp` carries six surfaces, three lines, three inks
and the accent family; entity hues (`BRANCH`, `CHANGES`, `PR`, `ISSUE`,
`BEHIND`, `AHEAD`) are fixed across ramps because they mean one thing each.

Blue is the cool slate field; Gray is the neutral graphite field with a muted
steel accent, for screens that should carry almost no colour cast. Gray's
inks and primary label clear AA on every surface (text 11.9:1, text-2 8.5:1,
text-3 5.6:1, accent-ink 7.7:1, primary label 7.0:1), and
`palette::tests::every_palette_clears_aa_and_orders_its_ramp` measures every
ramp, so a new palette cannot ship unmeasured.

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
the palette re-maps the kit theme. warp-notes and warp-station read
`current_ramp().accent_colors()` in their retheme, so a palette switch carries
their primary controls too. Do not add a second accent picker on top of any
app.

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

Running text in a panel — the README card and the daily summary — is the 15
step (`font_name`) at a 25 px line, not `font_body`; the note editor keeps
`font_note` 16.

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

## Diffs

`warpui-kit-diff` draws every diff: the Changes pane, the history detail and
warp-agent's tool tiles. The rules:

- **One line is one row by default.** No wrapping; a long line keeps its
  shape and the body scrolls horizontally. Word wrap is an opt-in, described
  below.
- **One code column per side.** The patch's marker column is consumed, not
  rendered: context, additions and deletions all start their text on the same
  x. In split the same rule holds inside each column, and one shared gutter
  width puts both code columns on the same x.
- **One number per side, and it is a line in the file.** Unified shows the
  new-file number on context and additions and none on deletions — their
  old-file number points at a line the file no longer has. Split shows the
  old-file number in the left column and the new-file number in the right;
  the −/+ marker and the wash carry the kind.
- **Two washes, not one band.** The gutter carries the strong wash
  (`DIFF_*_GUTTER_BG`), the code the quiet one (`DIFF_*_BG`), and a 3px bar
  in `ADD`/`DEL` runs down the row's leading edge. Token colors are never
  recolored.
- **Emphasis is the changed word.** Paired deletions/additions get a word
  diff; the changed span takes `DIFF_*_EMPHASIS`. A span covering most of the
  line is dropped — the row wash already says it.
- **Metadata and hunk headers are not rows.** `diff --git`, `index`, `---`
  and `+++` are hidden; the path, status and counts belong to the view's
  header. The `@@ … @@` header is parsed but never drawn, so the `-U3`
  context is the only gap between hunks and a copied selection carries no
  `@@` text.
- **Split is two aligned columns.** Old file left, new file right; context
  appears on both, a deletion only left, an addition only right, and the side
  without a row gets a 45° slash-hatched filler across the whole column, so
  the gap reads as deliberately empty and the columns never drift. A 1px
  `hairline` divider separates them. The columns split the pane evenly and
  each owns its horizontal viewport and a bar in the pinned footer under the
  body, so a long line on one side never pushes the other side off screen.
  A state-only, truncated or multi-file patch stays unified.
- **The scrolled pane owns both axes.** `render_diff_with_scroll` is the
  vertical viewport; the app does not wrap it in another scroller. Both the
  vertical frame and each horizontal viewport are axis-restricted, so a
  horizontal wheel step moves the code column under the pointer and never
  the shared body. The plain `render_diff_with` stays in flow for an embedded
  block (a history detail, an agent tile) and has no vertical viewport.
- **Word wrap is a toggle.** On, each source row is an element: the code wraps
  inside its column, the number stays on the first visual line, and a split
  row takes the taller side's height so the next row starts together. Wrap
  mode selects and copies within one row; the unwrapped body keeps the one
  selection across the whole patch. In the scrolled pane the wrapped body is
  a virtualised list: only the rows near the viewport are elements, so a
  wheel tick does not rebuild thousands of rows.
- **States are rows with a word:** loading, binary, no textual change and
  truncation. They never pass through the row renderer.
- **Size:** `font_mono_meta` (13) with a 21 px line (the kit's
  `DIFF_LINE_HEIGHT`), matching DESIGN.md's `mono 13/21`. The block still
  follows the app's text size through the caller's `font_detail`.
- **The body is selectable.** Unwrapped unified has one selectable text
  covering every line; unwrapped split has one per column, so a drag stays on
  its side and copy gives that side's code. Wrapped rows are separate
  selectable texts. The gutter and washes are drawn beside and behind the
  text, so copy never carries line numbers or +/− markers. The app must mount
  `gpui_kit::base::TextSelectionLayer` or the drag never registers.
  A selection change refreshes the window rather than notifying the view; a
  wrapped body has one participant per row, and only the row that owns an
  endpoint refreshes.
- **Unwrapped is texts and canvases, not a row per div.** The gutter and the
  code are one text layout each (two of each in split); a canvas per column
  paints every row's wash. A large unwrapped diff re-renders without laying
  out thousands of row divs. Wrapped rows need a real row height, so the
  scrolled pane uses a list (one element per visible row); only the embedded
  `render_diff_with` still builds every wrapped row up front.

The washes live in `warpui-kit-core::palette`; a new diff color goes there,
never into an app.

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

`~/.warp/ui.json` holds `schema_version`, `text_size` and `palette`. Both
settings are additive: a file that lacks one keeps the other, and a writer
must not drop the key it does not own. Writes go through the platform kit's
`SharedDoc` (sidecar lock, atomic rename), so two apps saving at once do not
lose each other's key. Reduced motion is **not** in this file; each app stores
its own switch.

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

### Where it shows

Motion is a state signal, so one state shows at three levels and the levels
must agree:

1. **The object.** The row that is working animates: a repo row spins and
   names its git job, a chat row spins, a running tool's title line carries the
   spinner and its word. A row that is only *near* running work does not.
2. **Its head.** The pane or tile says the same thing in its title row — the
   same spinner and word where the head has room (the chat head's `Running`), a
   list head's `Loading…`, the detail head's progress line. The head is what
   stays visible when the row is scrolled away or the tab is in the background.
3. **The window.** When the window has work running, the title bar carries one
   activity chip for it (spinner, count, word), and the OS window title
   carries the same summary as plain text (`2 running — Warp Agent`). Both
   change on the same state change and both drop the indicator when the window
   is idle.

- **Never motion alone.** Every spinner has a word beside it, an accessible
  name on the animated element, and a tooltip naming what runs. Reduced motion
  keeps the glyph and the word; it only stops the turn.
- **The OS title states, it does not move.** The in-app head may carry the same
  spinner as its row, but the OS window title is a string: it carries the word
  and count, never a frame animation.
- **A title does not invent work.** It repeats a count the app already has
  (`Store::activity`, the tile's `running`) and shows nothing at zero.

## Controls

The shapes the family repeats. Copy the numbers, since they come from the
mockup, and use the shared helper where one exists.

- **Radius:** controls 6, panels and menus 8, dialogs 10.
- **Card:** `panel()`, `.border_1()`, `hairline()`, `.rounded_lg()`.
- **Popover:** `controls::popover_card()` — `popup()`, `line_strong()` edge,
  8px radius, `.shadow_lg()`. A menu row is 32 high inside 4 padding, 14/500,
  with its icon at 16 and any keycap right-aligned. warp-agent uses it; Warp
  Station still draws its own menu chrome.
- **Dialog:** the scrim card every app draws for a blocking surface —
  `popup()`, `line_strong()` edge, 10px radius, one shadow. The header is 44
  high with the title at `font_brand` 15/600 and a quiet close; the footer
  holds the one filled primary. Notes' prompt, quick open, shortcut sheet and
  Trash are this same card.
- **Primary action:** one to a surface. Fill with `.bg(accent())` and
  `.text_color(on_accent())`; Warp Station's primary paths hover
  `accent_hover()`, while warp-notes' primary button and warp-agent's shared
  `controls::button(…, BtnKind::Primary(accent), …)` dim to 0.9.
- **Ghost action:** `ghost_bg()` / `ghost_border()` (station) read the live
  ramp; `GHOST_BORDER_HOVER` is still a Copper constant on the dialogs'
  secondary hover edge.
- **Head icon verb:** `controls::icon_button()` — 34 square on `panel()`,
  `line_strong()` edge, `raised()` hover, glyph as the child. Warp Station's
  detail head (`head_icon`) and warp-notes' note head (`…`) are this one.
  The list heads' smaller quiet verbs are theirs, not this shape.
- **Segmented control:** a trough 34 to 36 high, padding 3, radius 8, on
  `well()` (the shared pickers add a `hairline()` or `line()` edge); segments 28
  to 30 high, radius 6, `font_row_meta`, active on `raised()` with `text()`, the
  rest `text_muted()`/`text_dim()` and `text()` on hover. `settings::segmented`
  and `text_size_control`/`palette_control` are the 34/28 ones; Warp Station's
  36/30 `segment_row`/`segment` in `ui/repos.rs` is the list's. warp-notes'
  list scope (`All` / `Pinned`) uses the shared 34/28 control rather than a
  copy of station's private pair; its counts ride in the segment labels.
- **Settings rows:** `settings::{row, row_with_desc, stack_row, switch_row,
  switch_row_with_desc, subhead, rows}` — a 60-high row, label and optional
  description left, control right, `border_light()` rule between rows, no card
  around them. Switches use the kit's 36×20 `Switch`.
- **Nested scrollers:** a scroller inside another (a fold in the chat
  transcript, a menu over a list) takes the wheel itself while it has content
  left and lets the step chain out only when it is hard against the edge in
  that direction. GPUI hands one wheel event to every scrollable hitbox under
  the pointer, so without this the outer one moves on the same tick.
  `scroll::consume_wheel` is that rule — call it from the nested element's
  `.on_scroll`, tracked by its own `ScrollHandle`; `fold_body(id, &handle)`
  already does. Do not share one handle between two folds: their offsets
  would move together.
- **Keycap:** `field::kbd()` — 20 high, 4px radius, `line_strong()` edge,
  `font_mono_meta`; station's draw call is the same shape.
- **Search field:** `search_bg()` is `well()`, `search_border()` is
  `hairline()`; the `SEARCH_*` constants are the Copper reference.
- **Accessibility:** every hand-rolled control goes through `A11y`
  (`.a11y_button`, `.a11y_toggle`, `.a11y_tab`), plus a `debug_selector` so a
  probe can find it.

Adoption of the shared helpers is partial: `field`, `settings`, `motion` and
`popover_card` are used by all three apps, and `controls::icon_button` by Warp
Station and warp-notes. `controls::button` is warp-agent's; `menu_row` takes no
debug selector, so warp-notes and Warp Station keep their menu rows local while
their probes click selectors, and the list heads still draw their own rows.
Use the core helper for new code rather than adding another copy.

## Text editing

- **Editable fields** are `field::{text, mono, area, mono_area, custom}` over
  `gpui_kit::component::input::{Input, Textarea}`. Do not build a raw input or
  a second text box: the field owns the surface, the kit widget owns editing.
- **Editing is the widget's.** Drag, double-click (word), triple-click (line),
  shift-click, the selection keys, and Ctrl/Cmd+A/C/X/V come from the widget.
  Right-click opens Cut / Copy / Paste / Select all: a native OS menu on
  Windows and macOS, and a drawn `PopupMenu` fallback on Linux through
  `gpui_base::Root` (which `gpui_kit::open_window` mounts). Keep
  `gpui_kit::init` in `bootstrap::init`; it installs the overlay the fallback
  needs.
- **Read-only selectable text** — the diff body, a PR comment, a transcript
  message — selects through the window's `TextSelectionLayer` instead. Give it
  the Copy menu: install `text_menu::install(cx)` once before the first frame,
  render the returned `TextMenuAnchor` at the root, and wrap the surface in
  `text_menu::attach(id, element, text_menu::focus(cx))`. The menu is drawn by
  GPUI, so it matches the family on every platform.
- **The markdown editor** (`warpui-kit-markdown`) owns its own menu (Cut / Copy
  / Paste / Select all while editing, Copy / Select all read-only). Do not
  wrap a `LiveEditor` in `text_menu`; it would open two menus.
- **Keyboard copy** of a read-only selection is the host's. Neither surface
  takes focus, so Ctrl/Cmd+C never dispatches to it: the window's key handler
  copies the window selection (`TextSelection::selected_text`) or the
  document's (`read_only_selection_text`), and leaves the key alone while an
  `Input` context owns it (Warp Station's `MainWindow` is the reference).
- **Masked, read-only and disabled** states are the widget's to enforce. A
  masked field must not put its value on the clipboard through either the
  keyboard shortcut or the menu; a bump of `gpui-kit` gets the probe in
  `kits/warpui-kit-core/tests/text_editing_probe.rs` first.

## Text surfaces

Two kinds of text. The line is what the surface *is*, not how long it is.

- **Content** is prose the user reads or writes: a chat message, reasoning,
  tool output, a question, a note, a README or summary, a PR or issue body, a
  license. It renders as markdown through `warpui-kit-markdown` — the
  read-only viewer (`read_only_document` / `render_read_only_*`) where the
  user never edits, the live editor where they do (warp-notes). A surface
  where one editor per item is the wrong cost — a volatile list of small
  comments, a streaming tool payload — may use `TextView` instead, and then
  keeps `text_menu`. Selection, links and highlighted code stay either way.
- **The named `TextView` exceptions.** warp-agent's transcript is the current
  set, and both are deliberate:
  - **Tool details** (plain payloads; a payload that parses as JSON goes to
    the diff kit's code renderer) is sanctioned by the wrong-cost rule above:
    up to three volatile, possibly streaming payloads per tool, where one
    editor each is the wrong cost.
  - **System note** is a status by role (Labels) that keeps the content path so
    its text, notably error text, stays selectable and copyable; a real-view
    probe covers that Copy menu. Do not give a note an editor by accident.

  The transcript's other content surfaces — assistant text, the user message,
  reasoning, and a question's text and resolution — use the read-only
  `LiveEditor`. The kit's `host_menu` hook appends an item's own actions after
  Copy / Select all (the user message's Edit in input / Re-prompt / Fork), and
  `LiveEditorState::apply_streamed` feeds a streaming surface while keeping
  the reader's selection and scroll (reasoning). The per-surface decisions
  live beside `text_view` in
  [`ui/markdown.rs`](../../apps/warp-agent/src/ui/markdown.rs).
- **Labels** are fixed UI vocabulary: buttons, menu rows, tabs, list-row
  titles, field labels, section heads, tool names, statuses, counts. They are
  plain `div`/`SharedString` text, never markdown, never an editor, never a
  selection menu. Emphasis comes from `font_weight` and the palette, not from
  markup.

Rules that follow:

- **An exception names its constraint and has a probe.** A content surface
  that stays on `TextView` states the exact interaction an editor would lose,
  and a real-view probe covers that interaction today (warp-agent's
  `tests/text_surfaces_probe.rs` covers the tool-details guard, the system
  note's Copy menu, the user message's host menu and a streamed reasoning
  selection). "It was easier" is not a reason.
- **The context menu belongs to the surface.** A `LiveEditor` owns its menu
  (Cut/Copy/Paste/Select all while editing, Copy/Select all read-only); do not
  wrap it in `text_menu`. A host appends its own rows to a read-only editor's
  menu through `LiveEditor::host_menu`. A `TextView` or diff body takes
  `text_menu::attach(…, text_menu::focus(cx))`. A label takes the row's action
  menu (pin, rename, delete), not a Copy menu.
- **An item's own actions extend the text menu, they do not replace it.** A
  chat bubble may add Edit/Reprompt/Fork, but Copy and Select all stay for a
  selection inside it, and Ctrl/Cmd+C copies the selection.
- **A one-line clamp is still content.** `max_lines(1)` is layout; a tool's
  output is content even when it is clamped. A tool's *title* is a label and
  stays plain text.
- **No parser may take the window down.** A text the markdown parser cannot
  handle falls back to plain prose (warp-agent's `text_view` guard), never to a
  panic and never to a label. In warp-agent the guard is mandatory for every
  surface that stays on `TextView`; it is not a reason to keep one there,
  because the kit viewer's line-based parser does not have that failure mode.

## Keyboard navigation

Keys belong to the surface that has focus. A window may hold several lists;
each has its own cursor, and only the focused one moves. There is no
window-wide list cursor and no window-level arrow handler.

- **A list is a focusable pane.** Give it its own `FocusHandle`,
  `.track_focus(&handle)` on the list element, and handle keys in that
  element's `.on_key_down`. A click on a row or on the pane focuses it. Do not
  put the handler on the window: it would take arrows from the editor, the
  composer and the search field.
- **One cursor per list.** `Up`/`Down` step it, `Home`/`End` go to the ends,
  `PageUp`/`PageDown` move by a page, `Enter` opens or activates, `Left`/`Right`
  collapse and expand in a tree. The cursor scrolls into view on every move.
  Notes' `handle_tree_key` / `move_tree_cursor` is the reference key map; a new
  list copies the map, not the tree's path logic.
- **Cursor and selection are different.** The cursor is the keyboard's
  position; the open/active row keeps its accent edge and may differ (notes
  previews on click, opens on `Enter`). The cursor carries the focus emphasis
  only while its pane is focused; an unfocused list may keep it as a quiet
  marker, so two lists never look armed at once.
- **Fields and editors win.** With a text field, the composer or the markdown
  editor focused, arrows move the caret/selection and never reach a list. That
  is why the list handler lives on the list.
- **Global keys stay global.** Quick open, search, palette and app shortcuts
  stay on the window context. A list claims `Enter`, `Space`, `Delete` and
  `F2` only while focused, plus `Shift+F10` for its row menu.
- **The cursor is accessible.** Rows carry `a11y_tree_item`/`aria_selected`
  and the list exposes a name, so a probe can move the cursor without a
  pointer.

## The About screen

Every GPUI app carries **About above Settings** at the bottom of the rail, and
all three draw the same screen from `warpui_kit_core::about`:

- the list holds the four suite members — Station, Agent, Notes and the
  headless SnapIt — and the row for the app you are in carries a `This app`
  tag;
- the detail is a hero (mark, channel tag, tagline, Repository / Changelog /
  Report an issue), a **Build** card (version, build, channel, released,
  runtime) and a **Legal** card (GPL-3.0, copyright, `View license`);
- the detail owns one filled primary: `Release notes` on the current app,
  `Open <app>` on a sibling, and `Open controls` (Station) or `Open Station`
  (a sibling) for SnapIt. On Linux SnapIt's row says `Windows only` and has
  no primary;
- `Copy version` writes `Warp Station 0.1.0 (1b7d006) · dev` — one line, the
  format support asks for.

The marks are monochrome `currentColor` SVGs in
`kits/warpui-kit-core/assets/marks/`, so the accent tints them and they follow
a palette switch like everything else. Build facts come from each app's
`build.rs` (`WARP_SUITE_BUILD`, `WARP_SUITE_CHANNEL`, `WARP_SUITE_RELEASED`),
which the release scripts export; a plain `cargo build` has no script behind
it and reports `dev` rather than pretending to be a release. `View license`
opens the app's own GPL text in a modal, embedded at build time and rendered
through `warpui_kit_markdown`'s read-only document viewer — the GPL reads as
prose, at the family's type, selectable — so the license ships with the binary
and the MIT kit carries none of it.

SnapIt has no window of its own. It is a row on the other apps' screens, its
`-V` prints the same build line, and Station's Tools › SnapIt page repeats
the version the row promises.

## Where a change goes

- A new color or tone: `palette.rs` — add it to `Ramp` (with a value in all four
  presets) and a reader if it is part of the live surface system, or to the
  fixed semantic block if it means one thing. Update the contrast test.
- A new size: `fonts.rs`, and only if no existing step fits.
- A new shared shape: `warpui-kit-core` (see `kits/README.md`), not an app —
  `controls.rs` for a button/menu, `field.rs` for a field, `settings.rs` for a
  settings row, `about.rs` for anything the About screen shows, `motion.rs` for
  anything that moves.
- A new list or text surface: settle content vs label first
  (`## Text surfaces`), then give the list its own focus and key handler
  (`## Keyboard navigation`).
- Startup, text size or palette plumbing: `bootstrap.rs` / `scale.rs`.
- App-only layout constants: the app's `theme.rs`.
