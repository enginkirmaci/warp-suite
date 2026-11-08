//! The diff and code blocks must draw at the size their caller asks for, not at
//! whatever size the surrounding view happens to set. GPUI shapes a
//! `StyledText` at the *ambient* text style (a `TextRun` carries a font, never
//! a size), so the renderer has to pin the size on the element the rows live
//! in. Without the pin every diff in the suite ignored `font_detail` and
//! followed the app's text-size setting instead — which is exactly the bug
//! this probe locks down.

use gpui::prelude::*;
use gpui::{Context, Render, TestAppContext, Window, div, px};
use warpui_kit_core::{AccentColors, active_tokens, apply_kit_theme, fonts, palette};
use warpui_kit_diff::{render_code, render_diff};

const PATCH: &str = "\
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,4 @@
 fn a() {}
+fn b() {}
 fn c() {}
";

/// One hunk strip and three code rows: the `---`/`+++` metadata is no longer
/// drawn, and a single-file patch has no file header row.
const PATCH_ROWS: f32 = 4.;

struct DiffAt {
    font_detail: f32,
    /// The size the surrounding view sets on its own container, i.e. the
    /// ambient style a real app's root leaves in place.
    ambient: Option<f32>,
}

impl Render for DiffAt {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut host = div().size_full().flex().flex_col();
        if let Some(ambient) = self.ambient {
            host = host.text_size(px(ambient));
        }
        host.child(
            div()
                .debug_selector(|| "diff-host".into())
                .w(px(600.))
                .flex_shrink_0()
                .child(render_diff(PATCH, None, self.font_detail, window, cx)),
        )
        .child(
            div()
                .debug_selector(|| "code-host".into())
                .w(px(600.))
                .flex_shrink_0()
                .child(render_code(
                    "fn a() {}\nfn b() {}\nfn c() {}",
                    Some("rust"),
                    palette::CODE_FG,
                    self.font_detail,
                    window,
                    cx,
                )),
        )
    }
}

fn apply_theme(cx: &mut gpui::App) {
    apply_kit_theme(
        AccentColors::default_colors(),
        &active_tokens(),
        palette::surface(),
        cx,
    );
}

/// Height of the diff block and of the code block in a fresh window.
fn measure(cx: &mut TestAppContext, font_detail: f32, ambient: Option<f32>) -> (f32, f32) {
    let (_root, cx) = cx.add_window_view(|_, _| DiffAt {
        font_detail,
        ambient,
    });
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    let height = |cx: &mut gpui::VisualTestContext, selector| {
        cx.debug_bounds(selector)
            .unwrap_or_else(|| panic!("{selector} was not drawn"))
            .size
            .height
            .into()
    };
    (height(cx, "diff-host"), height(cx, "code-host"))
}

#[gpui_kit::gpui::test]
fn the_blocks_draw_at_the_requested_detail_size(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(apply_theme);

    let (diff_at_13, code_at_13) = measure(cx, 13., None);
    let (diff_at_26, code_at_26) = measure(cx, 26., None);
    let (diff_at_39, code_at_39) = measure(cx, 39., None);

    // A row is a line of text, so the block is rows × (size × the font's own
    // line height); that height is a font metric, not something to hardcode.
    // What must hold is that the *requested* size is the one it is built from.
    assert_eq!(
        diff_at_26,
        diff_at_13 * 2.,
        "doubling the detail size doubles the diff rows"
    );
    assert_eq!(
        code_at_26,
        code_at_13 * 2.,
        "doubling the detail size doubles the code rows"
    );
    assert_eq!(diff_at_39, diff_at_13 * 3., "the rows follow 13 → 39");
    assert_eq!(code_at_39, code_at_13 * 3., "the rows follow 13 → 39");

    // one row of each, so a silently empty block cannot pass
    let diff_row = diff_at_13 / PATCH_ROWS;
    let code_row = code_at_13 / 3.;
    assert!(
        diff_row > 13.,
        "a 13px row is at least 13px tall: {diff_row}"
    );
    assert!(
        code_row > 13.,
        "a 13px row is at least 13px tall: {code_row}"
    );
}

/// The regression itself: a surrounding view with a large ambient text size
/// used to stretch every diff, ignoring `font_detail` completely.
#[gpui_kit::gpui::test]
fn the_ambient_size_does_not_override_the_detail_size(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(apply_theme);

    let (diff_plain, code_plain) = measure(cx, 13., None);
    let (diff_ambient, code_ambient) = measure(cx, 13., Some(32.));

    assert_eq!(diff_ambient, diff_plain, "the diff keeps its own size");
    assert_eq!(
        code_ambient, code_plain,
        "the code block keeps its own size"
    );
}

/// The app's text size reaches the diff through the caller's `font_detail`
/// (warp-station passes `theme::font_mono_meta()`, warp-agent a token), so the
/// same call under a bigger setting must draw bigger.
#[gpui_kit::gpui::test]
fn the_app_text_size_reaches_the_diff_through_the_detail_size(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(apply_theme);

    let normal_detail = fonts::font_mono_meta();
    let (normal, _) = measure(cx, normal_detail, None);

    fonts::set_font_scale(Some(fonts::FONT_LARGER));
    cx.update(apply_theme);
    let larger_detail = fonts::font_mono_meta();
    let (larger, _) = measure(cx, larger_detail, None);
    fonts::set_font_scale(None);
    cx.update(apply_theme);

    assert!(
        larger_detail > normal_detail,
        "the setting must move the step the apps pass ({larger_detail} vs {normal_detail})"
    );
    // the block grows with the step it was handed, to within the pixel
    // snapping of the shaped line height
    let grew = larger / normal;
    let asked = larger_detail / normal_detail;
    assert!(
        (grew - asked).abs() < 0.05,
        "the diff grew {grew:.3}× for a {asked:.3}× larger step ({larger} vs {normal})"
    );
}
