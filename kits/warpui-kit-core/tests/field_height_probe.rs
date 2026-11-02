//! The family's field metric: a kit `Input` sized with `field_size()` draws at
//! the shared 36 px height, not the kit's own 24. `Sizable` only stores the
//! text size, so this is the guard that keeps every field the same height.

use gpui::prelude::*;
use gpui::{Context, IntoElement, Render, TestAppContext, Window, div, px};
use gpui_kit::component::Sizable as _;
use gpui_kit::component::input::{Input, InputState};
use warpui_kit_core::shell;

struct Form {
    bare: gpui::Entity<InputState>,
    family: gpui::Entity<InputState>,
}

impl Render for Form {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .id("bare")
                    .debug_selector(|| "bare".into())
                    .child(Input::new(&self.bare).with_size(warpui_kit_core::fonts::field_size())),
            )
            .child(
                shell::field(
                    "family",
                    Input::new(&self.family).with_size(warpui_kit_core::fonts::field_size()),
                )
                .debug_selector(|| "family".into()),
            )
    }
}

#[gpui_kit::gpui::test]
fn a_family_field_is_the_shared_height(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (_root, cx) = cx.add_window_view(|window, cx| Form {
        bare: cx.new(|cx| InputState::new(window, cx)),
        family: cx.new(|cx| InputState::new(window, cx)),
    });
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    let bare = cx.debug_bounds("bare").expect("the bare field draws");
    let family = cx.debug_bounds("family").expect("the family field draws");

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
}
