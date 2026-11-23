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
use gpui_kit::component::menu::PopupMenuItem;

/// The button looks: the accent fill, and the two quiet ones, which are the
/// design's one outlined control. Primary carries the accent's on-accent text
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
        // One weight for every button: a filled primary is louder by its fill,
        // not by a heavier label. A semibold primary beside a medium chip read
        // as a bigger control, which is what the About head showed.
        BtnKind::Primary(on_accent) => {
            el = el
                .font_weight(FontWeight::MEDIUM)
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
///
/// `keys` is the shortcut the app bound for the row, printed at the row's
/// right edge as a keycap. Pass `None` for a row with no shortcut: the row
/// must not invent a key the app does not answer to.
pub fn menu_row(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    keys: Option<SharedString>,
    destructive: bool,
    text_size: f32,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    menu_row_inner(
        id,
        label,
        None,
        keys,
        destructive,
        text_size,
        on_click,
    )
}

/// [`menu_row`] with the design's 16 px leading icon slot, for a menu whose
/// rows are things rather than verbs (a run's folder, the repository it lives
/// in). `ink` is the glyph's colour, usually the row's own text step.
#[expect(
    clippy::too_many_arguments,
    reason = "a menu row is id, label, glyph, ink, keycap, tone, size and action; the plain menu_row keeps the short call for the common case"
)]
pub fn menu_row_with_icon(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    glyph: &'static str,
    ink: gpui::Rgba,
    keys: Option<SharedString>,
    destructive: bool,
    text_size: f32,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    menu_row_inner(
        id,
        label,
        Some((glyph, ink)),
        keys,
        destructive,
        text_size,
        on_click,
    )
}

fn menu_row_inner(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    leading: Option<(&'static str, gpui::Rgba)>,
    keys: Option<SharedString>,
    destructive: bool,
    text_size: f32,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let id: ElementId = id.into();
    let selector = id.to_string();
    let label: SharedString = label.into();
    div()
        .id(id)
        .debug_selector(move || selector.clone())
        .role(Role::MenuItem)
        .aria_label(label.clone())
        .px_3()
        .py_1p5()
        .rounded_md()
        .text_size(px(text_size))
        .cursor_pointer()
        .flex()
        .items_center()
        .justify_between()
        .gap_3()
        .when(destructive, |d| d.text_color(palette::DANGER))
        .when(!destructive, |d| d.text_color(palette::text()))
        .hover(|s| s.bg(palette::HOVER))
        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
            cx.stop_propagation()
        })
        .on_click(on_click)
        .children(leading.map(|(glyph, ink)| {
            div()
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .w(px(16.))
                .h(px(16.))
                .child(crate::icon::icon(glyph, px(16.), ink))
        }))
        .child(div().min_w_0().flex_1().truncate().child(label))
        .children(keys.map(menu_keycap))
        .into_any_element()
}

/// One `PopupMenu` row with an optional right-aligned keycap.
///
/// `gpui-component`'s [`PopupMenuItem`] prints a keycap only for an item bound
/// to an action it can resolve a keybinding for. A menu that runs its rows
/// through `on_click` (the read-only text menu, a host's own rows) gets the
/// same column from this instead, with the shortcut the host actually binds.
pub fn popup_menu_item(
    label: impl Into<SharedString>,
    keys: Option<SharedString>,
    disabled: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> PopupMenuItem {
    let label: SharedString = label.into();
    PopupMenuItem::element(move |_, _| {
        let selector = label.clone();
        let mut row = div()
            .id(ElementId::Name(format!("menu-item-{label}").into()))
            .debug_selector(move || format!("menu-row-{selector}"))
            .w_full()
            .min_w_0()
            .flex()
            .items_center()
            .justify_between()
            .gap_3()
            // An `ElementItem` carries no accessible name of its own, so the
            // row names itself: the screen reader still reads one menuitem.
            .role(Role::MenuItem)
            .aria_label(label.clone())
            .child(div().min_w_0().truncate().child(label.clone()));
        if let Some(keys) = keys.clone() {
            row = row.child(menu_keycap(keys));
        }
        row
    })
    .disabled(disabled)
    .on_click(on_click)
}

/// The keycap at a menu row's right edge, tagged with its own key so a probe
/// can assert the shortcut a row prints.
fn menu_keycap(keys: SharedString) -> AnyElement {
    div()
        .flex_shrink_0()
        .debug_selector({
            let keys = keys.clone();
            move || format!("menu-key-{keys}")
        })
        .child(crate::field::kbd(keys))
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
