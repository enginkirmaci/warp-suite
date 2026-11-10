//! The palette control: a click switches every surface this process draws,
//! saves the choice where the other apps read it at their next start, and runs
//! the host's kit-theme hook so the kit's own tokens follow. Runs under its own
//! HOME, since the shared file lives there.

use std::path::PathBuf;
use std::sync::OnceLock;

use gpui::{Context, IntoElement, Modifiers, Render, TestAppContext, Window};
use warpui_kit_core::{follow_palette, palette, palette_control, palette_key};

/// HOME is process-wide and set once, before any test reads it.
fn scratch_home() -> &'static PathBuf {
    static HOME: OnceLock<PathBuf> = OnceLock::new();
    HOME.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("warp-palette-{}", std::process::id()));
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
        palette_control()
    }
}

#[gpui_kit::gpui::test]
fn a_click_switches_the_surfaces_saves_and_runs_the_hook(cx: &mut TestAppContext) {
    let home = scratch_home();
    cx.update(gpui_kit::init);

    // the app's kit-theme re-run: a palette switch must reach it, or the kit's
    // input ring and primary would keep the old palette
    let calls = std::rc::Rc::new(std::cell::Cell::new(0usize));
    let seen = calls.clone();
    follow_palette(move |_cx| {
        seen.set(seen.get() + 1);
    });

    let (_root, cx) = cx.add_window_view(|_, _| Picker);
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    // every palette is offered
    for ramp in &palette::RAMPS {
        assert!(
            cx.debug_bounds(Box::leak(format!("palette-{}", ramp.key).into_boxed_str()))
                .is_some(),
            "the {} segment is drawn",
            ramp.key
        );
    }

    // the last segment is the one a too-wide control would clip, so click it
    let last = palette::RAMPS.last().expect("at least one palette");
    let selector: &'static str = Box::leak(format!("palette-{}", last.key).into_boxed_str());
    let before = palette::text();
    let segment = cx.debug_bounds(selector).expect("the last segment");
    cx.simulate_click(segment.center(), Modifiers::default());
    cx.run_until_parked();

    assert_eq!(
        palette_key(),
        last.key,
        "the click switches the live palette"
    );
    assert_ne!(palette::text(), before, "the surfaces read the new ink");
    assert_eq!(
        palette::text(),
        last.text,
        "the ink is the clicked palette's own"
    );
    assert_eq!(calls.get(), 1, "the host's kit-theme hook runs once");

    let saved = std::fs::read_to_string(home.join(".warp").join("ui.json")).expect("saved");
    let doc: serde_json::Value = serde_json::from_str(&saved).expect("valid json");
    assert_eq!(doc["palette"], last.key);

    // put the process back on the default for the other tests in this binary
    palette::set_ramp("copper");
    assert_eq!(palette_key(), "copper");
}
