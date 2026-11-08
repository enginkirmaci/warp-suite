# Testing and looking at a running app

How the suite proves UI behaviour without a screen, and how to put a real
window in front of a capture when a probe cannot answer the question.

## Probes

- UI integration tests are headless **probes**: `tests/<name>_probe.rs` using
  `#[gpui_kit::gpui::test]` (or `#[gpui::test]`), `TestAppContext` /
  `VisualTestContext`, real components in a headless window.
- `gpui-kit/test-support` goes under `[dev-dependencies]` only, never in
  `[dependencies]`.
- Run them with the package selector from the repo root:
  `cargo test -p warp-notes`, `cargo test -p warp-station`,
  `cargo test -p warp-agent`,
  `cargo test -p warpui-kit-core`.
- CI (`.github/workflows/kits.yml`) runs `cargo fmt --all --check`, clippy
  with `-D warnings` on the kit and app crates, then the kit and app tests.
  Match that locally before calling a UI change done; a passing probe is not
  the whole gate.

Probe skeleton (the shape used across
`apps/warp-station/crates/warp-station/tests/`):

```rust
use gpui::{Context, IntoElement, Render, TestAppContext, Window, div};

struct ProbeElement;

impl Render for ProbeElement {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

#[gpui_kit::gpui::test]
fn probe_name(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    // put the family palette/type on the kit tokens before anything draws
    cx.update(myapp::theme::apply_kit_theme);
    let (_root, cx) = cx.add_window_view(|_, _| ProbeElement);
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    // assert via cx.debug_bounds(selector), focus, state, callbacks
}
```

warp-agent's `theme::apply_kit_theme` takes the accent and `FontTokens`; call
it through a closure that passes that app's snapshot.

- Import explicitly in probe files. `use gpui_kit::*;` or `use gpui::*;` brings
  gpui's `test` attribute into scope over std's, and `#[gpui_kit::gpui::test]`
  then fails with `recursion limit reached while expanding #[test]`.
- `ElementSnapshot` is immutable — re-query after interactions. Optional state
  means "unreported", not false.
- Every UI behavior change gets a probe when the behavior can be exercised
  headlessly; geometry claims (equal widths, containment) are asserted via
  `debug_bounds`, not eyeballed.
- A probe that writes shared state runs under a scratch `HOME`
  (`kits/warpui-kit-core/tests/text_size_probe.rs`,
  `kits/warpui-kit-core/tests/palette_probe.rs`). Set `USERPROFILE` and
  `XDG_CONFIG_HOME` too when Windows resolves the data root (see
  `warp-station/tests/settings_probe.rs`), and serialize the HOME swaps behind a
  lock. Inside core's own tests, process-wide state such as the scale factors
  takes turns behind `fonts::factor_lock::serial()`.
- The kit's own probes cover the shared controls: the text-size and palette
  probes write `~/.warp/ui.json` under their own scratch HOME, and
  `palette_follows_probe` source-scans the kit's drawing code so a fixed ramp
  constant cannot come back.
- `cargo test` after changing a kit crate builds the apps against it directly —
  that is the point of the one workspace, so run it when a kit's public API
  moves.

## Environment hooks

Each app opens a page for a capture through an env var, read once at window
construction. Use them instead of clicking through a hidden window.

| App | Variable | Values |
| --- | --- | --- |
| Warp Station | `WARP_STATION_PAGE` | `settings`, `summary`, `tools` |
| Warp Station | `WARP_STATION_SHEET` | tab label, case-insensitive: `Overview`, `Changes`, `History`, `Pull requests`, `Issues`, `Pipelines`, `README` |
| Warp Station | `WARP_STATION_SELECT` | a name or index in the list |
| Warp Station | `WARP_STATION_TOOLS_OPEN` | `formatters`, `nuget`, `clipboard` |
| Warp Station | `WARP_STATION_GROUPS` | `1` to group the repo list by root |
| Warp Station | `WARP_STATION_NOTES` | `1` to open the Notes panel on launch |
| Warp Station | `WARP_STATION_AUTO_FETCH_SECS`, `WARP_STATION_AUTO_REFRESH_SECS` | seconds, for background-refresh captures |
| Warp Station | `WARP_STATION_IPC_DIR` | override the agent-door directory |
| warp-agent | `WARP_AGENT_PAGE` | `settings`, `settings-chat`, `settings-permissions`, `settings-server` |
| warp-notes | `WARP_NOTES_PAGE` | `settings` |

The hooks are read once at launch; unset means the app's normal default. They
exist for captures and automation, not as user-facing settings.

## Looking at a running app

Do not start these apps with `hypruse launch`. `misc:focus_on_activate` is on
here and a gpui window asks for activation when it opens, so Hyprland moves the
owner's monitor to the agent workspace a few seconds later, despite the
background promise. Closing the window then leaves them on an empty workspace.
This launch keeps their screen and focus where they are, and keeps the hidden
window drawing, which a capture needs:

```sh
hyprctl dispatch "hl.dsp.exec_cmd(\"[workspace 97 silent; suppress_event activate activatefocus; no_initial_focus on; render_unfocused on] env HOME=/scratch/home /path/to/binary\")"
hyprctl -j activeworkspace     # confirm the view held
hyprctl -j clients             # the window's address, for `hypruse screenshot`
```

- The dispatcher on this box is the **Lua** build (Hyprland 0.56.2 on Omarchy
  4.5, `~/.config/hypr/*.lua`), so the plain form
  `hyprctl dispatch exec '[workspace 97 silent] …'` is a syntax error
  (`']' expected near '97'`): go through `hl.dsp.exec_cmd("…")` as above.
  Confirm with `hyprctl getoption misc:focus_on_activate` (`bool: true`) before
  trusting the rules; `hyprctl -j workspaces` shows whether 97 appeared at all.
- If `hyprctl` answers `HYPRLAND_INSTANCE_SIGNATURE not set`, the agent shell
  lacks the session environment; read the desktop through the `hypruse` MCP
  rather than guessing at window addresses.
- Avoid spaces that need quoting in the command. Close the window with
  `hypruse hypr close_window` when done.
- A scratch `HOME` keeps the apps off the owner's data. gsettings reads through
  `HOME`, so the desktop factor there is 1.0 and not the owner's 1.1667: compare
  sizes only between runs under the same `HOME`.
- A hidden gpui window takes no pointer or keys and exposes no accessibility
  tree, so reach a page through an env hook or a throwaway patch: copy the
  source file aside, build, copy the binary out, restore the file, confirm with
  `cmp`, rebuild. The hooks above are read once at window construction, which is
  why they work for this.
- The kit's own drawing code reads `palette::current_ramp()` and the scale
  factor through accessors, so a capture under a scratch `HOME` shows that
  `HOME`'s `~/.warp/ui.json`, not the owner's.
