//! The one window shape: every screen of every app is a rail, then a list beside
//! a detail pane, under a title bar. Detail appears nowhere else, so there is no
//! inspector and no bottom sheet to build, and what used to be a panel is a tab.
//!
//! These are plain elements, no state: an app chains its own content, ids and
//! listeners onto them. Widths and heights ride the text size, so a rail label
//! at Larger still fits.
//!
//! ```text
//! title_bar
//! rail | stage
//!      |   tab_strip (optional)
//!      |   panes
//!      |     list_pane | detail_pane
//! ```

use gpui::prelude::*;
use gpui::{Div, ElementId, FontWeight, SharedString, Stateful, div, px};

use crate::a11y::A11y as _;
use crate::icon::icon;
use crate::{fonts, palette};

pub const TITLE_BAR_HEIGHT: f32 = 46.;
pub const RAIL_WIDTH: f32 = 76.;
pub const TAB_STRIP_HEIGHT: f32 = 40.;
/// The list's default width; a drag moves it between the two bounds.
pub const LIST_WIDTH: f32 = 432.;
pub const LIST_MIN_WIDTH: f32 = 320.;
pub const LIST_MAX_WIDTH: f32 = 560.;
/// The detail pane's padding.
pub const DETAIL_PADDING: f32 = 24.;
/// The widest running text reads at.
pub const READING_WIDTH: f32 = 760.;
/// The widest a grid of cards goes.
pub const GRID_WIDTH: f32 = 1180.;
/// The primary fill's control height, and every button's.
pub const CONTROL_HEIGHT: f32 = 34.;
pub const FIELD_HEIGHT: f32 = 36.;
pub const DETAIL_TAB_HEIGHT: f32 = 42.;

/// Below this window width a wide tab hides the list on its own.
pub const COMPACT_WINDOW_WIDTH: f32 = 1180.;

const STRIP_TAB_HEIGHT: f32 = 35.;
const STRIP_TAB_MIN_WIDTH: f32 = 120.;
const STRIP_TAB_MAX_WIDTH: f32 = 240.;
/// The selected row's and tab's edge.
const MARK: f32 = 2.;

/// A measure at the live text size.
pub fn scaled(v: f32) -> f32 {
    v * fonts::font_factor()
}

/// The list width a drag may leave: stays between the bounds whatever was saved.
pub fn clamp_list_width(width: f32) -> f32 {
    width.clamp(scaled(LIST_MIN_WIDTH), scaled(LIST_MAX_WIDTH))
}

/// The bar over everything: the app name, search and window controls.
pub fn title_bar() -> Div {
    div()
        .flex()
        .flex_row()
        .flex_none()
        .items_center()
        .gap_2()
        .h(px(scaled(TITLE_BAR_HEIGHT)))
        .bg(palette::window_chrome())
        .border_b_1()
        .border_color(palette::hairline())
}

/// The titlebar's linked apps: the sibling windows this app can open, right
/// after its own mark and name so the family reads as one group. Each link is
/// a 28px tile carrying the other app's own mark; the caller adds the ids,
/// listeners and tooltips.
///
/// ```text
/// [mark] [name] │ [notes] [agent]        … search, activity, window controls
/// ```
pub fn app_links() -> Div {
    div()
        .flex_shrink_0()
        .flex()
        .items_center()
        .gap_0p5()
        .ml_1()
        .pl_2()
        .border_l_1()
        .border_color(palette::hairline())
}

/// The debug selector a caller's element id stands for.
///
/// Kit shapes take the caller's id and must also expose it to
/// `debug_bounds`, or a headless probe can only find the shape by its
/// contents. Deriving the selector from the id means the app names a control
/// once and the probe reaches it by that same name.
fn selector_for(id: &ElementId) -> String {
    match id {
        ElementId::Name(name) => name.to_string(),
        ElementId::NamedInteger(name, n) => format!("{name}-{n}"),
        ElementId::Integer(n) => n.to_string(),
        other => format!("{other:?}"),
    }
}

/// One linked app in [`app_links`]: a 28px tile in the app's own mark.
///
/// The element id doubles as the debug selector, so a probe reaches the link
/// by the same name the app passed in (`notes-toggle`, `launch-station`) and
/// the callers do not have to restate it.
pub fn app_link(
    id: impl Into<ElementId>,
    icon_path: &str,
    label: impl Into<SharedString>,
) -> Stateful<Div> {
    let label = label.into();
    let id = id.into();
    let selector = selector_for(&id);
    div()
        .id(id)
        .debug_selector(move || selector)
        .a11y_button(label.clone())
        .aria_label(label)
        .occlude()
        .cursor_pointer()
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .size(px(scaled(28.)))
        .rounded(px(6.))
        .hover(|d| d.bg(palette::raised()))
        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(crate::icon::icon(
            icon_path,
            px(scaled(18.)),
            palette::text_dim(),
        ))
}

/// The app's sections. A spacer child pins Settings to the bottom.
pub fn rail() -> Div {
    div()
        .flex()
        .flex_col()
        .flex_none()
        .gap_1()
        .w(px(scaled(RAIL_WIDTH)))
        .h_full()
        .p_2()
        .bg(palette::window_chrome())
        .border_r_1()
        .border_color(palette::hairline())
}

/// One section: an icon over its label, the selected one lifted and inked.
pub fn rail_item(
    id: impl Into<ElementId>,
    icon_path: &str,
    label: impl Into<SharedString>,
    selected: bool,
) -> Stateful<Div> {
    rail_item_inked(id, icon_path, label, selected, palette::accent_ink())
}

/// [`rail_item`] in an accent the user picked: an app with presets passes its own.
pub fn rail_item_inked(
    id: impl Into<ElementId>,
    icon_path: &str,
    label: impl Into<SharedString>,
    selected: bool,
    accent_ink: gpui::Rgba,
) -> Stateful<Div> {
    let label = label.into();
    let (ink, glyph) = if selected {
        (palette::text(), accent_ink)
    } else {
        (palette::text_muted(), palette::text_muted())
    };
    div()
        .id(id)
        .a11y_tab(label.clone(), selected)
        .cursor_pointer()
        .flex()
        .flex_col()
        .items_center()
        .gap_1()
        .w_full()
        .py_2()
        .rounded(px(8.))
        .text_size(px(fonts::font_micro()))
        .font_weight(FontWeight::MEDIUM)
        .text_color(ink)
        .when(selected, |d| d.bg(palette::raised()))
        .when(!selected, |d| {
            d.hover(|s| s.bg(palette::panel()).text_color(palette::text()))
        })
        .child(icon(icon_path, px(scaled(22.)), glyph))
        .child(label)
}

/// Everything right of the rail.
pub fn stage() -> Div {
    div().flex().flex_col().flex_1().min_w_0().min_h_0()
}

/// The list and detail panes, side by side.
pub fn panes() -> Div {
    div().flex().flex_row().flex_1().min_h_0()
}

/// The strip of opened items over the panes. Its first tab is the list itself.
pub fn tab_strip() -> Div {
    div()
        .flex()
        .flex_row()
        .flex_none()
        .items_end()
        .gap_0p5()
        .h(px(scaled(TAB_STRIP_HEIGHT)))
        .px_2()
        .bg(palette::window_chrome())
        .border_b_1()
        .border_color(palette::hairline())
}

/// One tab of the strip. It joins the surface under it: pass the list's
/// [`palette::surface()`] for the list tab and the detail's [`palette::panel()`] for
/// an item.
pub fn strip_tab(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    selected: bool,
    surface: gpui::Rgba,
) -> Stateful<Div> {
    div()
        .id(id)
        .a11y_tab(label, selected)
        .relative()
        .cursor_pointer()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .h(px(scaled(STRIP_TAB_HEIGHT)))
        .min_w(px(scaled(STRIP_TAB_MIN_WIDTH)))
        .max_w(px(scaled(STRIP_TAB_MAX_WIDTH)))
        .px_3()
        .rounded_t(px(6.))
        .text_size(px(fonts::font_body()))
        .when(selected, |d| {
            d.bg(surface)
                .text_color(palette::text())
                .font_weight(FontWeight::SEMIBOLD)
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .right_0()
                        .h(px(MARK))
                        .bg(palette::accent()),
                )
        })
        .when(!selected, |d| {
            d.text_color(palette::text_dim())
                .font_weight(FontWeight::MEDIUM)
                .hover(|s| s.bg(palette::panel()).text_color(palette::text()))
        })
}

/// The list: what you can look at. `width` is the app's, kept per app and passed
/// through [`clamp_list_width`].
pub fn list_pane(width: f32) -> Div {
    div()
        .flex()
        .flex_col()
        .flex_none()
        .w(px(width))
        .h_full()
        .min_h_0()
        .bg(palette::surface())
        .border_r_1()
        .border_color(palette::hairline())
}

/// The detail: the selected item, the one place it ever opens.
pub fn detail_pane() -> Div {
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .h_full()
        .min_h_0()
        .bg(palette::panel())
}

/// One tab of a detail pane's strip. Its count goes in as a child.
pub fn detail_tab(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    selected: bool,
) -> Stateful<Div> {
    let label = label.into();
    div()
        .id(id)
        .a11y_tab(label.clone(), selected)
        .relative()
        .cursor_pointer()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .h(px(scaled(DETAIL_TAB_HEIGHT)))
        .px_3()
        .text_size(px(fonts::font_body()))
        .when(selected, |d| {
            d.text_color(palette::text())
                .font_weight(FontWeight::SEMIBOLD)
                .child(
                    div()
                        .absolute()
                        .bottom_0()
                        .left_3()
                        .right_3()
                        .h(px(MARK))
                        .child(crate::motion::grow_mark(
                            SharedString::from(format!("detail-tab-mark-{label}")),
                            palette::accent(),
                        )),
                )
        })
        .when(!selected, |d| {
            d.text_color(palette::text_dim())
                .font_weight(FontWeight::MEDIUM)
                .hover(|s| s.text_color(palette::text()))
        })
        .child(label)
}

/// A list row: the selected one lifted, with the accent down its left edge.
pub fn list_row(id: impl Into<ElementId>, selected: bool) -> Stateful<Div> {
    div()
        .id(id)
        .relative()
        .cursor_pointer()
        .w_full()
        .px_4()
        .py_2p5()
        .when(selected, |d| {
            d.bg(palette::raised()).child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left_0()
                    .w(px(MARK + 1.))
                    .bg(palette::accent()),
            )
        })
        .when(!selected, |d| d.hover(|s| s.bg(palette::panel())))
}

/// The one field height, at the live text size. A kit `Input` sizes itself to
/// `h_6` (24 px) whatever `with_size` says, because `Sizable` only stores the
/// text size and `input_h` falls through to the small control; the family's
/// fields are 36, so every field goes through [`field_height`] to say so.
pub fn field_height() -> gpui::Pixels {
    px(scaled(FIELD_HEIGHT))
}

/// The one control height, at the live text size.
pub fn control_height() -> gpui::Pixels {
    px(scaled(CONTROL_HEIGHT))
}

/// One field, at the family's metric: the kit `Input` inside a box of the
/// shared height. The height cannot go on the `Input` itself — a single-line
/// input applies its own `input_h` after any `.h()`, so a 36 would render at
/// 24 — and putting it on the box also gives the field one width and one
/// border to draw on.
pub fn field(id: impl Into<gpui::ElementId>, input: impl IntoElement) -> gpui::Stateful<gpui::Div> {
    let id = id.into();
    let selector = selector_for(&id);
    div()
        .id(id)
        .debug_selector(move || selector)
        .h(field_height())
        .w_full()
        .min_w_0()
        .child(input)
}

/// [`field`] for a slot that sets its own width (a row's trailing control).
pub fn field_at_width(
    id: impl Into<gpui::ElementId>,
    width: gpui::Pixels,
    input: impl IntoElement,
) -> gpui::Stateful<gpui::Div> {
    field(id, input).w(width)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bounds ride the text size, and the factor is process-wide: the
    /// scale tests move it, so this takes its turn and reads the factor once,
    /// instead of assuming 1.0 or reading it twice across a change.
    #[test]
    fn a_saved_list_width_stays_between_the_bounds() {
        let _turn = crate::fonts::factor_lock::serial();
        let factor = fonts::font_factor();
        let at = |v: f32| v * factor;
        assert_eq!(clamp_list_width(10.), at(LIST_MIN_WIDTH));
        assert_eq!(clamp_list_width(5000.), at(LIST_MAX_WIDTH));
        assert_eq!(clamp_list_width(LIST_WIDTH), at(LIST_WIDTH));
    }
}
