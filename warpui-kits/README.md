# warpui-kits

The shared GPUI component family the suite's apps build on: one cargo workspace
for the four `warpui-kit-*` crates, extracted from warp-agent and Warp Station.

| Crate | Provides |
| --- | --- |
| `warpui-kit-core` | palette, bundled fonts, type scale, shell, motion, `apply_kit_theme`, a11y, icons, tooltips |
| `warpui-kit-diff` | `render_diff`, `render_code`, `sniff_lang`, tree-sitter highlighting |
| `warpui-kit-markdown` | the live markdown editor, span parser, highlighter and formatting ops |
| `warpui-kit-picker` | `FilteredPicker`: trigger, filter input, virtualized rows, overlay popups |

## Working here

```sh
cargo test                  # all four crates, headless GPUI probes included
cargo fmt --check
cargo clippy --all-targets
```

`warpui-kit-core` is the base: every other crate depends on it. The other
three are independent of each other except that `-markdown` uses `-diff` for
code blocks.

## How the apps consume it

The apps point at this workspace from their own checkout:

```toml
warpui-kit-core = { path = "../warp-suite/warpui-kits/warpui-kit-core" }
```

warp-station's crate sits two levels deeper, so it uses
`../../../warp-suite/warpui-kits/...`. A local app checkout therefore needs
this repo as a sibling.

The apps do not check out this workspace themselves. `scripts/bundle-linux.sh`
and the Windows job in `.github/workflows/suite-release.yml` stage the whole
workspace at `.build/warp-suite/warpui-kits`, the same relative path from
every app clone, so a suite release builds from the suite repo alone. The suite commit is the kits' provenance: the
Linux bundle's `REVISIONS.txt` records it against each crate, and there is no
separate sync step.

Adding a crate: give it a directory with a `Cargo.toml` and `src/`, add it to
the workspace `members` in `Cargo.toml`, and both release jobs pick it up
automatically.
