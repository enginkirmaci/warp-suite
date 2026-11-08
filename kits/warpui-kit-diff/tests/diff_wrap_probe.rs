//! Word wrap: a wrapped body fits its pane, a long line takes more than one
//! visual line, and split keeps both columns and every row aligned.

use gpui::prelude::*;
use gpui::{Bounds, Context, Pixels, Render, TestAppContext, Window, div, point, px};
use warpui_kit_core::{AccentColors, active_tokens, apply_kit_theme, palette};
use warpui_kit_diff::{DiffMode, DiffOptions, render_diff_with};

const SHORT: &str = "\
--- a/x.rs
+++ b/x.rs
@@ -1,2 +1,2 @@
 fn a() {}
+fn b() {}
";

const LONG: &str = "\
--- a/x.rs
+++ b/x.rs
@@ -1,2 +1,2 @@
 fn a() {}
+fn b() { let x = \"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"; }
";

struct WrapHost {
    patch: String,
    width: f32,
    mode: DiffMode,
}

impl Render for WrapHost {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(gpui_kit::base::TextSelectionLayer)
            .child(
                div()
                    .w(px(self.width))
                    .flex_shrink_0()
                    .child(render_diff_with(
                        &self.patch,
                        Some("x.rs"),
                        DiffOptions {
                            mode: self.mode,
                            wrap: true,
                        },
                        13.,
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

fn with_host<R>(
    cx: &mut TestAppContext,
    patch: String,
    width: f32,
    mode: DiffMode,
    f: impl FnOnce(&mut gpui::VisualTestContext) -> R,
) -> R {
    let (_root, vcx) = cx.add_window_view(|_, _| WrapHost { patch, width, mode });
    vcx.run_until_parked();
    vcx.update(|window, cx| drop(window.draw(cx)));
    f(vcx)
}

fn bounds(vcx: &mut gpui::VisualTestContext, selector: &'static str) -> Bounds<Pixels> {
    vcx.debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} was not drawn"))
}

#[gpui_kit::gpui::test]
fn a_wrapped_unified_line_fits_the_pane(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(apply_theme);

    let short_height = with_host(cx, SHORT.to_string(), 600., DiffMode::Unified, |vcx| {
        assert!(
            vcx.debug_bounds("diff-x-scroll").is_none(),
            "a wrapped body needs no horizontal scroller"
        );
        bounds(vcx, "diff-wrap-body").size.height
    });
    let long_height = with_host(cx, LONG.to_string(), 600., DiffMode::Unified, |vcx| {
        let body = bounds(vcx, "diff-wrap-body");
        assert!(
            body.size.width <= px(601.),
            "the wrapped body fits the 600px pane: {body:?}"
        );
        body.size.height
    });
    let line_height = short_height / 2.;
    assert!(
        long_height >= short_height + line_height,
        "the long line wraps to at least one more row: {long_height} vs {short_height}"
    );
}

#[gpui_kit::gpui::test]
fn a_wrapped_split_keeps_both_columns_and_wraps_the_long_side(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(apply_theme);

    let short_height = with_host(cx, SHORT.to_string(), 600., DiffMode::Split, |vcx| {
        let body = bounds(vcx, "diff-wrap-body");
        assert!(
            body.size.width <= px(601.),
            "the split wrapped body fits the pane: {body:?}"
        );
        body.size.height
    });
    let long_height = with_host(cx, LONG.to_string(), 600., DiffMode::Split, |vcx| {
        let body = bounds(vcx, "diff-wrap-body");
        assert!(
            body.size.width <= px(601.),
            "the split wrapped body fits the pane: {body:?}"
        );
        assert!(
            vcx.debug_bounds("diff-x-scroll").is_none(),
            "wrap mode has no horizontal scroller"
        );
        body.size.height
    });
    let line_height = short_height / 2.;
    assert!(
        long_height >= short_height + line_height,
        "the split row grows to the taller wrapped side: {long_height} vs {short_height}"
    );
}

#[gpui_kit::gpui::test]
fn a_wrapped_drag_stays_in_its_row(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(apply_theme);

    let long = format!(" alpha{}", " alpha".repeat(60));
    let patch = format!("--- a/x.rs\n+++ b/x.rs\n@@ -1,2 +1,2 @@\n{long}\n-beta\n+gamma\n");
    with_host(cx, patch, 600., DiffMode::Unified, |vcx| {
        let row0 = bounds(vcx, "diff-wrap-code-0-u");
        let start = point(row0.origin.x + px(8.), row0.origin.y + px(8.));
        let end = point(row0.origin.x + px(260.), row0.origin.y + px(8.));
        vcx.simulate_mouse_down(start, gpui::MouseButton::Left, gpui::Modifiers::default());
        vcx.simulate_mouse_move(
            end,
            Some(gpui::MouseButton::Left),
            gpui::Modifiers::default(),
        );
        vcx.simulate_mouse_up(end, gpui::MouseButton::Left, gpui::Modifiers::default());
        vcx.update(|window, cx| {
            drop(window.draw(cx));
            let selected = gpui_kit::base::TextSelection::selected_text(window, cx);
            assert!(
                selected.contains("alpha"),
                "the drag selects the wrapped row: {selected:?}"
            );
            assert!(
                !selected.contains("beta") && !selected.contains("gamma"),
                "a wrapped drag does not cross into the next row: {selected:?}"
            );
        });
    });
}

#[gpui_kit::gpui::test]
fn a_trailing_wrapped_filler_keeps_its_row(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(apply_theme);

    // two deletions, one addition: the right side's last split row is a
    // filler, so the right column ends on an empty row
    let patch = "\
--- a/x.rs
+++ b/x.rs
@@ -1,3 +1,2 @@
 a
-b
-c
+x
";
    with_host(cx, patch.to_string(), 600., DiffMode::Split, |vcx| {
        let body = bounds(vcx, "diff-wrap-body");
        // three split rows, each at least a 13px text line; a dropped trailing
        // filler would leave only two
        assert!(
            body.size.height / 3. >= px(13.),
            "the trailing filler is a real third row: {body:?}"
        );
    });
}
