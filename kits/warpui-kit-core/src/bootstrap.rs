//! The shared app bootstrap. Every GPUI app in the suite ran the same startup
//! dance by hand — gpui-kit's init, the persisted text size and palette, the
//! follow hooks, the first kit-theme pass, reduced motion — each with its own
//! copy of the ordering comments; this bundles it so a change to the sequence
//! lands once.

use gpui::App;
use std::rc::Rc;

use crate::motion;
use crate::scale::{adopt_palette, adopt_text_size, follow_palette, follow_ui_scale};

/// Run the shared startup inside `application().run(...)`, before opening
/// windows: `gpui_kit::init` (kit components must init before any kit
/// component renders), the shared text size (seeded once from an app's old
/// setting, hence `text_size`) and palette, the follow hooks, the first
/// kit-theme pass and reduced motion (`reduced` ORs in the desktop's
/// preference).
///
/// `retheme` is the app's kit-theme mapping; it runs once here and again on
/// every palette or desktop-scale change, followed by a window refresh.
///
/// Bind app keybindings *after* this call: they must win ties against the
/// bindings `gpui_kit::init` installs.
pub fn init(
    cx: &mut App,
    text_size: Option<&str>,
    reduced: bool,
    retheme: impl Fn(&mut App) + 'static,
) {
    gpui_kit::init(cx);
    // the shared text size, seeded once from this app's old setting
    adopt_text_size(text_size);
    // the shared palette, before the first kit theme pass so the first frame
    // already draws in it
    adopt_palette(None);
    // a palette switch or a desktop scale change re-maps the kit and repaints
    // every window; `retheme` closes over whatever it needs (a store, statics)
    let retheme = Rc::new(retheme);
    follow_palette({
        let retheme = retheme.clone();
        move |cx: &mut App| {
            retheme(cx);
            cx.refresh_windows();
        }
    });
    retheme(cx);
    follow_ui_scale(cx, move |cx: &mut App| retheme(cx));
    // the family motion setting, or the desktop's preference, applies before
    // the first frame
    motion::set_reduced(reduced || motion::system_prefers_reduced());
}
