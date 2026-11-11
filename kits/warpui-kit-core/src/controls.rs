//! The family's small control shapes: the one button look, the one
//! popover/menu chrome, the one fold body, plus the two media helpers that
//! were being re-stated per app. They came out of warp-agent's `ui::controls`
//! once warp-station and warp-notes grew their own copies of the same shapes.
//!
//! Everything here draws through [`crate::palette`]'s live accessors, so a
//! palette switch moves these with the rest of the family.

use crate::palette;
use crate::scroll;
use crate::shell;
use gpui::prelude::*;
use gpui::{
    AnyElement, App, ClickEvent, Div, ElementId, FontWeight, Role, ScrollHandle, SharedString,
    Window, div, px,
};

/// The button looks: the accent fill, and the two quiet ones, which are the
/// design's one outlined control. Primary carries the preset's on-accent text
/// color.
pub enum BtnKind {
    Primary(gpui::Rgba),
    Chip,
    Outline,
}

/// The shared button, at the shell's control height. Hover dims Primary to
/// 0.9 and lifts the quiet ones to the raised step; `enabled=false` parks the
/// button at half opacity with no hover (the click handler stays wired —
/// callers that need to ignore the click guard it in the closure).
pub fn button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    kind: BtnKind,
    accent: gpui::Rgba,
    text_size: f32,
    enabled: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let label: SharedString = label.into();
    let mut el = div()
        .id(id.into())
        .role(Role::Button)
        .aria_label(label.clone())
        .flex_shrink_0()
        .h(shell::control_height())
        .px_3()
        .flex()
        .items_center()
        .rounded(px(6.))
        .text_size(px(text_size))
        .cursor_pointer();
    match kind {
        BtnKind::Primary(on_accent) => {
            el = el
                .font_weight(FontWeight::SEMIBOLD)
                .bg(accent)
                .text_color(on_accent);
        }
        BtnKind::Chip | BtnKind::Outline => {
            el = el
                .font_weight(FontWeight::MEDIUM)
                .border_1()
                .border_color(palette::line_strong())
                .bg(palette::panel())
                .text_color(palette::text());
        }
    }
    el.when(enabled, |d| match kind {
        BtnKind::Primary(_) => d.hover(|s| s.opacity(0.9)),
        BtnKind::Chip | BtnKind::Outline => d.hover(|s| s.bg(palette::raised())),
    })
    .when(!enabled, |d| d.opacity(0.5))
    .on_click(on_click)
    .child(label)
    .into_any_element()
}

/// One context-menu row; destructive rows render in the danger color.
pub fn menu_row(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    destructive: bool,
    text_size: f32,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let label: SharedString = label.into();
    div()
        .id(id.into())
        .role(Role::MenuItem)
        .aria_label(label.clone())
        .px_3()
        .py_1p5()
        .rounded_md()
        .text_size(px(text_size))
        .cursor_pointer()
        .when(destructive, |d| d.text_color(palette::DANGER))
        .when(!destructive, |d| d.text_color(palette::text()))
        .hover(|s| s.bg(palette::HOVER))
        .on_click(on_click)
        .child(label)
        .into_any_element()
}

/// The head's quiet icon verb: a square on the panel step at the control
/// height, edged like every outlined control, lifted to `raised()` on hover.
/// Returns a stateful div so the caller adds its debug selector, its glyph and
/// its click.
pub fn icon_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
) -> gpui::Stateful<Div> {
    let label: SharedString = label.into();
    div()
        .id(id.into())
        .role(Role::Button)
        .aria_label(label)
        .cursor_pointer()
        .flex_shrink_0()
        .size(shell::control_height())
        .rounded(px(6.))
        .border_1()
        .border_color(palette::line_strong())
        .bg(palette::panel())
        .flex()
        .items_center()
        .justify_center()
        .hover(|d| d.bg(palette::raised()))
}

/// The shared popover card chrome: the design's menu edge, overlay bg, soft
/// shadow. Returns a `Div` so callers keep chaining (p_2, min_w, children).
pub fn popover_card() -> Div {
    div()
        .rounded(px(8.))
        .border_1()
        .border_color(palette::line_strong())
        .bg(palette::popup())
        .p_1()
        .shadow_lg()
}

/// The collapsible fold body: inset panel, 260px cap, scrollable. Returns a
/// stateful div so callers add their text styling.
///
/// The body is a scroller nested inside its host (a fold in a chat sits in the
/// transcript's list), so it takes the wheel step itself while it has content
/// left and lets the step chain out only at its edge — see [`crate::scroll`].
/// `scroll` is the caller's per-fold offset; keep one handle per fold so two
/// open folds do not share a position.
pub fn fold_body(id: impl Into<ElementId>, scroll: &ScrollHandle) -> gpui::Stateful<Div> {
    let handle = scroll.clone();
    div()
        .id(id.into())
        .rounded(px(6.))
        .bg(palette::well())
        .p_2()
        .max_h(px(260.))
        .overflow_y_scroll()
        .track_scroll(scroll)
        .on_scroll(move |event, window, cx| scroll::consume_wheel(&handle, event, window, cx))
}
