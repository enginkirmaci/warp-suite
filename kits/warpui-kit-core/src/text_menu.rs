//! The one right-click menu for selectable text outside the input widgets.
//!
//! The editable fields get their Cut / Copy / Paste menu from
//! `gpui-component` itself. A read-only surface (the diff body, a PR comment,
//! a transcript message) registers with the window selection system instead,
//! and that system has no menu of its own. This module adds the missing half:
//!
//! - [`TextMenuAnchor`] is a zero-sized, focusable element rendered once per
//!   window. It carries the [`CopySelection`] action, which writes the
//!   window's selected text to the clipboard.
//! - [`menu`] builds the drawn context menu whose Copy item dispatches to that
//!   anchor.
//! - [`attach`] wraps a selectable surface in the menu and the layout it
//!   needs. It is a no-op when no host was installed, so a probe or a second
//!   window without the host still renders.
//! - [`install`] creates the anchor and publishes its focus handle to the
//!   window; call it before the first render and render the returned entity
//!   from the window root.
//!
//! The menu is drawn by GPUI, like the markdown editor's, so it looks the same
//! on every platform and is not clipped to the window by an OS popup.

use gpui::prelude::*;
use gpui::{
    AnyElement, App, ClipboardItem, Context, ElementId, Entity, FocusHandle, Global, IntoElement,
    Render, Window, div, px,
};
use gpui_kit::base::TextSelection;
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenu};

gpui::actions!(text_menu, [CopySelection]);

/// Copy the window's current text selection.
fn copy_selected(_: &CopySelection, window: &mut Window, cx: &mut App) {
    let text = TextSelection::selected_text(window, cx);
    if !text.is_empty() {
        cx.write_to_clipboard(ClipboardItem::new_string(text));
    }
}

/// The zero-sized action anchor a window renders once.
pub struct TextMenuAnchor {
    focus: FocusHandle,
}

impl TextMenuAnchor {
    /// Build the anchor for a window that does not use [`install`]'s global
    /// (a probe, a second window).
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
        }
    }

    pub fn focus_handle(&self) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for TextMenuAnchor {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        // zero-sized and absolute, so no pointer can land inside it: the
        // menu focuses this handle only to dispatch its action
        div()
            .id("text-menu-anchor")
            .track_focus(&self.focus)
            .absolute()
            .top_0()
            .left_0()
            .size(px(0.))
            .on_action(copy_selected)
    }
}

struct TextMenuHost {
    anchor: Entity<TextMenuAnchor>,
    focus: FocusHandle,
}

impl Global for TextMenuHost {}

/// Create the window's text-menu anchor and publish its focus handle.
///
/// Idempotent: a second call returns the first anchor. Render the returned
/// entity from the window root, once.
pub fn install(cx: &mut App) -> Entity<TextMenuAnchor> {
    if let Some(host) = cx.try_global::<TextMenuHost>() {
        return host.anchor.clone();
    }
    let anchor = cx.new(TextMenuAnchor::new);
    let focus = anchor.read(cx).focus_handle();
    cx.set_global(TextMenuHost {
        anchor: anchor.clone(),
        focus,
    });
    anchor
}

/// The installed anchor's focus handle, or `None` when [`install`] has not
/// run (a headless probe, an error page).
pub fn focus(cx: &App) -> Option<FocusHandle> {
    cx.try_global::<TextMenuHost>()
        .map(|host| host.focus.clone())
}

/// The drawn context menu: Copy, disabled while the window has no selection.
///
/// `focus` is the action target; it must be the handle of a
/// [`TextMenuAnchor`] rendered in the same window.
pub fn menu(
    focus: FocusHandle,
) -> impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static {
    move |menu, window, cx| {
        let copyable = TextSelection::has_selection(window, cx);
        menu.action_context(focus.clone()).menu_with_disabled(
            "Copy",
            Box::new(CopySelection),
            !copyable,
        )
    }
}

/// Wrap a selectable surface in the Copy menu.
///
/// `id` keeps the menu's element state separate per surface; rows rendered
/// from one call site must not share it. A surface rendered without an
/// installed host (no focus handle) still draws, just without the menu.
pub fn attach(
    id: impl Into<ElementId>,
    content: impl IntoElement,
    focus: Option<FocusHandle>,
) -> AnyElement {
    let wrapper = div().id(id).relative().child(content);
    match focus {
        Some(focus) => wrapper.context_menu(menu(focus)).into_any_element(),
        None => wrapper.into_any_element(),
    }
}
