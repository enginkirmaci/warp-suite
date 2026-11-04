# kits

The shared component family the suite's apps build on: one cargo workspace
under `kits/` for the five `warpui-kit-*` crates, extracted from warp-agent and
Warp Station.

| Crate | Provides |
| --- | --- |
| `warpui-kit-core` | the switchable palettes, bundled fonts, type scale, shell, motion, `apply_kit_theme`, a11y, icons, tooltips, the control shapes (`button`, `menu_row`, `popover_card`, `fold_body`) and the media helpers |
| `warpui-kit-diff` | `render_diff`, `render_code`, `sniff_lang`, tree-sitter highlighting |
| `warpui-kit-markdown` | the live markdown editor, span parser, highlighter and formatting ops |
| `warpui-kit-picker` | `FilteredPicker`: trigger, filter input, virtualized rows, overlay popups |
| `warpui-kit-platform` | the gpui-free plumbing every app had re-grown: atomic file replace, file logging; the rust-embed-over-gpui-kit asset source rides behind the `assets` feature |

## Where a shared shape goes

A shape that two apps draw is a kit shape, not an app's. warp-agent's
`ui/controls.rs`, its `ui/tooltip.rs` (a copy of core's `tooltip_overlay`) and
its `mime_for_path`/`decode_png_render` were moved here once the same shapes
started appearing in the other apps. An app keeps its `theme.rs` (the family's
names, its own metrics) and its screens; the widgets come from here.

The kit's own drawing code reads `palette::current_ramp()` through the
accessors, never the published Copper constants, so a palette switch moves the
kit's chrome with the app's. `tests/palette_follows_probe.rs` enforces that at
the source, because gpui's headless context reports bounds and not painted
colours.

## Working here

```sh
cargo test                  # all five crates, headless GPUI probes included
cargo fmt --check
cargo clippy --all-targets
```

`warpui-kit-core` is the base: every other crate depends on it. The component
crates are independent of each other except that `-markdown` uses `-diff` for
code blocks. `-platform` is the odd one out — no gpui by default, and
`warp-station-core` takes it with `default-features = false`.

## How the apps consume it

The apps point at this workspace from their own directory:

```toml
warpui-kit-core = { path = "../../kits/warpui-kit-core" }
```

warp-station's crate sits two levels deeper
(`apps/warp-station/crates/warp-station`), so it uses
`../../../../kits/...`.

The apps live under `apps/` in this repo, so a release builds from the suite
checkout alone. The suite commit is the kits' provenance: the Linux bundle's
`REVISIONS.txt` records it against each crate, and there is no separate sync
step.

Adding a crate: give it a directory with a `Cargo.toml` and `src/`, add it to
the workspace `members` in `Cargo.toml`, and both release jobs pick it up
automatically.
