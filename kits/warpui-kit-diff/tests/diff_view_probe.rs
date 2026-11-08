//! The unified diff's layout rules: one code column, one line per row,
//! metadata takes no rows, and the body is one selectable text so a drag can
//! cross lines and copy the patch content.

use gpui::prelude::*;
use gpui::{Bounds, Context, Pixels, Render, TestAppContext, Window, div, point, px};
use std::cell::Cell;
use std::rc::Rc;
use warpui_kit_core::{AccentColors, active_tokens, apply_kit_theme, palette};
use warpui_kit_diff::render_diff;

const ALIGN: &str = "\
--- a/x.rs
+++ b/x.rs
@@ -1,3 +1,3 @@
 fn a() {}
-fn b() {}
+fn b() { let x = 1; }
 fn c() {}
";

const BARE: &str = "\
--- a/x.rs
+++ b/x.rs
@@ -1 +1 @@
-a
+b
";

const WITH_META: &str = "\
diff --git a/x.rs b/x.rs
index 1111111..2222222 100644
--- a/x.rs
+++ b/x.rs
@@ -1 +1 @@
-a
+b
";

struct DiffHost {
    patch: String,
}

impl Render for DiffHost {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(gpui_kit::base::TextSelectionLayer)
            .child(div().w(px(600.)).flex_shrink_0().child(render_diff(
                &self.patch,
                Some("x.rs"),
                13.,
                window,
                cx,
            )))
    }
}

struct CountingHost {
    patch: String,
    renders: Rc<Cell<usize>>,
}

impl Render for CountingHost {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.renders.set(self.renders.get() + 1);
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(gpui_kit::base::TextSelectionLayer)
            .child(div().w(px(600.)).flex_shrink_0().child(render_diff(
                &self.patch,
                Some("x.rs"),
                13.,
                window,
                cx,
            )))
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
    f: impl FnOnce(&mut gpui::VisualTestContext) -> R,
) -> R {
    let (_root, vcx) = cx.add_window_view(|_, _| DiffHost { patch });
    vcx.run_until_parked();
    vcx.update(|window, cx| drop(window.draw(cx)));
    f(vcx)
}

fn bounds(vcx: &mut gpui::VisualTestContext, selector: &'static str) -> Bounds<Pixels> {
    vcx.debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} was not drawn"))
}

#[gpui_kit::gpui::test]
fn a_drag_crosses_lines_and_copies_the_code(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(apply_theme);

    with_host(cx, ALIGN.to_string(), |vcx| {
        let text = bounds(vcx, "diff-text");
        // one hunk strip plus four code rows (a, the deletion, the addition, c)
        let line_height = text.size.height / 5.;
        // the selectable text starts after the gutter and its left padding
        let start = point(text.origin.x + px(8.), text.origin.y + line_height * 2.5);
        let end = point(
            text.origin.x + px(8.) + px(160.),
            text.origin.y + line_height * 3.5,
        );
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
                selected.contains("fn b() {}"),
                "the selection starts on the deletion: {selected:?}"
            );
            assert!(
                selected.contains("let x = 1;"),
                "the selection reaches the addition: {selected:?}"
            );
            assert!(
                selected.contains('\n'),
                "the selection crosses the line break: {selected:?}"
            );
        });
    });
}

#[gpui_kit::gpui::test]
fn the_body_keeps_one_code_column_and_scrolls_long_lines(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(apply_theme);

    with_host(cx, ALIGN.to_string(), |vcx| {
        let text = bounds(vcx, "diff-text");
        let gutter = bounds(vcx, "diff-gutter");
        assert!(
            text.origin.x >= gutter.origin.x + gutter.size.width,
            "the code text starts after the gutter: {text:?} vs {gutter:?}"
        );
        assert_eq!(
            text.size.height,
            bounds(vcx, "diff-body").size.height,
            "the text sets the body height"
        );
    });

    let long = format!("+fn b() {{ let long = \"{}\"; }}", "a".repeat(200));
    let long_patch = format!("--- a/x.rs\n+++ b/x.rs\n@@ -1,2 +1,2 @@\n fn a() {{}}\n{long}\n");
    let short_patch = "--- a/x.rs\n+++ b/x.rs\n@@ -1,2 +1,2 @@\n fn a() {}\n+fn b() {}\n";
    let long_height = with_host(cx, long_patch, |vcx| {
        let body = bounds(vcx, "diff-body");
        assert!(
            body.size.width > px(600.),
            "the long line overflows the 600px pane: {:?}",
            body.size.width
        );
        bounds(vcx, "diff-text").size.height
    });
    let short_height = with_host(cx, short_patch.to_string(), |vcx| {
        bounds(vcx, "diff-text").size.height
    });
    assert_eq!(
        long_height, short_height,
        "a long line stays one row instead of wrapping"
    );
}

#[gpui_kit::gpui::test]
fn file_metadata_takes_no_rows(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(apply_theme);

    let bare = with_host(cx, BARE.to_string(), |vcx| {
        bounds(vcx, "diff-x-scroll").size.height
    });
    let with_meta = with_host(cx, WITH_META.to_string(), |vcx| {
        bounds(vcx, "diff-x-scroll").size.height
    });
    assert_eq!(
        with_meta, bare,
        "diff --git/index/---/+++ must not add rows"
    );
}

#[gpui_kit::gpui::test]
fn a_blank_source_line_keeps_its_row(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(apply_theme);

    let blank = "--- a/x.rs\n+++ b/x.rs\n@@ -1,3 +1,3 @@\n a\n \n-b\n+c\n";
    let plain = "--- a/x.rs\n+++ b/x.rs\n@@ -1,3 +1,3 @@\n a\n z\n-b\n+c\n";
    let blank_height = with_host(cx, blank.to_string(), |vcx| {
        bounds(vcx, "diff-text").size.height
    });
    let plain_height = with_host(cx, plain.to_string(), |vcx| {
        bounds(vcx, "diff-text").size.height
    });
    assert_eq!(
        blank_height, plain_height,
        "an empty source line still takes one row"
    );
}

#[gpui_kit::gpui::test]
fn a_drag_move_repaints_the_view(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(apply_theme);

    let renders = Rc::new(Cell::new(0));
    let (_root, vcx) = cx.add_window_view(|_, _| CountingHost {
        patch: ALIGN.to_string(),
        renders: renders.clone(),
    });
    vcx.run_until_parked();
    vcx.update(|window, cx| drop(window.draw(cx)));

    let text = bounds(vcx, "diff-text");
    let line_height = text.size.height / 5.;
    let start = point(text.origin.x + px(8.), text.origin.y + line_height * 1.5);
    vcx.simulate_mouse_down(start, gpui::MouseButton::Left, gpui::Modifiers::default());
    vcx.update(|window, cx| drop(window.draw(cx)));
    let before = renders.get();

    let moved = point(start.x + px(40.), start.y);
    vcx.simulate_mouse_move(
        moved,
        Some(gpui::MouseButton::Left),
        gpui::Modifiers::default(),
    );
    vcx.update(|window, cx| drop(window.draw(cx)));
    vcx.run_until_parked();
    vcx.update(|window, cx| drop(window.draw(cx)));
    assert!(
        renders.get() > before,
        "a drag move must repaint the selection"
    );
}
