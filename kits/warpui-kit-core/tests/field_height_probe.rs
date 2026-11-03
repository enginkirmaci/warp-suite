//! The family's field metric: a field box is the shared 36 px high, and the
//! kit `Input` inside it fills that box rather than the kit's own 24. `Sizable`
//! only stores the text size, so this is the guard that keeps every field the
//! same height.
//!
//! The box's look — the recessed `well` fill and the `line` edge — lives in
//! `warpui_kit_core::field`; a headless test cannot read painted colours, so
//! what is asserted here is the geometry the look is drawn on: the box's
//! height, the input filling it, and the title bar's one step shorter variant.

use gpui::prelude::*;
use gpui::{Context, Focusable as _, IntoElement, Render, TestAppContext, Window, div, px};
use gpui_kit::component::Sizable as _;
use gpui_kit::component::input::{Input, InputState};
use warpui_kit_core::{field, fonts, shell};

struct Form {
    bare: gpui::Entity<InputState>,
    family: gpui::Entity<InputState>,
    title: gpui::Entity<InputState>,
}

impl Render for Form {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let title_handle = self.title.read(cx).focus_handle(cx);
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .id("bare")
                    .debug_selector(|| "bare".into())
                    .child(Input::new(&self.bare).with_size(fonts::field_size())),
            )
            .child(field::text("family", &self.family, window, cx))
            .child(
                field::boxed("title", &title_handle, window, field::title_height())
                    .debug_selector(|| "title".into()),
            )
    }
}

#[gpui_kit::gpui::test]
fn a_family_field_is_the_shared_height(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (_root, cx) = cx.add_window_view(|window, cx| Form {
        bare: cx.new(|cx| InputState::new(window, cx)),
        family: cx.new(|cx| InputState::new(window, cx)),
        title: cx.new(|cx| InputState::new(window, cx)),
    });
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    let bare = cx.debug_bounds("bare").expect("the bare field draws");
    let family = cx.debug_bounds("family").expect("the family field draws");
    let title = cx.debug_bounds("title").expect("the title field draws");

    assert_eq!(
        family.size.height,
        px(shell::scaled(shell::FIELD_HEIGHT)),
        "the family field takes the shared height"
    );
    assert!(
        family.size.height > bare.size.height,
        "and it is taller than the kit's own default ({:?} vs {:?})",
        family.size.height,
        bare.size.height
    );
    // the width is the slot's, not the field's: both fill their column
    assert_eq!(family.size.width, bare.size.width);

    // the title bar's search is the design's 32, one step under a page field
    assert_eq!(title.size.height, px(shell::scaled(32.)));
    assert!(title.size.height < family.size.height);
}
