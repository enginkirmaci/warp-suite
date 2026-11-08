# GPUI Kit app recipe

The annotated skeleton for a new app in this suite, extracted from the apps
already here (`apps/warp-notes` is the closest whole example, then
`apps/warp-station` and `apps/warp-agent`). Everything below is conventions they
already follow — match them rather than inventing variants. `apps/warp-snapit`
is the exception: a Windows-only Win32 binary that does not link the kit and
does not follow this recipe.

The blocks build together on **gpui-kit 0.9.0 + gpui-pre `=0.3.11`** (the pins
in this checkout) and pass the gates; `logging.rs` and `paths.rs` are thin
wrappers over `warpui-kit-platform` and can be copied from
`apps/warp-notes/src/`.

**Where a new app goes.** Inside warp-suite, under `apps/<name>/`, imported with
`git subtree add --prefix=apps/<name>` so it keeps its history. The path deps
below assume that layout (`../../kits/...`). A standalone checkout is the
exception now, not the rule.

## 1. Cargo.toml

```toml
[package]
name = "myapp"
version = "0.1.0"
edition = "2024"
description = "One line"

# The library boundary exists so the headless probes (tests/) can build the
# real views; the binary is a thin bootstrap over it.
[lib]
name = "myapp"
path = "src/lib.rs"

[[bin]]
name = "myapp"
path = "src/main.rs"

[dependencies]
# the shared component family: a path dep inside the suite checkout
# (apps/<app>/ -> ../../kits/, apps/warp-station/crates/<crate>/ -> ../../../../kits/).
warpui-kit-core = { path = "../../kits/warpui-kit-core" }
warpui-kit-platform = { path = "../../kits/warpui-kit-platform" }

# gpui-pre: crates.io snapshots of zed's gpui, pinned to the exact rev
# gpui-kit 0.9 is built against (=0.3.11). Bump together with gpui-kit.
gpui = { package = "gpui-pre", version = "=0.3.11" }
gpui_platform = { package = "gpui-pre-platform", version = "=0.3.11", features = ["wayland", "x11"] }
gpui-kit = "0.9"

anyhow = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "fmt"] }
tracing-appender = "0.2"

# debug-embed: embed assets at compile time in debug builds too — without it
# rust-embed resolves "assets" against the process CWD at runtime, so the app
# only finds its icons when launched from the repo root
rust-embed = { version = "8", features = ["debug-embed"] }

[target.'cfg(windows)'.build-dependencies]
winresource = "0.1"

[dev-dependencies]
# headless UI probes (tests/); feature-unifies with the normal gpui-kit dep
# only under `cargo test`
gpui-kit = { version = "0.9", features = ["test-support"] }
```

Notes:

- `gpui-pre` + `gpui-pre-platform` must match the kit exactly. The suite pins
  them office-wide (every app and every kit crate): a new member joins the
  existing pin, it does not introduce a second snapshot. Bump every crate in the
  same commit, or the workspace fails to resolve.
- Never add the real crates.io `gpui`. The `gpui` key above is the `gpui-pre`
  remap, and `gpui_kit::application`, `gpui_kit::gpui::…` work without it.
- Windows-only deps go under `[target.'cfg(windows)'.dependencies]`
  (e.g. the `windows` crate with explicit features, as in
  `apps/warp-snapit/Cargo.toml`).
- Add `warpui-kit-diff`, `-markdown` or `-picker` when the app shows diffs,
  edits markdown or needs a filtered picker.

## 2. lib.rs and main.rs — bootstrap order

`lib.rs` is the module list; probes depend on it. `apps/warp-notes` and
`apps/warp-station/crates/warp-station` have the boundary; `apps/warp-agent` is
binary-only and its one probe targets the markdown kit rather than the app's
views, so prefer the lib boundary for a new app:

```rust
//! MyApp — one-line purpose. The library boundary exists so the headless
//! probes (tests/) can build the real views.

pub mod assets;
pub mod logging;
pub mod paths;
pub mod services;
pub mod theme;
pub mod ui;
```

The order in `main.rs` matters; copy it:

```rust
//! MyApp — one-line purpose, on GPUI.
// no console window behind the packaged Windows exe; debug builds keep it for logs
#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

use myapp::ui::window::MainWindow;
use myapp::{assets, logging, services, theme};

use gpui::prelude::*;
use gpui::{App, Bounds, TitlebarOptions, WindowBounds, WindowOptions, px, size};
use gpui_platform::application;

fn main() {
    // 1. legacy layout migrations first: they may move the folder logging
    //    and settings live in (see `paths.rs` below)
    // paths::migrate_legacy_layout();

    // 2. logging next — everything after can log. Keep the guard in main
    //    so the file stays open for the process lifetime.
    let _log_guard = logging::initialize();
    // 3. panic hook so crashes reach the log
    services::install_panic_hook();

    // 4. single instance (when it makes sense): a live door owner takes the
    //    launch args (a bare liveness probe when there are none) and this
    //    launch exits — see apps/warp-agent/src/ipc.rs
    // if ipc::try_forward(&folders) { return; }

    application()
        .with_assets(assets::EmbedAssets::new())
        .run(|cx: &mut App| {
            // 5. kit components (input decorations, popovers), the shared text
            //    size and palette from ~/.warp/ui.json, the follow hooks, the
            //    first kit-theme pass and reduced motion — all before any kit
            //    component renders. A new app has no old text size and no
            //    reduced-motion preference, hence `None, false`.
            warpui_kit_core::bootstrap::init(cx, None, false, |cx: &mut App| {
                theme::apply_kit_theme(cx);
            });

            // 6. app key bindings AFTER the bootstrap, so ours win "Input" context
            //    ties (later registration wins at the same depth)
            // cx.bind_keys([gpui::KeyBinding::new(...)]);

            // 7. spawn background threads / build the store, then open windows.
            //    gpui_kit::open_window wraps the view in the kit's Root, which
            //    dialogs, sheets and notifications need.
            let bounds = Bounds::centered(None, size(px(980.), px(700.)), cx);
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    // empty by default in GPUI — needed so Hyprland/launchers
                    // match StartupWMClass in the desktop entry
                    app_id: Some("myapp".into()),
                    titlebar: Some(TitlebarOptions {
                        title: Some("MyApp".into()),
                        appears_transparent: true,
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                cx,
                |_, cx| cx.new(|_| MainWindow),
            )
            .expect("failed to open window");
            cx.activate(true);
        });
}
```

The startup order lives in `bootstrap::init`
(`kits/warpui-kit-core/src/bootstrap.rs`). It is the only place that calls
`gpui_kit::init`, `adopt_text_size`, `adopt_palette`, `follow_palette`,
`follow_ui_scale` and `motion::set_reduced`; do not hand-roll that sequence.
`docs/agents/design-system.md` explains what each step settles.

`logging.rs` is `apps/warp-notes/src/logging.rs` — one call to
`warpui_kit_platform::logging::init(&AppPaths::logs_dir(), "myapp-")`, returning
the guard. `services::install_panic_hook` is in `apps/warp-agent/src/services.rs`.

`ui/mod.rs` is `pub mod window;`, and each view renders from the kit theme:

```rust
//! The main window: one sentence on its layout and design language.

use gpui::prelude::*;
use gpui::{Context, Window, div};
use gpui_kit::component::ActiveTheme;

pub struct MainWindow;

impl Render for MainWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().bg(cx.theme().background)
    }
}
```

## 3. theme.rs — re-export the family

Colors, fonts, sizes and the kit mapping live in `warpui-kit-core`. An app's
`theme.rs` re-exports them and adds only its layout constants. Copy
`apps/warp-agent/src/theme.rs` for the live-accent shape; `warp-notes` and
Warp Station still pin the accent and should not be copied until they follow
the ramp. warp-agent's `apply_kit_theme` takes `(AccentColors, &FontTokens,
&mut App)` because its store holds snapshots; the snippet below reads the live
values itself, which suits a new app. `docs/agents/design-system.md` has what
is in there.

```rust
//! The MyApp theme: the family palette, type and scale from warpui-kit-core,
//! re-exported under the app's `theme::` paths, plus the kit mapping.

use warpui_kit_core::palette;
// every shared font step and color const stays reachable as `theme::X`
pub use warpui_kit_core::fonts::*;
pub use warpui_kit_core::shell::scaled;

/// Map the family palette onto gpui-kit's shadcn-style theme tokens. Called by
/// `bootstrap::init` before the first frame, and again after a palette or
/// text-size change.
pub fn apply_kit_theme(cx: &mut gpui::App) {
    let ramp = palette::current_ramp();
    warpui_kit_core::apply_kit_theme(
        // the accent follows the live palette: one switch moves surfaces and accent
        ramp.accent_colors(),
        &warpui_kit_core::fonts::active_tokens(),
        // the step the kit's sidebar token paints; a new app with no
        // full-height rail passes the canvas, as here. warp-agent passes
        // window_chrome(), Warp Station surface().
        ramp.base,
        cx,
    );
}

// The live palette reads through core's readers (they follow the ramp the
// settings page switches); these aliases are the app's own vocabulary.
pub use warpui_kit_core::palette::{
    accent, accent_hover, accent_ink, accent_soft, border_light, hairline, line_strong, on_accent,
    panel, popup, raised, surface, text, text_dim, text_muted, well, window_chrome,
};
```

- `apply_kit_theme` registers the bundled **Inter and JetBrains Mono** faces
  and resolves the families itself, so `main.rs` needs no font code and
  `assets/` holds no font files. `ui_font()` and `code_font()` return what it
  picked.
- Don't hardcode font families: naming a missing family makes gpui fall back per
  text run and skip the kit's own probe.
- Surface order is **well < chrome < base < panel < raised < overlay**, four to
  eleven points apart, so a panel needs a border and not just a tone. Font/icon
  size tokens ride one factor so a text size change rescales the whole UI.
- Show `warpui_kit_core::text_size_control()` and `palette_control()` in the
  app's Appearance settings; the app stores nothing for either.
- The accent is a parameter of `apply_kit_theme`, not a setting: pass
  `current_ramp().accent_colors()` so a palette switch carries it. warp-notes and
  warp-station still pass a fixed family and their accent does not follow yet;
  do not copy their signature for a new app (the design doc records the status).
- gpui's `rgba()` is not const in this rev; const colors use
  `warpui_kit_core::rgba_hex`.
- `theme.rs` must not keep palette values as constants: the palette is
  switchable at runtime (see `docs/agents/design-system.md`).

## 4. assets.rs — embed icons, fall back to the kit

```rust
//! Embedded asset source: our icons ship inside the binary, with gpui-kit's
//! bundled Lucide set as the fallback so kit components (input decorations,
//! popovers, …) can resolve their IconName assets.

use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "assets"]
pub struct Assets;

pub type EmbedAssets = warpui_kit_platform::assets::KitBackedAssets<Assets>;
```

- `rust-embed`'s `debug-embed` feature is not optional here: without it the
  crate resolves `assets` against the process CWD at runtime, so the app only
  finds its icons when launched from the repo root.
- `KitBackedAssets<E>` owns the `AssetSource` impl (`load` then `list`), with the
  app's own files first and the kit's as the fallback. Add
  `warpui-kit-platform` with its default `assets` feature; `rust-embed` still
  needs `debug-embed` in the app's own dependency.
- Warp Station outgrew this: `rust-embed` silently drops every file whose
  `canonicalize` fails (mapped/subst Windows volumes fail it for everything),
  leaving an empty set, so its `build.rs` generates an `include_bytes!` table
  from plain `fs` reads and `assets.rs` `include!`s it. Copy that if an app
  ships a large icon set on Windows.

## 5. paths.rs — per-user locations

Suite apps keep everything in one folder under `~/.warp`, one subfolder each,
with `warpui_kit_platform::paths` owning the layout and its migrations (the
notes content is the exception, at `~/notes`):

```rust
//! Global per-user locations: settings and rolling logs under ~/.warp/myapp.

use std::path::PathBuf;
use warpui_kit_platform::paths;

pub struct AppPaths;

impl AppPaths {
    pub fn config_dir() -> PathBuf {
        paths::app_dir("myapp")
    }

    pub fn logs_dir() -> PathBuf {
        paths::logs_dir() // the suite's shared ~/.warp/logs
    }

    pub fn settings_file() -> PathBuf {
        Self::config_dir().join("settings.json")
    }
}
```

Settings are serde JSON loaded from `AppPaths::settings_file()`. warp-agent's
`Store` owns them, notes' `UiPrefs` owns its `settings.json`, and Warp Station's
`Store` wraps an `Arc<SettingsService>` from `warp-station-core`; KwmSwitcher
(outside the suite) shares an `Arc<RwLock<AppConfig>>` across its threads.

Once a layout has shipped, a moved folder gets a startup migration. The
platform's `paths` module carries the primitives (`adopt_legacy_dir`,
`adopt_legacy_logs`, `adopt_notes_store`, `mark_notes_store_adopted`) and each
app composes them in its own startup migration (`AppPaths::migrate_legacy_layout`
in notes/agent, the `paths::migrate_*` calls in Warp Station); call it before
logging and before anything reads the moved folders. Migrations are idempotent
and the notes-store scan is one-shot behind a marker.

## 6. Packaging and the suite

- Windows exe icon: `winresource` in `build.rs`, windows-gated — see
  `apps/warp-notes/build.rs`. Console hiding: the `windows_subsystem` cfg_attr
  in the main.rs snippet above.
- **The app ships assets only.** `packaging/linux/` (`<app>.desktop`, `icons/`)
  and `packaging/windows/` (`.iss`, `.ico`, `settings/`); there is no
  `install.sh` or `build.ps1` in an app. The suite owns every build and install
  step: `scripts/build.sh` (Linux bundle + tarball), `scripts/build.ps1`
  (Windows zips + Inno Setup), `scripts/deploy.{sh,ps1}`,
  `scripts/bundle-install.sh`. `scripts/build.sh` rewrites the desktop entry's
  `Exec=` line with the `__WARP_SUITE_BIN__` placeholder resolved at install
  time, so a `.desktop` file must keep that placeholder.
- A shipped app lives in `apps/<name>/` and gets an `[apps.<name>]` section in
  `apps.toml` only when it needs non-default build knobs (`package`,
  `linux_build`, `linux_skip`, `icon_prefix`, `scalable_svg`); it documents its
  contracts under *Cross-app contracts* in the suite README and carries its own
  `AGENTS.md`; the suite-wide rules are in the root `AGENTS.md` and the deep
  guides live in `docs/agents/`.
- Windows builds an installer only when `packaging/windows/<app>.iss` exists;
  without one, `build.ps1` ships a portable zip. A Windows-only app sets
  `linux_skip = true` (see `apps/warp-snapit` in `apps.toml`).
- Linux system tray: `ksni` with `blocking`/`async-io` features
  (`KwmSwitcher/src/tray.rs`, outside the suite).
- Autostart: FreeDesktop `.desktop` entry in the user autostart dir
  (`apps/warp-station/crates/warp-station-core/src/autostart.rs`).

## 7. UI tests recap

- `gpui-kit = { version = "0.9", features = ["test-support"] }` under
  `[dev-dependencies]` only.
- `tests/<name>_probe.rs`, `#[gpui_kit::gpui::test]`,
  `cx.update(gpui_kit::init)` plus the app's `theme::apply_kit_theme`, real views
  through the `lib.rs` boundary in headless windows,
  `VisualTestContext::from_window(handle.into(), cx).into_mut()`.
- Explicit imports in probe files: a glob of GPUI breaks
  `#[gpui_kit::gpui::test]` (see `docs/agents/testing-and-captures.md`).
- Geometry assertions via `cx.debug_bounds("selector")`; input via
  `cx.simulate_click`/`simulate_mouse_move`, keyboard through the focus path;
  always `run_until_parked()` and force a draw (`drop(window.draw(cx))`) before
  asserting.
- Examples to copy: `apps/warp-notes/tests/notes_probe.rs` (a real view),
  `apps/warp-station/crates/warp-station/tests/split_probe.rs` (layout probes),
  `apps/warp-station/crates/warp-station/tests/settings_probe.rs` (controls),
  and `kits/warpui-kit-core/tests/` for the shared controls under a scratch
  `HOME`.
