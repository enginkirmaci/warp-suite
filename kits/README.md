# kits

The shared component family the suite's apps build on: six `warpui-kit-*`
crates, extracted from warp-agent and Warp Station. They are members of the
suite workspace (root `Cargo.toml`), not a workspace of their own; there is no
`kits/Cargo.lock`.

| Crate | Provides |
| --- | --- |
| `warpui-kit-core` | the switchable palettes, bundled fonts, type scale, shell, motion, `apply_kit_theme`, a11y, icons, tooltips, the control shapes (`button`, `menu_row`, `popover_card`, `fold_body`), the nested-scroll wheel rule (`scroll`), the About screen, the read-only text menu and the media helpers |
| `warpui-kit-diff` | `render_diff`, `render_diff_with`, `render_diff_with_scroll`, `render_code`, `sniff_lang`, `patch_from_hunks`, tree-sitter highlighting |
| `warpui-kit-markdown` | the live markdown editor, span parser, highlighter, formatting ops, and the read-only document viewer (`read_only_document`) |
| `warpui-kit-picker` | `FilteredPicker`: trigger, filter input, virtualized rows, overlay popups |
| `warpui-kit-platform` | the gpui-free plumbing every app had re-grown: atomic file replace, file logging, the sibling-app launcher; the rust-embed-over-gpui-kit asset source rides behind the `assets` feature |
| `warpui-kit-build` | build-script only (`[build-dependencies]`, never linked): reads `version.toml` and the `WARP_SUITE_*` env, stamps the About facts and the Windows resource block (file version, strings, icon); see `docs/agents/versioning.md` |

`render_diff_with_scroll` is the full-pane diff: pass a `DiffScrollHandles`
and the kit owns the vertical viewport, the per-column horizontal viewports
and the pinned-bar handles. Wrap mode there is a virtualised list, so a large
patch builds only its visible rows. The plain `render_diff_with` stays in
flow for an embedded block.

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
cargo test -p warpui-kit-core -p warpui-kit-diff -p warpui-kit-markdown \
  -p warpui-kit-picker -p warpui-kit-platform -p warpui-kit-build
cargo fmt --all --check
cargo clippy --all-targets -p warpui-kit-core -p warpui-kit-diff \
  -p warpui-kit-markdown -p warpui-kit-picker -p warpui-kit-platform \
  -p warpui-kit-build -- -D warnings
```

These run from `kits/` too: with no manifest here, cargo walks up to the suite
workspace.

`warpui-kit-core` is the base: every other crate depends on it. The component
crates are independent of each other except that `-markdown` uses `-diff` for
code blocks. `-platform` is the odd one out — no gpui by default, and
`warp-station-core` takes it with `default-features = false`.

## How the apps consume it

Every app repository pins the kit line by tag:

```toml
warpui-kit-core = { git = "https://github.com/enginkirmaci/warp-suite", tag = "kits-v0.1.11", package = "warpui-kit-core" }
warpui-kit-markdown = { git = "...", tag = "kits-v0.1.11", package = "warpui-kit-markdown", features = ["mermaid", "all-languages"] }
```

The `kits-v<version>` tag is the provenance: `kits.toml` on that release
records the version, build and commit. When an app is built through
`warp-suite/apps/<name>`, the root `[patch]` table swaps the git source for the
local `kits/` paths, so a kit change is visible without cutting a tag.

Adding a crate: give it a directory with a `Cargo.toml` and `src/`, add it to
the workspace `members` in the root `Cargo.toml`, and add it to
`[workspace.dependencies]` as soon as a second member needs it. Both release
jobs pick it up automatically.
