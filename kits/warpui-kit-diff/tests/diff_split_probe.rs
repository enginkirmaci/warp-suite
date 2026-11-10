//! The split view's layout and selection rules: two aligned columns, per-side
//! numbers and copy, empty fillers on the side without a row, and the width
//! floor that turns a narrow pane into horizontal scroll instead of crushed
//! columns.

use gpui::prelude::*;
use gpui::{Bounds, Context, Pixels, Render, TestAppContext, Window, div, point, px};
use warpui_kit_core::{AccentColors, active_tokens, apply_kit_theme, palette};
use warpui_kit_diff::{DiffMode, DiffOptions, render_diff_with};

const PAIR: &str = "\
--- a/x.rs
+++ b/x.rs
@@ -1,3 +1,3 @@
 fn a() {}
-fn b() {}
+fn b() { let x = 1; }
 fn c() {}
";

const FILLER: &str = "\
--- a/x.rs
+++ b/x.rs
@@ -1,4 +1,3 @@
 a
-b
-c
+x
 d
";

const TWO_FILES: &str = "\
diff --git a/x.rs b/x.rs
--- a/x.rs
+++ b/x.rs
@@ -1 +1 @@
-a
+b
diff --git a/y.rs b/y.rs
--- a/y.rs
+++ b/y.rs
@@ -1 +1 @@
-c
+d
";

const TRUNCATED: &str = "\
--- a/x.rs
+++ b/x.rs
@@ -1,2 +1,2 @@
 a
-b
+c
… (patch truncated)
";

struct SplitHost {
    patch: String,
    width: f32,
    mode: DiffMode,
}

impl Render for SplitHost {
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
                            wrap: false,
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
    f: impl FnOnce(&mut gpui::VisualTestContext) -> R,
) -> R {
    let (_root, vcx) = cx.add_window_view(|_, _| SplitHost {
        patch,
        width,
        mode: DiffMode::Split,
    });
    vcx.run_until_parked();
    vcx.update(|window, cx| drop(window.draw(cx)));
    f(vcx)
}

fn bounds(vcx: &mut gpui::VisualTestContext, selector: &'static str) -> Bounds<Pixels> {
    vcx.debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} was not drawn"))
}

#[gpui_kit::gpui::test]
fn the_columns_align_row_for_row(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(apply_theme);

    with_host(cx, PAIR.to_string(), 1000., |vcx| {
        let left_gutter = bounds(vcx, "diff-left-gutter");
        let right_gutter = bounds(vcx, "diff-right-gutter");
        let left = bounds(vcx, "diff-left-text");
        let right = bounds(vcx, "diff-right-text");

        assert_eq!(
            left_gutter.size.width, right_gutter.size.width,
            "both columns share one gutter width"
        );
        assert!(
            left.origin.x >= left_gutter.origin.x + left_gutter.size.width,
            "left code starts after its gutter: {left:?} vs {left_gutter:?}"
        );
        assert!(
            right.origin.x >= right_gutter.origin.x + right_gutter.size.width,
            "right code starts after its gutter: {right:?} vs {right_gutter:?}"
        );
        assert_eq!(left.origin.y, right.origin.y, "the columns start together");
        assert_eq!(
            left.size.height, right.size.height,
            "paired rows keep the same height"
        );
        assert!(
            bounds(vcx, "diff-split").size.width > px(900.),
            "a wide pane gives both columns room"
        );
    });
}

#[gpui_kit::gpui::test]
fn an_unpaired_row_leaves_a_filler(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(apply_theme);

    with_host(cx, FILLER.to_string(), 1000., |vcx| {
        let left = bounds(vcx, "diff-left-text");
        let right = bounds(vcx, "diff-right-text");
        assert_eq!(
            left.size.height, right.size.height,
            "the right side's filler keeps its line: 2 deletions vs 1 addition"
        );
        assert_eq!(left.origin.y, right.origin.y);
    });
}

#[gpui_kit::gpui::test]
fn an_unpaired_row_gets_a_hatch_filler(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(apply_theme);

    with_host(cx, FILLER.to_string(), 1000., |vcx| {
        let right_gutter = bounds(vcx, "diff-right-gutter");
        let right_text = bounds(vcx, "diff-right-text");
        let right_body = bounds(vcx, "diff-right-body");
        let row_height = right_text.size.height / 4.;
        let fill = bounds(vcx, "diff-right-fill-0");

        assert_eq!(
            fill.origin.x, right_gutter.origin.x,
            "the hatch starts at the column's leading edge, gutter included"
        );
        assert_eq!(
            fill.size.width, right_body.size.width,
            "and spans the whole empty column"
        );
        assert_eq!(
            fill.origin.y,
            right_text.origin.y + row_height * 2.,
            "it sits on the row whose line only exists on the left"
        );
        assert_eq!(
            fill.size.height, row_height,
            "one unpaired row is one hatch band"
        );
        assert!(
            vcx.debug_bounds("diff-left-fill-0").is_none(),
            "the side that has the line needs no filler"
        );
    });
}

#[gpui_kit::gpui::test]
fn consecutive_fillers_are_one_hatch_run(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(apply_theme);

    // one deletion against three additions: the left column has two blank
    // rows in a row, and the hatch must not restart between them.
    let patch = "--- a/x.rs\n+++ b/x.rs\n@@ -1,2 +1,4 @@\n a\n-b\n+c\n+d\n+e\n";
    with_host(cx, patch.to_string(), 1000., |vcx| {
        let left_text = bounds(vcx, "diff-left-text");
        let row_height = left_text.size.height / 4.;
        let fill = bounds(vcx, "diff-left-fill-0");
        assert_eq!(
            fill.size.height,
            row_height * 2.,
            "consecutive filler rows share one hatch band"
        );
        assert!(
            vcx.debug_bounds("diff-left-fill-1").is_none(),
            "the run is not split into a band per row"
        );
    });
}

#[gpui_kit::gpui::test]
fn a_trailing_filler_keeps_its_row(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(apply_theme);

    // the right side's last split row is a filler: the column must still end
    // on a real line, or its height drifts from the left column
    let patch = "--- a/x.rs\n+++ b/x.rs\n@@ -1,3 +1,2 @@\n a\n-b\n-c\n+x\n";
    with_host(cx, patch.to_string(), 1000., |vcx| {
        let left = bounds(vcx, "diff-left-text");
        let right = bounds(vcx, "diff-right-text");
        assert_eq!(
            left.size.height, right.size.height,
            "a trailing filler keeps the columns the same height"
        );
        assert!(
            right.size.height / 3. >= px(13.),
            "the filler is a real third row: {right:?}"
        );
    });
}

#[gpui_kit::gpui::test]
fn copy_stays_on_its_side(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(apply_theme);

    with_host(cx, PAIR.to_string(), 1000., |vcx| {
        let left = bounds(vcx, "diff-left-text");
        let right = bounds(vcx, "diff-right-text");
        let line_height = left.size.height / 3.;

        let start = point(left.origin.x + px(8.), left.origin.y + line_height * 1.5);
        let end = point(left.origin.x + px(160.), left.origin.y + line_height * 1.5);
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
                "left copy gives the old line: {selected:?}"
            );
            assert!(
                !selected.contains("let x = 1;"),
                "left copy never crosses to the addition: {selected:?}"
            );
        });

        let start = point(right.origin.x + px(8.), right.origin.y + line_height * 1.5);
        let end = point(
            right.origin.x + px(200.),
            right.origin.y + line_height * 1.5,
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
                selected.contains("let x = 1;"),
                "right copy gives the new line: {selected:?}"
            );
            assert!(
                !selected.contains("fn b() {}"),
                "right copy never crosses to the deletion: {selected:?}"
            );
        });
    });
}

#[gpui_kit::gpui::test]
fn a_long_line_does_not_wrap_in_split(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(apply_theme);

    let long = format!("+fn b() {{ let long = \"{}\"; }}", "a".repeat(200));
    let long_patch = format!("--- a/x.rs\n+++ b/x.rs\n@@ -1,2 +1,2 @@\n fn a() {{}}\n{long}\n");
    let short_patch = "--- a/x.rs\n+++ b/x.rs\n@@ -1,2 +1,2 @@\n fn a() {}\n+fn b() {}\n";
    let long_height = with_host(cx, long_patch, 1000., |vcx| {
        assert!(
            bounds(vcx, "diff-split").size.width <= px(1001.),
            "the split body stays the pane width"
        );
        assert!(
            bounds(vcx, "diff-right-body").size.width
                > bounds(vcx, "diff-right-viewport").size.width,
            "the long line overflows its own viewport, not the other column"
        );
        bounds(vcx, "diff-right-text").size.height
    });
    let short_height = with_host(cx, short_patch.to_string(), 1000., |vcx| {
        bounds(vcx, "diff-right-text").size.height
    });
    assert_eq!(
        long_height, short_height,
        "a 200-character line stays one row in split"
    );
}

#[gpui_kit::gpui::test]
fn the_default_changes_width_fits_both_columns(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(apply_theme);

    with_host(cx, PAIR.to_string(), 720., |vcx| {
        let body = bounds(vcx, "diff-split");
        assert!(
            body.size.width <= px(721.),
            "the split body fits a 720px pane: {body:?}"
        );
        let left = bounds(vcx, "diff-left-viewport");
        let right = bounds(vcx, "diff-right-viewport");
        assert!(
            (left.size.width - right.size.width).abs() <= px(0.5),
            "the columns split the pane evenly: {left:?} vs {right:?}"
        );
    });
}

#[gpui_kit::gpui::test]
fn a_narrow_pane_keeps_both_viewports_and_scrolls_each_side(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(apply_theme);

    let long = format!("+fn b() {{ let long = \"{}\"; }}", "a".repeat(200));
    let long_patch = format!("--- a/x.rs\n+++ b/x.rs\n@@ -1,2 +1,2 @@\n fn a() {{}}\n{long}\n");
    with_host(cx, long_patch, 600., |vcx| {
        assert_eq!(
            bounds(vcx, "diff-split").size.width,
            px(600.),
            "the split body fits the narrow pane"
        );
        let left = bounds(vcx, "diff-left-viewport");
        let right = bounds(vcx, "diff-right-viewport");
        assert_eq!(left.size.width, right.size.width, "equal columns");
        assert!(
            bounds(vcx, "diff-right-body").size.width > right.size.width,
            "the long side scrolls inside its own viewport"
        );
    });
}

#[gpui_kit::gpui::test]
fn a_truncated_split_falls_back_to_unified(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(apply_theme);

    with_host(cx, TRUNCATED.to_string(), 1000., |vcx| {
        assert!(
            vcx.debug_bounds("diff-text").is_some(),
            "the truncation state is drawn in the unified body"
        );
        assert!(
            vcx.debug_bounds("diff-left-text").is_none(),
            "no split column hides the truncation notice"
        );
    });
}

#[gpui_kit::gpui::test]
fn a_multi_file_split_falls_back_to_unified(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(apply_theme);

    let (_root, vcx) = cx.add_window_view(|_, _| SplitHost {
        patch: TWO_FILES.to_string(),
        width: 1000.,
        mode: DiffMode::Split,
    });
    vcx.run_until_parked();
    vcx.update(|window, cx| drop(window.draw(cx)));
    assert!(
        vcx.debug_bounds("diff-text").is_some(),
        "a multi-file patch keeps the unified body"
    );
    assert!(
        vcx.debug_bounds("diff-left-text").is_none(),
        "no split column is drawn for a multi-file patch"
    );
}
