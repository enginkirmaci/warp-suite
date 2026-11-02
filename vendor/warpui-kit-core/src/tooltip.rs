//! Attribute-based hover tooltips: `with_tooltip(el, show, "…")` attaches the
//! kit's `.tooltip()` support to a hand-rolled control. The `show` flag lets
//! the host app honor its own "show tooltips" setting at the call site.

use gpui::prelude::*;
use gpui::{SharedString, Styled, div, px};

/// A tooltip body in the family palette. GPUI hosts tooltip views in their
/// own layer, so this is a tiny view rather than an absolutely-positioned
/// child.
struct TooltipText {
    text: SharedString,
}

impl Render for TooltipText {
    fn render(
        &mut self,
        _window: &mut gpui::Window,
        _cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        div()
            .rounded_md()
            .border_1()
            .border_color(crate::palette::HAIRLINE)
            .bg(crate::palette::POPUP)
            .px_2()
            .py_1()
            .text_size(px(crate::fonts::font_meta()))
            .text_color(crate::palette::TEXT_DIM)
            .shadow_md()
            .whitespace_nowrap()
            .child(self.text.clone())
    }
}

/// Tooltip builder for `.tooltip(tooltip_text("…"))`.
pub fn tooltip_text(
    text: impl Into<SharedString>,
) -> impl Fn(&mut gpui::Window, &mut gpui::App) -> gpui::AnyView + 'static {
    let text = text.into();
    move |_, cx| {
        let text = text.clone();
        cx.new(move |_| TooltipText { text }).into()
    }
}

/// Attaches a tooltip when the host's "show tooltips" setting is on. Every
/// helper takes an id'd element, so this stays a plain `Stateful<Div>`
/// pass-through.
pub fn with_tooltip(
    element: gpui::Stateful<gpui::Div>,
    show: bool,
    text: impl Into<SharedString> + 'static,
) -> gpui::Stateful<gpui::Div> {
    if show {
        let text = text.into();
        element.tooltip(tooltip_text(text))
    } else {
        element
    }
}
