//! The text size control: a click saves the choice where the other apps read
//! it at their next start, and moves this one. Runs under its own HOME, since
//! the shared file lives there.

use std::path::PathBuf;
use std::sync::OnceLock;

use gpui::{Context, IntoElement, Modifiers, Render, TestAppContext, Window};
use warpui_kit_core::{fonts, text_size_control};

/// HOME is process-wide and set once, before any test reads it.
fn scratch_home() -> &'static PathBuf {
    static HOME: OnceLock<PathBuf> = OnceLock::new();
    HOME.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("warp-text-size-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch home");
        // SAFETY: set inside the OnceLock, before this process reads HOME
        unsafe { std::env::set_var("HOME", &dir) };
        dir
    })
}

struct Picker;

impl Render for Picker {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        text_size_control()
    }
}

#[gpui_kit::gpui::test]
fn a_click_saves_for_every_app_and_applies_here(cx: &mut TestAppContext) {
    let home = scratch_home();
    cx.update(gpui_kit::init);
    let (_root, cx) = cx.add_window_view(|_, _| Picker);
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    let large = cx
        .debug_bounds("text-size-large")
        .expect("the Large segment is drawn");
    cx.simulate_click(large.center(), Modifiers::default());
    cx.run_until_parked();

    assert_eq!(fonts::font_scale(), fonts::FONT_LARGE);
    let saved = std::fs::read_to_string(home.join(".warp").join("ui.json")).expect("saved");
    let doc: serde_json::Value = serde_json::from_str(&saved).expect("valid json");
    assert_eq!(doc["text_size"], "large");

    assert!(
        cx.debug_bounds("text-size-larger").is_some(),
        "the other segments stay"
    );
    fonts::set_font_scale(None);
}
