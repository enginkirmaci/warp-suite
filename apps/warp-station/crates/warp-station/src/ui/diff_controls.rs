//! The diff viewer's shared controls: the header's wrap and unified/split
//! toggles, the pinned horizontal bars under the body, the Copy-menu wrapper
//! and the one empty state. Changes, History and the pull request's Files and
//! Commits tabs all draw these, so the choice a user makes in one of them holds
//! in the rest — the mode and the wrap are settings, not per-view state.

use crate::store::Store;
use crate::theme;
use crate::ui::{A11y, icon};
use gpui::prelude::*;
use gpui::{AnyElement, App, Entity, ScrollHandle, div, px};
use gpui_kit::base::{Scrollbar, ScrollbarMode};
use gpui_kit::component::menu::ContextMenuExt as _;
use warp_station_core::settings::DiffViewMode;
use warpui_kit_diff::DiffScrollHandles;

/// The kit's mode for the app's setting.
pub fn kit_mode(mode: DiffViewMode) -> warpui_kit_diff::DiffMode {
    match mode {
        DiffViewMode::Unified => warpui_kit_diff::DiffMode::Unified,
        DiffViewMode::Split => warpui_kit_diff::DiffMode::Split,
    }
}

/// The header's single icon button. The glyph names the other mode and the
/// tooltip says the action; a click flips the setting and hands the new value
/// back to the view that owns the layout.
pub fn view_toggle(
    prefix: &'static str,
    split: bool,
    store: Entity<Store>,
    on_pick: impl Fn(DiffViewMode, &mut App) + 'static,
    show_tooltips: bool,
) -> AnyElement {
    let label = if split {
        "Switch to unified view"
    } else {
        "Switch to split view"
    };
    let icon_path = if split {
        "icons/icon-diff-unified.svg"
    } else {
        "icons/icon-diff-split.svg"
    };
    let selector = format!("{prefix}-view-toggle");
    let button = div()
        .id(selector.clone())
        .debug_selector(move || selector.clone())
        .a11y_button(label)
        .cursor_pointer()
        .rounded_md()
        .p_1()
        .flex()
        .items_center()
        .hover(|s| s.bg(theme::raised()))
        .on_click(move |_, _, cx| {
            let next = if split {
                DiffViewMode::Unified
            } else {
                DiffViewMode::Split
            };
            store.update(cx, |store, _| store.set_diff_view(next));
            on_pick(next, cx);
        })
        .child(icon(
            icon_path,
            px(theme::icon_small()),
            theme::text_muted(),
        ));
    crate::ui::with_tooltip(button, show_tooltips, label).into_any_element()
}

/// The header's word-wrap toggle. The icon button is active while rows wrap;
/// the choice is persisted with the layout.
pub fn wrap_toggle(
    prefix: &'static str,
    wrap: bool,
    store: Entity<Store>,
    on_pick: impl Fn(bool, &mut App) + 'static,
    show_tooltips: bool,
) -> AnyElement {
    let label = "Wrap long lines";
    let selector = format!("{prefix}-wrap-toggle");
    let button = div()
        .id(selector.clone())
        .debug_selector(move || selector.clone())
        .a11y_toggle(label, wrap)
        .cursor_pointer()
        .rounded_md()
        .p_1()
        .flex()
        .items_center()
        .when(wrap, |d| d.bg(theme::raised()))
        .hover(|d| d.bg(theme::raised()))
        .on_click(move |_, _, cx| {
            let next = !wrap;
            store.update(cx, |store, _| store.set_diff_wrap(next));
            on_pick(next, cx);
        })
        .child(icon(
            "icons/icon-wrap.svg",
            px(theme::icon_small()),
            if wrap {
                theme::text()
            } else {
                theme::text_muted()
            },
        ));
    crate::ui::with_tooltip(button, show_tooltips, label).into_any_element()
}

/// Pinned horizontal bars under the diff body. The kit tracks each viewport
/// with the matching handle; the bars live here, outside the vertical
/// scroller, so they stay visible while the diff scrolls.
pub fn scrollbars(prefix: &'static str, handles: &DiffScrollHandles, split: bool) -> AnyElement {
    let bar = |handle: &ScrollHandle, id: String| {
        div().flex_1().min_w_0().relative().child(
            Scrollbar::horizontal(handle)
                .id(id)
                .mode(ScrollbarMode::Always)
                .viewport_from_layout(),
        )
    };
    let mut footer = div()
        .debug_selector(move || format!("{prefix}-scrollbars"))
        .flex_shrink_0()
        .h(px(16.))
        .px_1()
        .flex()
        .flex_row()
        .bg(theme::well());
    footer = if split {
        footer
            .child(bar(&handles.left, format!("{prefix}-scrollbar-left")))
            .child(div().flex_shrink_0().w(px(1.)).bg(theme::hairline()))
            .child(bar(&handles.right, format!("{prefix}-scrollbar-right")))
    } else {
        footer.child(bar(&handles.unified, format!("{prefix}-scrollbar-unified")))
    };
    footer.into_any_element()
}

/// Wraps a full-pane selectable surface in the Copy menu. `text_menu::attach`
/// sizes to its content, which is right for a text block in flow but collapses
/// a surface that owns a scroller; this wrapper grows in the pane's flex
/// column and gives the scroller a definite height.
pub fn attach_filling(
    id: gpui::ElementId,
    content: AnyElement,
    focus: Option<gpui::FocusHandle>,
) -> AnyElement {
    let wrapper = div()
        .id(id)
        .relative()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .child(content);
    match focus {
        Some(focus) => wrapper
            .context_menu(warpui_kit_core::text_menu::menu(focus))
            .into_any_element(),
        None => wrapper.into_any_element(),
    }
}

/// A body with nothing to draw says why, in the pane's quiet ink.
pub fn state(text: &str) -> AnyElement {
    div()
        .flex_1()
        .flex()
        .items_center()
        .justify_center()
        .p_4()
        .text_size(px(theme::font_body()))
        .text_color(theme::text_muted())
        .child(text.to_string())
        .into_any_element()
}

/// The letter a changed file's rail shows, in the colour the family gives it.
pub fn status_color(code: &str) -> gpui::Rgba {
    match code {
        "A" => theme::SUCCESS_BRIGHT,
        "D" => theme::DANGER,
        "R" => theme::blue_bright(),
        _ => theme::AMBER_BRIGHT,
    }
}
