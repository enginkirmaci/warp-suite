//! Scrolling the scrolled diff: a horizontal wheel step moves only the
//! horizontal viewport, a vertical step moves only the vertical one, and a
//! wrapped body renders only the rows near its viewport.

use gpui::prelude::*;
use gpui::{
    Context, Entity, Modifiers, MouseButton, Pixels, Render, ScrollDelta, ScrollEvent,
    ScrollSource, TestAppContext, TouchPhase, Window, div, point, px,
};
use warpui_kit_core::{AccentColors, active_tokens, apply_kit_theme, palette};
use warpui_kit_diff::{DiffMode, DiffOptions, DiffScrollHandles, render_diff_with_scroll};

struct ScrollHost {
    patch: String,
    mode: DiffMode,
    wrap: bool,
    scroll: DiffScrollHandles,
}

impl Render for ScrollHost {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(gpui_kit::base::TextSelectionLayer)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .child(render_diff_with_scroll(
                        &self.patch,
                        Some("x.rs"),
                        DiffOptions {
                            mode: self.mode,
                            wrap: self.wrap,
                        },
                        &self.scroll,
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
    mode: DiffMode,
    wrap: bool,
    f: impl FnOnce(&mut gpui::VisualTestContext, Entity<ScrollHost>) -> R,
) -> R {
    let (root, vcx) = cx.add_window_view(|_, _| ScrollHost {
        patch,
        mode,
        wrap,
        scroll: DiffScrollHandles::default(),
    });
    vcx.run_until_parked();
    vcx.update(|window, cx| drop(window.draw(cx)));
    f(vcx, root)
}

fn wheel(vcx: &mut gpui::VisualTestContext, position: gpui::Point<Pixels>, delta: ScrollDelta) {
    vcx.simulate_event(ScrollEvent {
        position,
        delta,
        modifiers: Modifiers::default(),
        touch_phase: TouchPhase::Moved,
        source: ScrollSource::Pointer,
    });
    vcx.update(|window, cx| drop(window.draw(cx)));
}

fn center(vcx: &mut gpui::VisualTestContext, selector: &'static str) -> gpui::Point<Pixels> {
    let bounds = vcx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} was not drawn"));
    bounds.center()
}

fn near_top(vcx: &mut gpui::VisualTestContext, selector: &'static str) -> gpui::Point<Pixels> {
    let bounds = vcx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} was not drawn"));
    bounds.origin + point(px(40.), px(40.))
}

/// A patch with a line far wider than the pane and plenty of rows to scroll.
fn tall_wide_patch() -> String {
    let mut patch = String::from("--- a/x.rs\n+++ b/x.rs\n@@ -1,300 +1,300 @@\n");
    for index in 0..300 {
        if index == 0 {
            patch.push_str(&format!(
                "+fn wide() {{ let x = \"{}\"; }}\n",
                "a".repeat(400)
            ));
        } else {
            patch.push_str(&format!("+fn row_{index}() {{}}\n"));
        }
    }
    patch
}

/// A patch whose rows wrap; a high row's selector proves the body did not
/// build every row up front.
fn wrapped_patch(rows: usize) -> String {
    let mut patch = String::from("--- a/x.rs\n+++ b/x.rs\n");
    patch.push_str(&format!("@@ -1,{rows} +1,{rows} @@\n"));
    for index in 0..rows {
        patch.push_str(&format!("+row {index}: {}\n", "word ".repeat(30)));
    }
    patch
}

#[gpui_kit::gpui::test]
fn a_horizontal_wheel_moves_only_the_horizontal_viewport(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(apply_theme);

    with_host(
        cx,
        tall_wide_patch(),
        DiffMode::Unified,
        false,
        |vcx, host| {
            let at = near_top(vcx, "diff-text");
            wheel(vcx, at, ScrollDelta::Pixels(point(px(0.), px(-80.))));
            let scroll = vcx.update(|_, cx| host.read(cx).scroll.clone());
            assert!(
                scroll.vertical.offset().y < px(0.),
                "the vertical step moved the body: {:?}",
                scroll.vertical.offset()
            );

            let y = scroll.vertical.offset().y;
            wheel(vcx, at, ScrollDelta::Pixels(point(px(-120.), px(0.))));
            let scroll = vcx.update(|_, cx| host.read(cx).scroll.clone());
            assert!(
                scroll.unified.offset().x < px(0.),
                "the horizontal step moved the code: {:?}",
                scroll.unified.offset()
            );
            assert_eq!(
                scroll.vertical.offset().y,
                y,
                "the horizontal step must not move the body vertically"
            );
        },
    );
}

#[gpui_kit::gpui::test]
fn a_wrapped_list_rows_stay_selectable(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(apply_theme);

    with_host(
        cx,
        wrapped_patch(3),
        DiffMode::Unified,
        true,
        |vcx, _host| {
        let row = vcx
            .debug_bounds("diff-wrap-code-0-u")
            .expect("the first wrapped row");
        assert!(
            row.size.height > px(20.),
            "the list row wraps at the pane width: {row:?}"
        );
            let start = point(row.origin.x + px(8.), row.origin.y + px(8.));
            let end = point(row.origin.x + px(260.), row.origin.y + px(8.));
            vcx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
            vcx.simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::default());
            vcx.simulate_mouse_up(end, MouseButton::Left, Modifiers::default());
            vcx.update(|window, cx| {
                drop(window.draw(cx));
                let selected = gpui_kit::base::TextSelection::selected_text(window, cx);
                assert!(
                    selected.contains("row 0"),
                    "the list row is selectable: {selected:?}"
                );
                assert!(
                    !selected.contains("row 1"),
                    "the drag stayed in its row: {selected:?}"
                );
            });
        },
    );
}

#[gpui_kit::gpui::test]
fn a_horizontal_wheel_in_split_moves_the_column_under_the_pointer(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(apply_theme);

    with_host(
        cx,
        tall_wide_patch(),
        DiffMode::Split,
        false,
        |vcx, host| {
            let at = near_top(vcx, "diff-left-text");
            wheel(vcx, at, ScrollDelta::Pixels(point(px(-120.), px(0.))));
            let scroll = vcx.update(|_, cx| host.read(cx).scroll.clone());
            assert!(
                scroll.left.offset().x < px(0.),
                "the left column scrolled: {:?}",
                scroll.left.offset()
            );
            assert_eq!(
                scroll.right.offset().x,
                px(0.),
                "the right column stayed put"
            );
            assert_eq!(
                scroll.vertical.offset().y,
                px(0.),
                "a horizontal step did not scroll the shared body"
            );
        },
    );
}

#[gpui_kit::gpui::test]
fn a_wrapped_body_draws_only_rows_near_its_viewport(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(apply_theme);

    with_host(
        cx,
        wrapped_patch(400),
        DiffMode::Unified,
        true,
        |vcx, _host| {
            assert!(
                vcx.debug_bounds("diff-wrap-code-0-u").is_some(),
                "the first row is drawn"
            );
            assert!(
                vcx.debug_bounds("diff-wrap-code-399-u").is_none(),
                "the last row is not drawn before scrolling"
            );

            let at = center(vcx, "diff-wrap-code-0-u");
            for _ in 0..40 {
                wheel(vcx, at, ScrollDelta::Pixels(point(px(0.), px(-120.))));
            }

            assert!(
                vcx.debug_bounds("diff-wrap-code-0-u").is_none(),
                "the first row left the viewport"
            );
            assert!(
                vcx.debug_bounds("diff-wrap-code-399-u").is_none(),
                "the body still does not build every row"
            );
        },
    );
}
