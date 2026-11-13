//! The family's keyboard-reference card.
//!
//! One occluding dialog over the caller's root: a title, a scrolling body of
//! groups (a mono key column and a plain description per row) and a close
//! button. The content belongs to the app — callers pass the groups they
//! actually bind — so the sheet never grows a second key map.
//!
//! The card takes a [`FocusHandle`] it tracks, so opening it can move the
//! keyboard off the list underneath: a list must not move its cursor while
//! the sheet is up. `Escape` and the scrim press are the caller's close path;
//! the card only renders and stops presses from reaching what is under it.

use gpui::prelude::*;
use gpui::{
    AnyElement, ClickEvent, ElementId, FocusHandle, FontWeight, MouseButton, Role, SharedString,
    Window, div, px, relative,
};

use crate::controls::{BtnKind, button};
use crate::field::kbd;
use crate::fonts;
use crate::palette;
use crate::shell;

/// One heading and its key rows: `(keys, description)`.
pub struct Group {
    pub title: SharedString,
    pub rows: Vec<(SharedString, SharedString)>,
}

impl Group {
    pub fn new(title: impl Into<SharedString>, rows: Vec<(SharedString, SharedString)>) -> Self {
        Self {
            title: title.into(),
            rows,
        }
    }
}

/// The sheet card. `focus` is tracked on the card and focused by the caller
/// when it opens; `on_close` runs for the close button and a press on the
/// scrim.
pub fn dialog(
    id: impl Into<ElementId>,
    title: impl Into<SharedString>,
    groups: &[Group],
    focus: &FocusHandle,
    on_close: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> AnyElement {
    let id = id.into();
    let title = title.into();
    let on_close = std::rc::Rc::new(on_close);
    let close_button = on_close.clone();
    let close_scrim = on_close;

    let mut body = div()
        .id("shortcut-sheet-list")
        .debug_selector(|| "shortcut-sheet-list".to_string())
        .flex()
        .flex_col()
        .gap(px(shell::scaled(2.)))
        .px_4()
        .py_2()
        .overflow_y_scroll();
    for group in groups {
        body = body.child(
            div()
                .flex_shrink_0()
                .pt_2()
                .pb_0p5()
                .px_2()
                .text_size(px(fonts::font_meta()))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(palette::text_muted())
                .child(group.title.clone()),
        );
        for (keys, label) in &group.rows {
            body = body.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .min_h(px(shell::scaled(26.)))
                    .px_2()
                    .rounded(px(4.))
                    .child(
                        div()
                            .w(px(shell::scaled(156.)))
                            .flex_shrink_0()
                            .font_family(SharedString::from(fonts::code_font()))
                            .text_size(px(fonts::font_mono_meta()))
                            .text_color(palette::text_dim())
                            .child(keys.clone()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(fonts::font_body()))
                            .text_color(palette::text())
                            .child(label.clone()),
                    ),
            );
        }
    }

    div()
        .id(id)
        .debug_selector(|| "shortcut-sheet-dialog".to_string())
        .role(Role::Dialog)
        .aria_label(title.clone())
        .absolute()
        .inset_0()
        .occlude()
        .bg(palette::SCRIM)
        .flex()
        .items_center()
        .justify_center()
        .p_6()
        .on_click(move |event, window, cx| close_scrim(event, window, cx))
        .child(
            div()
                .id("shortcut-sheet-card")
                .debug_selector(|| "shortcut-sheet-card".to_string())
                .track_focus(focus)
                .occlude()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .w(px(shell::scaled(720.)))
                .max_w(relative(1.))
                .max_h(relative(0.92))
                .flex()
                .flex_col()
                .rounded(px(10.))
                .border_1()
                .border_color(palette::line_strong())
                .bg(palette::popup())
                .shadow_lg()
                .child(
                    div()
                        .flex_shrink_0()
                        .h(px(shell::scaled(52.)))
                        .px_4()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .border_b_1()
                        .border_color(palette::hairline())
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_size(px(fonts::font_brand()))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(palette::text())
                                .child(title.clone()),
                        )
                        .child(
                            div()
                                .debug_selector(|| "shortcut-sheet-close".to_string())
                                .flex_shrink_0()
                                .child(button(
                                    ElementId::Name("shortcut-sheet-close-btn".into()),
                                    "Close",
                                    BtnKind::Chip,
                                    palette::accent(),
                                    fonts::font_control(),
                                    true,
                                    move |event, window, cx| close_button(event, window, cx),
                                )),
                        ),
                )
                .child(body.flex_1().min_h(px(0.)))
                .child(
                    div()
                        .flex_shrink_0()
                        .h(px(shell::scaled(44.)))
                        .px_4()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .border_t_1()
                        .border_color(palette::hairline())
                        .text_size(px(fonts::font_meta()))
                        .text_color(palette::text_faint())
                        .child(div().flex_1().child("Every key the window answers to."))
                        .child(kbd("Esc")),
                ),
        )
        .into_any_element()
}
