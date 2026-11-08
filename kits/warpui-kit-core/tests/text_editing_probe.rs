//! The family's text-editing contract: a field selects with the mouse, a
//! right click opens the field menu, and a read-only selectable surface gets
//! the shared Copy menu.
//!
//! The field half guards the `gpui-component` defaults the kit relies on; the
//! read-only half guards `warpui-kit-core::text_menu`. Both are headless, so a
//! dependency bump that drops either behavior fails CI instead of the app.

use std::ops::Deref as _;

use gpui::prelude::*;
use gpui::{
    Context, Entity, Focusable as _, IntoElement, Modifiers, MouseButton, Render, TestAppContext,
    VisualTestContext, Window, div, point, px,
};
use gpui_kit::component::input::{InputState, TextareaState};
use warpui_kit_core::field;
use warpui_kit_core::text_menu::{self, CopySelection, TextMenuAnchor};

// ---- single-line field ------------------------------------------------------

struct Form {
    state: Entity<InputState>,
}

impl Render for Form {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .p_6()
            .flex()
            .flex_col()
            .gap_4()
            .child(field::text("probe-field", &self.state, window, cx))
    }
}

fn open<'a>(
    cx: &'a mut TestAppContext,
    text: &str,
) -> (Entity<InputState>, &'a mut VisualTestContext) {
    cx.update(gpui_kit::init);
    let (view, cx) = cx.add_window_view(|window, cx| {
        let text = text.to_string();
        let state = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_value(text.as_str(), window, cx);
            state
        });
        Form { state }
    });
    let state = cx.read_entity(&view, |form, _| form.state.clone());
    cx.update(|window, cx| drop(window.draw(cx)));
    (state, cx)
}

fn drag(
    cx: &mut VisualTestContext,
    from: gpui::Point<gpui::Pixels>,
    to: gpui::Point<gpui::Pixels>,
) {
    cx.simulate_mouse_move(from, None, Modifiers::default());
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(to, Some(MouseButton::Left), Modifiers::default());
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::default());
    cx.run_until_parked();
}

#[gpui_kit::gpui::test]
fn a_drag_selects_text_in_a_kit_field(cx: &mut TestAppContext) {
    let (state, cx) = open(cx, "hello world");
    let bounds = cx.read_entity(&state, |state, _| state.input_bounds());
    let y = bounds.center().y;
    drag(
        cx,
        point(bounds.origin.x + px(2.), y),
        point(bounds.origin.x + px(80.), y),
    );
    let range = cx.read_entity(&state, |state, _| state.selected_range());
    assert_eq!(range, 0..10, "the drag covers the first ten bytes");
}

#[gpui_kit::gpui::test]
fn a_double_click_selects_a_word(cx: &mut TestAppContext) {
    let (state, cx) = open(cx, "hello world");
    let bounds = cx.read_entity(&state, |state, _| state.input_bounds());
    let at = point(bounds.origin.x + px(2.), bounds.center().y);
    cx.simulate_event(gpui::MouseDownEvent {
        position: at,
        modifiers: Modifiers::default(),
        button: MouseButton::Left,
        click_count: 2,
        first_mouse: false,
    });
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::default());
    cx.run_until_parked();
    let range = cx.read_entity(&state, |state, _| state.selected_range());
    assert_eq!(range, 0..5, "the double click selects 'hello'");
}

#[gpui_kit::gpui::test]
fn a_triple_click_selects_a_line(cx: &mut TestAppContext) {
    let (state, cx) = open(cx, "hello world");
    let bounds = cx.read_entity(&state, |state, _| state.input_bounds());
    let at = point(bounds.origin.x + px(2.), bounds.center().y);
    cx.simulate_event(gpui::MouseDownEvent {
        position: at,
        modifiers: Modifiers::default(),
        button: MouseButton::Left,
        click_count: 3,
        first_mouse: false,
    });
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::default());
    cx.run_until_parked();
    let range = cx.read_entity(&state, |state, _| state.selected_range());
    assert_eq!(range, 0..11, "the triple click selects the line");
}

#[gpui_kit::gpui::test]
fn the_box_padding_moves_the_caret_to_the_near_edge(cx: &mut TestAppContext) {
    let (state, cx) = open(cx, "hello world");
    let box_bounds = cx.debug_bounds("probe-field").expect("the field draws");
    let len = cx.read_entity(&state, |state, _| state.value().len());

    let left = point(box_bounds.origin.x + px(3.), box_bounds.center().y);
    cx.simulate_click(left, Modifiers::default());
    cx.run_until_parked();
    assert_eq!(
        cx.read_entity(&state, |state, _| state.cursor()),
        0,
        "the left inset puts the caret before the value"
    );

    let right = point(box_bounds.right() - px(3.), box_bounds.center().y);
    cx.simulate_click(right, Modifiers::default());
    cx.run_until_parked();
    assert_eq!(
        cx.read_entity(&state, |state, _| state.cursor()),
        len,
        "the right inset puts the caret after the value"
    );
}

#[gpui_kit::gpui::test]
fn a_right_click_opens_the_input_menu(cx: &mut TestAppContext) {
    use std::cell::RefCell;
    use std::rc::Rc;

    cx.update(gpui_kit::init);
    let slot: Rc<RefCell<Option<Entity<InputState>>>> = Rc::new(RefCell::new(None));
    let build_slot = slot.clone();
    let root = cx.add_window(move |window, cx| {
        let state = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_value("hello world", window, cx);
            state
        });
        *build_slot.borrow_mut() = Some(state.clone());
        let form = cx.new(|_| Form { state });
        gpui_kit::base::Root::new(form, window, cx)
    });
    let cx = VisualTestContext::from_window(*root.deref(), cx).into_mut();
    cx.run_until_parked();
    let state = slot.borrow().clone().expect("state built");

    let bounds = cx.read_entity(&state, |state, _| state.input_bounds());
    cx.simulate_click(bounds.center(), Modifiers::default());
    cx.run_until_parked();
    assert!(
        cx.update(|window, cx| state.read(cx).focus_handle(cx).is_focused(window)),
        "the press focuses the field"
    );

    let at = point(bounds.origin.x + px(30.), bounds.center().y);
    cx.simulate_mouse_down(at, MouseButton::Right, Modifiers::default());
    cx.simulate_mouse_up(at, MouseButton::Right, Modifiers::default());
    cx.run_until_parked();
    cx.run_until_parked();

    assert!(
        !cx.update(|window, cx| state.read(cx).focus_handle(cx).is_focused(window)),
        "the context menu took focus from the field"
    );
}

// ---- multi-line area --------------------------------------------------------

struct Area {
    state: Entity<TextareaState>,
}

impl Render for Area {
    fn render(&mut self, _: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().p_6().child(
            div()
                .debug_selector(|| "area-field".into())
                .h(px(120.))
                .w_full()
                .child(warpui_kit_core::field::bare_area(&self.state)),
        )
    }
}

#[gpui_kit::gpui::test]
fn a_drag_selects_text_in_a_bare_textarea(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (view, cx) = cx.add_window_view(|window, cx| {
        let state = cx.new(|cx| {
            let mut state = TextareaState::new(window, cx)
                .submit_on_enter(true)
                .auto_grow(2, 9);
            state.set_value("hello world\nsecond line", window, cx);
            state
        });
        Area { state }
    });
    let state = cx.read_entity(&view, |form, _| form.state.clone());
    cx.update(|window, cx| drop(window.draw(cx)));
    let bounds = cx.read_entity(&state, |state, _| state.input_bounds());
    let y = bounds.origin.y + px(6.);
    drag(
        cx,
        point(bounds.origin.x + px(2.), y),
        point(bounds.origin.x + px(80.), y),
    );
    let range = cx.read_entity(&state, |state, _| state.selected_range());
    assert_eq!(range, 0..10, "the area drag covers the first ten bytes");
}

// ---- read-only selectable text ---------------------------------------------

struct ReadOnly {
    anchor: Entity<TextMenuAnchor>,
}

impl Render for ReadOnly {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focus = self.anchor.read(cx).focus_handle();
        let view = gpui_kit::base::text::TextView::markdown(
            gpui::ElementId::Name("readonly-text".into()),
            "hello read only world",
        )
        .selectable(true);
        div()
            .relative()
            .size_full()
            .p_6()
            .child(gpui_kit::base::TextSelectionLayer)
            .child(self.anchor.clone())
            .child(
                div()
                    .debug_selector(|| "readonly-host".into())
                    .w(px(400.))
                    .child(text_menu::attach(
                        gpui::ElementId::Name("readonly-menu".into()),
                        view,
                        Some(focus),
                    )),
            )
    }
}

#[gpui_kit::gpui::test]
fn the_read_only_menu_copies_the_selection(cx: &mut TestAppContext) {
    use std::cell::RefCell;
    use std::rc::Rc;

    cx.update(gpui_kit::init);
    let slot: Rc<RefCell<Option<Entity<TextMenuAnchor>>>> = Rc::new(RefCell::new(None));
    let build_slot = slot.clone();
    let root = cx.add_window(move |window, cx| {
        let anchor = cx.new(TextMenuAnchor::new);
        *build_slot.borrow_mut() = Some(anchor.clone());
        let view = cx.new(|_| ReadOnly { anchor });
        gpui_kit::base::Root::new(view, window, cx)
    });
    let cx = VisualTestContext::from_window(*root.deref(), cx).into_mut();
    cx.run_until_parked();
    let anchor = slot.borrow().clone().expect("anchor built");
    let host = cx.debug_bounds("readonly-host").expect("the text draws");
    let y = host.origin.y + px(8.);
    drag(
        cx,
        point(host.origin.x + px(4.), y),
        point(host.origin.x + px(300.), y),
    );

    let selected = cx.update(gpui_kit::base::TextSelection::selected_text);
    assert!(
        !selected.is_empty(),
        "the drag selects the read-only text (got {selected:?})"
    );

    let at = point(host.origin.x + px(150.), y);
    let before = cx.update(|window, cx| window.focused(cx));
    cx.simulate_mouse_down(at, MouseButton::Right, Modifiers::default());
    cx.simulate_mouse_up(at, MouseButton::Right, Modifiers::default());
    cx.run_until_parked();
    let after = cx.update(|window, cx| window.focused(cx));
    assert!(after.is_some(), "the menu took focus");
    assert_ne!(before, after, "focus moved from the surface to the menu");

    // The menu item and the action anchor use the same action; dispatching it
    // directly proves the anchor half without driving the drawn menu.
    let focus = cx.read_entity(&anchor, |anchor, _| anchor.focus_handle());
    cx.update(|window, cx| {
        focus.dispatch_action(&CopySelection, window, cx);
    });
    cx.run_until_parked();
    let copied = cx
        .read_from_clipboard()
        .and_then(|item| item.text())
        .unwrap_or_default();
    assert_eq!(copied, selected, "the action copies the window selection");
}

// A parent that frames the input and swallows nothing: the app wrappers add
// press handlers around the field; this one is the smallest such shape.
struct Framed {
    state: Entity<InputState>,
    stop: bool,
}

impl Render for Framed {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.clone();
        let stop = self.stop;
        div()
            .size_full()
            .p_6()
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                let handle = state.read(cx).focus_handle(cx);
                window.focus(&handle, cx);
                if stop {
                    cx.stop_propagation();
                }
            })
            .child(field::text("framed-field", &self.state, window, cx))
    }
}

#[gpui_kit::gpui::test]
fn a_parent_press_handler_does_not_break_field_selection(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (view, cx) = cx.add_window_view(|window, cx| {
        let state = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_value("hello world", window, cx);
            state
        });
        Framed { state, stop: false }
    });
    let state = cx.read_entity(&view, |form, _| form.state.clone());
    cx.update(|window, cx| drop(window.draw(cx)));
    let bounds = cx.read_entity(&state, |state, _| state.input_bounds());
    let y = bounds.center().y;
    drag(
        cx,
        point(bounds.origin.x + px(2.), y),
        point(bounds.origin.x + px(80.), y),
    );
    let range = cx.read_entity(&state, |state, _| state.selected_range());
    assert_eq!(range, 0..10);
}

#[gpui_kit::gpui::test]
fn a_parent_that_stops_the_press_does_not_break_field_selection(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (view, cx) = cx.add_window_view(|window, cx| {
        let state = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_value("hello world", window, cx);
            state
        });
        Framed { state, stop: true }
    });
    let state = cx.read_entity(&view, |form, _| form.state.clone());
    cx.update(|window, cx| drop(window.draw(cx)));
    let bounds = cx.read_entity(&state, |state, _| state.input_bounds());
    let y = bounds.center().y;
    drag(
        cx,
        point(bounds.origin.x + px(2.), y),
        point(bounds.origin.x + px(80.), y),
    );
    let range = cx.read_entity(&state, |state, _| state.selected_range());
    assert_eq!(range, 0..10, "stopping the parent's press leaves the drag");
}

#[gpui_kit::gpui::test]
fn a_masked_field_does_not_copy_its_value(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (view, cx) = cx.add_window_view(|window, cx| {
        let state = cx.new(|cx| {
            let mut state = InputState::new(window, cx).masked(true);
            state.set_value("hunter2", window, cx);
            state
        });
        Form { state }
    });
    let state = cx.read_entity(&view, |form, _| form.state.clone());
    cx.update(|window, cx| drop(window.draw(cx)));
    cx.update(|window, cx| {
        state.update(cx, |state, cx| state.focus(window, cx));
    });
    cx.simulate_keystrokes("ctrl-a ctrl-c");
    cx.run_until_parked();
    assert_eq!(
        cx.read_entity(&state, |state, _| state.selected_range()),
        0..7,
        "ctrl-a selects the masked value first"
    );
    let copied = cx
        .read_from_clipboard()
        .and_then(|item| item.text())
        .unwrap_or_default();
    assert!(
        !copied.contains("hunter2"),
        "a masked field never puts its value on the clipboard (got {copied:?})"
    );
}
