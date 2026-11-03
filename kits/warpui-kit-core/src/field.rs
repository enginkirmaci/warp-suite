//! The design's one text box: `.field`, `.area` and `.kbd` from `warp.css`.
//!
//! `DESIGN.md` puts a field at 36 high on 6 px corners with the recessed `well`
//! fill and the default `line` for an edge, turning `accent-ink` while it holds
//! focus; a multi-line `.area` takes the same skin with 9/10 padding, and the
//! shortcut chip (`Ctrl K`) is the `.kbd` row — mono 12 on `text-2`, one
//! `line-strong` border.
//!
//! The kit's own `Input` cannot draw that box: it paints its border with
//! `theme.input` and its fill as `theme.input` mixed toward transparent, so one
//! token has to be both the `line` edge and the `well` recess — it can only be
//! one colour. It also draws 24 high (a single-line input re-applies its own
//! height after any `.h()`), which is two thirds of the design's box. So the
//! box is drawn here and the kit widget goes inside it chrome-less
//! (`.appearance(false)`): the element edits, this module owns the surface.
//!
//! Every app routes its fields through these, so a dialog field, a settings
//! field and the title bar's search cannot disagree.

use crate::{fonts, palette, shell};
use gpui::prelude::*;
use gpui::{
    AnyElement, App, Div, ElementId, Entity, FocusHandle, Focusable as _, FontWeight, IntoElement,
    MouseButton, Pixels, SharedString, Stateful, Window, div, px,
};
use gpui_kit::component::Sizable as _;
use gpui_kit::component::input::{Input, InputState, Textarea, TextareaState};

/// The design's `.field` padding either side, and the gap between its slots
/// (an icon, the text, a `.kbd` chip).
const PAD_X: f32 = 10.;
const GAP: f32 = 8.;

/// `.titlebar .field { height: 32px }`: the search sits one step shorter than
/// the 36 of a page field so it fits the 46-high title bar.
pub fn title_height() -> Pixels {
    px(shell::scaled(32.))
}

/// The box alone, so a caller can seat slots around the text: the search
/// field's icon and its `Ctrl K` chip, or a compact 26-high tag field.
///
/// `focus` is the handle of the widget that will live inside: the box tracks
/// it, so the border turns `accent-ink` while that widget holds focus (the
/// style is resolved when the box paints, so it moves without a re-render),
/// and a press anywhere in the box — including its padding — focuses the
/// widget, as the prototype's `<label class="field">` does.
pub fn boxed(
    id: impl Into<ElementId>,
    focus: &FocusHandle,
    _window: &Window,
    height: Pixels,
) -> Stateful<Div> {
    let handle = focus.clone();
    let press_handle = focus.clone();
    let id = id.into();
    // the id doubles as the debug selector, so a probe reaches the box by the
    // name the app gave it
    let selector = shell::selector_for(&id);
    div()
        .id(id)
        .debug_selector(move || selector.clone())
        .track_focus(&handle)
        // the design's `.field:focus-within { border-color: var(--accent-ink) }`
        .focus(|style| style.border_color(palette::accent_ink()))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(shell::scaled(GAP)))
        .h(height)
        .w_full()
        .min_w_0()
        .px(px(shell::scaled(PAD_X)))
        .rounded(px(6.))
        .bg(palette::well())
        .border_1()
        .border_color(palette::hairline())
        // the value is `text` whatever the row it sits in paints; the widget's
        // placeholder keeps the kit's `text-3`
        .text_color(palette::text())
        // a long value or a `.kbd` chip stays inside the box instead of
        // widening it past the pane
        .overflow_hidden()
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            window.focus(&press_handle, cx);
        })
}

/// The kit input, chrome-less and stretched across the box: the field's own
/// surface and edge are the box's, not the widget's.
pub fn input(state: &Entity<InputState>) -> Input {
    bare_input(state).flex_1().h_full()
}

/// [`input`] for a machine value — a path, an executable, a pattern, a branch
/// name: the prototype's `.field.text` rule, mono 13.
pub fn mono_input(state: &Entity<InputState>) -> Input {
    mono_face(input(state))
}

/// The `.field.text` type on an input a caller built itself, so a mono field
/// that also needs a widget setting (disabled, a mask toggle) keeps the face.
pub fn mono_face(input: Input) -> Input {
    input
        .font_family(SharedString::from(fonts::code_font()))
        .text_size(px(fonts::font_mono_meta()))
}

fn bare_input(state: &Entity<InputState>) -> Input {
    Input::new(state)
        .appearance(false)
        .focus_bordered(false)
        .with_size(fonts::field_size())
        .min_w(px(0.))
        // the value is `text`; the kit paints a placeholder in `text-3` on its
        // own, and the input's text the colour it inherits — which, inside a
        // row, is whatever the row last set
        .text_color(palette::text())
}

/// A complete single-line field: the box with the input already inside. This
/// is the shape of nearly every field in the suite.
pub fn text(
    id: impl Into<ElementId>,
    state: &Entity<InputState>,
    window: &Window,
    cx: &App,
) -> Stateful<Div> {
    custom(id, state, window, cx, |input| input)
}

/// [`text`] in the mono face: the prototype's `.field text`.
pub fn mono(
    id: impl Into<ElementId>,
    state: &Entity<InputState>,
    window: &Window,
    cx: &App,
) -> Stateful<Div> {
    custom(id, state, window, cx, mono_face)
}

/// [`text`] for a widget that needs its own settings — the password mask
/// toggle, a disabled field, an aria label — applied to the input before it is
/// seated in the box.
pub fn custom(
    id: impl Into<ElementId>,
    state: &Entity<InputState>,
    window: &Window,
    cx: &App,
    build: impl FnOnce(Input) -> Input,
) -> Stateful<Div> {
    let handle = state.read(cx).focus_handle(cx);
    boxed(id, &handle, window, shell::field_height()).child(build(input(state)))
}

/// The design's `textarea.area`: the same recessed box, 9/10 padding, 14/20
/// text. The editor states its own paddings (4 either side, none above or
/// below at this size), so the box tops them up to the design's 10/9.
///
/// The box also carries a min height for the content's line count: the
/// editor's own auto-height collapses multi-line content to one visible row
/// on some platforms (its `size_full` inside this `h_auto` box resolves to
/// the minimum), which hid every scan root past the first on Windows.
pub fn area(
    id: impl Into<ElementId>,
    state: &Entity<TextareaState>,
    window: &Window,
    cx: &App,
) -> Stateful<Div> {
    let handle = state.read(cx).focus_handle(cx);
    boxed(id, &handle, window, px(0.))
        .h_auto()
        .min_h(area_min_height(state, cx, fonts::font_body()))
        .flex_col()
        .pt(px(shell::scaled(9.)))
        .pb(px(shell::scaled(9.)))
        .px(px(shell::scaled(6.)))
        .child(bare_area(state))
}

/// [`area`] for code: the prototype's `.area.t-mono` — pasted payloads, folder
/// lists, a commit message.
pub fn mono_area(
    id: impl Into<ElementId>,
    state: &Entity<TextareaState>,
    window: &Window,
    cx: &App,
) -> Stateful<Div> {
    let handle = state.read(cx).focus_handle(cx);
    boxed(id, &handle, window, px(0.))
        .h_auto()
        .min_h(area_min_height(state, cx, fonts::font_mono_meta()))
        .flex_col()
        .pt(px(shell::scaled(9.)))
        .pb(px(shell::scaled(9.)))
        .px(px(shell::scaled(6.)))
        .child(
            bare_area(state)
                .font_family(SharedString::from(fonts::code_font()))
                .text_size(px(fonts::font_mono_meta())),
        )
}

/// The 9/9 box padding plus one line height per content line, at 1.55 leading —
/// a floor for the editor's own height, so a multi-line value is fully visible
/// even where the editor's measurement comes back short.
fn area_min_height(state: &Entity<TextareaState>, cx: &App, font_size: f32) -> Pixels {
    let lines = state.read(cx).value().lines().count().max(1) as f32;
    px(shell::scaled(9. * 2. + lines * (font_size * 1.55)))
}

/// The kit textarea, chrome-less and filling its box: a caller that draws
/// its own frame (the agent's composer) seats this and owns the surface.
pub fn bare_area(state: &Entity<TextareaState>) -> Textarea {
    Textarea::new(state)
        .appearance(false)
        .bordered(false)
        // the smallest step's 4/0 inner padding is what the box's own padding
        // tops up to the design's 10/9
        .with_size(gpui_kit::component::Size::XSmall)
        .text_size(px(fonts::font_body()))
        .w_full()
        .min_w(px(0.))
}

/// The design's `.kbd`: a shortcut named inside a field, or beside a menu row.
pub fn kbd(label: impl Into<SharedString>) -> AnyElement {
    let label: SharedString = label.into();
    div()
        .flex_shrink_0()
        .flex()
        .items_center()
        .h(px(shell::scaled(20.)))
        .px(px(shell::scaled(5.)))
        .rounded(px(4.))
        .border_1()
        .border_color(palette::line_strong())
        .font_family(SharedString::from(fonts::code_font()))
        .font_weight(FontWeight::MEDIUM)
        .text_size(px(fonts::font_micro()))
        .text_color(palette::text_dim())
        .child(label)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The box rides the text factor like every other shell metric, so a
    /// larger text setting moves the field with the type inside it.
    #[test]
    fn the_field_box_rides_the_text_factor() {
        assert_eq!(shell::scaled(PAD_X), PAD_X);
        assert_eq!(shell::scaled(GAP), GAP);
    }

    /// The title bar's search is one step shorter than a page field.
    #[test]
    fn the_title_field_is_shorter_than_a_page_field() {
        assert!(title_height() < shell::field_height());
    }
}
