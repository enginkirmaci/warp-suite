//! A fixed document at read-only markdown size: the license viewer, and any
//! other surface that shows prose the user never edits.
//!
//! The live editor is built for notes, so a document surface needs three
//! things this module pins down: read-only state (no caret, no input, no
//! formatting), a height chase (the editor reports its laid-out height and the
//! host re-renders on a move), and a content-sized element the host's own
//! scroll area can hold.

use gpui::{
    AnyElement, App, AppContext as _, Context, Entity, IntoElement as _, ParentElement as _,
    SharedString, Styled as _, Window, div, px,
};

use crate::editor::{LiveEditor, LiveEditorState};
use warpui_kit_core::fonts;

/// Creates the read-only editor hosting `text` and wires height chasing: the
/// render helper reads the editor's height through `&App`, which registers no
/// dependency, so the owning view is notified on editor moves explicitly.
pub fn read_only_document<C: 'static>(
    text: SharedString,
    window: &mut Window,
    cx: &mut Context<C>,
) -> Entity<LiveEditorState> {
    let editor = cx.new(|cx| {
        let mut state = LiveEditorState::new(window, cx);
        state.set_read_only(true, cx);
        state.set_value(text, window, cx);
        state
    });
    let owner = cx.entity().downgrade();
    cx.observe(&editor, move |_, _, cx| {
        // defer: the editor notifies while its host is often still on the
        // update stack (a click inside the document, a layout pass), and
        // updating the host from here re-enters its lease. The effect runs
        // after the current update, where notifying is safe.
        let owner = owner.clone();
        cx.defer(move |cx| {
            if let Some(owner) = owner.upgrade() {
                owner.update(cx, |_, cx| cx.notify());
            }
        });
    })
    .detach();
    editor
}

/// The laid-out document, sized to its content at the family's body size.
pub fn render_read_only_document(editor: &Entity<LiveEditorState>, cx: &App) -> AnyElement {
    render_at(editor, cx, fonts::font_body(), false)
}

/// [`render_read_only_document`] at another type step (a note preview keeps
/// the note size).
pub fn render_read_only_document_at(
    editor: &Entity<LiveEditorState>,
    cx: &App,
    text_size: f32,
) -> AnyElement {
    render_at(editor, cx, text_size, false)
}

/// [`render_read_only_document`] for a document embedded in a bounded card (a
/// modal): the card is already the reading measure, so the engine's column and
/// its generous top margin only add blank space around the text.
pub fn render_read_only_document_in_card(editor: &Entity<LiveEditorState>, cx: &App) -> AnyElement {
    render_at(editor, cx, fonts::font_body(), true)
}

fn render_at(
    editor: &Entity<LiveEditorState>,
    cx: &App,
    text_size: f32,
    in_card: bool,
) -> AnyElement {
    let height = editor.read(cx).content_height().max(1.);
    let mut element = LiveEditor::new(editor)
        .read_only(true)
        .fit_content(true)
        .text_size(px(text_size));
    if in_card {
        element = element.unbounded_column().top_margin_em(0.25);
    }
    div()
        .w_full()
        .h(px(height))
        .child(element)
        .into_any_element()
}
