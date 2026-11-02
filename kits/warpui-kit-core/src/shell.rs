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
        .bg(palette::WINDOW_CHROME)
        .border_b_1()
        .border_color(palette::HAIRLINE)
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
        .bg(palette::WINDOW_CHROME)
        .border_r_1()
        .border_color(palette::HAIRLINE)
}

/// One section: an icon over its label, the selected one lifted and inked.
pub fn rail_item(
    id: impl Into<ElementId>,
    icon_path: &str,
    label: impl Into<SharedString>,
    selected: bool,
) -> Stateful<Div> {
    rail_item_inked(id, icon_path, label, selected, palette::ACCENT_INK)
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
        (palette::TEXT, accent_ink)
    } else {
        (palette::TEXT_MUTED, palette::TEXT_MUTED)
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
        .when(selected, |d| d.bg(palette::RAISED))
        .when(!selected, |d| {
            d.hover(|s| s.bg(palette::PANEL).text_color(palette::TEXT))
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
        .bg(palette::WINDOW_CHROME)
        .border_b_1()
        .border_color(palette::HAIRLINE)
}

/// One tab of the strip. It joins the surface under it: pass the list's
/// [`palette::SURFACE`] for the list tab and the detail's [`palette::PANEL`] for
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
                .text_color(palette::TEXT)
                .font_weight(FontWeight::SEMIBOLD)
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .right_0()
                        .h(px(MARK))
                        .bg(palette::ACCENT),
                )
        })
        .when(!selected, |d| {
            d.text_color(palette::TEXT_DIM)
                .font_weight(FontWeight::MEDIUM)
                .hover(|s| s.bg(palette::PANEL).text_color(palette::TEXT))
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
        .bg(palette::SURFACE)
        .border_r_1()
        .border_color(palette::HAIRLINE)
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
        .bg(palette::PANEL)
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
            d.text_color(palette::TEXT)
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
                            palette::ACCENT,
                        )),
                )
        })
        .when(!selected, |d| {
            d.text_color(palette::TEXT_DIM)
                .font_weight(FontWeight::MEDIUM)
                .hover(|s| s.text_color(palette::TEXT))
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
            d.bg(palette::RAISED).child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left_0()
                    .w(px(MARK + 1.))
                    .bg(palette::ACCENT),
            )
        })
        .when(!selected, |d| d.hover(|s| s.bg(palette::PANEL)))
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
    div()
        .id(id)
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

    #[test]
    fn a_saved_list_width_stays_between_the_bounds() {
        assert_eq!(clamp_list_width(10.), LIST_MIN_WIDTH);
        assert_eq!(clamp_list_width(5000.), LIST_MAX_WIDTH);
        assert_eq!(clamp_list_width(LIST_WIDTH), LIST_WIDTH);
    }
}
