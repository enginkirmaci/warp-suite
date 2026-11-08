//! Headless probe for the commit message well: the box keeps the card's row
//! height and shows the whole draft, rather than collapsing to one row.

use gpui::prelude::*;
use gpui::{
    Context, Entity, Focusable, IntoElement, Modifiers, MouseButton, Render, TestAppContext,
    VisualTestContext, Window, div, point, px,
};
use gpui_kit::component::input::TextareaState;
use warpui_kit_core::field;

const SHORT_DRAFT: &str = "fix(ui): keep the message box border neutral while focused";

struct Probe {
    state: Entity<TextareaState>,
}

impl Render for Probe {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(320.))
            .h(px(190.))
            .p_3()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div().flex_1().min_h_0().relative().child(
                    field::area("commit-message", &self.state, window, cx)
                        .h_full()
                        .min_h(px(0.)),
                ),
            )
            .child(
                div()
                    .debug_selector(|| "probe-button".into())
                    .flex_shrink_0()
                    .w_full()
                    .h(px(36.)),
            )
    }
}

/// Draws the probe with `draft` already in the box, the caret at its end the
/// way typing leaves it.
fn open<'a>(
    cx: &'a mut TestAppContext,
    draft: &'static str,
) -> (Entity<TextareaState>, &'a mut VisualTestContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let state = cx.new(|cx| TextareaState::new(window, cx));
        state.update(cx, |s, cx| s.set_value(draft, window, cx));
        Probe { state }
    });
    let state = cx.read_entity(&view, |p, _| p.state.clone());
    cx.update(|window, cx| {
        let handle = state.read(cx).focus_handle(cx);
        handle.focus(window, cx);
    });
    cx.simulate_keystrokes("ctrl-end");
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    (state, cx)
}

fn assert_inside_the_row(area: gpui::Bounds<gpui::Pixels>, button: gpui::Bounds<gpui::Pixels>) {
    assert_eq!(
        area.size.height,
        px(122.),
        "the well keeps the card's row, not the draft's height"
    );
    assert!(
        area.bottom() <= button.top(),
        "the well ends above the button: well {area:?}, button {button:?}"
    );
}

fn assert_caret_visible(state: &Entity<TextareaState>, cx: &mut VisualTestContext) {
    cx.read_entity(state, |s, _| {
        let input = s.input_bounds();
        let (caret, _) = s.cursor_layout().expect("caret laid out");
        let scroll = s.scroll_offset();
        let top = caret.top() + scroll.y;
        let bottom = caret.bottom() + scroll.y;
        assert!(
            top >= input.top() && bottom <= input.bottom(),
            "the caret is inside the visible well: caret {top:?}..{bottom:?}, input {input:?}"
        );
        assert_eq!(
            s.selected_range().end,
            s.value().len(),
            "the caret sits at the end of the draft"
        );
    });
}

#[gpui_kit::gpui::test]
fn commit_message_area_shows_the_full_draft(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let (state, cx) = open(cx, SHORT_DRAFT);
    let area = cx.debug_bounds("commit-message").expect("the well draws");
    let button = cx
        .debug_bounds("probe-button")
        .expect("the button row draws");
    assert_inside_the_row(area, button);

    cx.read_entity(&state, |s, _| {
        let line_height = s.line_height().expect("laid out");
        let input = s.input_bounds();
        assert!(
            input.size.height >= line_height * 2.,
            "the well is taller than one row: {} vs {line_height}",
            input.size.height
        );
        assert_eq!(
            s.display_map().wrap_row_count(),
            2,
            "the subject wraps onto two rows"
        );
        assert_eq!(
            s.scroll_offset().y,
            px(0.),
            "a draft that fits never scrolls off line one"
        );
    });
    assert_caret_visible(&state, cx);
}

/// The commit well is a `field::area` inside the card's own row; the frame
/// must not swallow the drag that the textarea needs for selection.
#[gpui_kit::gpui::test]
fn a_drag_selects_in_the_commit_message(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let (state, cx) = open(cx, SHORT_DRAFT);
    let input = cx.read_entity(&state, |state, _| state.input_bounds());
    let y = input.origin.y + px(6.);
    let from = point(input.origin.x + px(2.), y);
    let to = point(input.origin.x + px(140.), y);
    cx.simulate_mouse_move(from, None, Modifiers::default());
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(to, Some(MouseButton::Left), Modifiers::default());
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::default());
    cx.run_until_parked();

    let selected = cx.read_entity(&state, |state, _| {
        state.value()[state.selected_range()].to_string()
    });
    assert!(
        !selected.is_empty(),
        "the drag selects part of the commit draft"
    );
}
