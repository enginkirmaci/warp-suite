//! Station's document markdown rendering through the suite's own live
//! markdown engine (warpui-kit-markdown) in read-only mode — the same
//! rendered surface chat answers (warp-agent) and notes use: no caret, no
//! text input, but text selection and copy, clickable links and
//! tree-sitter-highlighted code blocks stay. The PR comment list stays on
//! gpui-kit's TextView deliberately: it is a volatile list of many small
//! comments, not a document, and one editor entity per comment buys nothing.

use gpui::{
    AnyElement, AppContext as _, Entity, IntoElement as _, ParentElement as _, SharedString,
    Styled as _, Window, div, px,
};
use warpui_kit_markdown::{LiveEditor, LiveEditorState};

use crate::theme;

/// Creates the read-only editor hosting `text` and wires height chasing: the
/// render helper reads the editor's height through `&App`, which registers no
/// dependency, so the owning view is notified on editor moves explicitly.
/// One per document surface (README, summary); feed later content with
/// [`feed`].
pub fn attach<C: 'static>(
    text: SharedString,
    window: &mut Window,
    cx: &mut gpui::Context<C>,
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

/// Swaps the document the editor renders (a regenerated summary, a saved
/// README): `set_value` forgets undo history and resets scroll, which is the
/// wanted behavior for a whole-document replace.
pub fn feed<C: gpui::AppContext>(
    editor: &Entity<LiveEditorState>,
    text: SharedString,
    window: &mut Window,
    cx: &mut C,
) {
    editor.update(cx, |state, cx| state.set_value(text, window, cx));
}

/// The laid-out editor: sized to its content (the surrounding pane scrolls;
/// the editor never becomes an inner scroll area).
pub fn render_read_only(editor: &Entity<LiveEditorState>, cx: &gpui::App) -> AnyElement {
    render(editor, cx, false)
}

/// Like [`render_read_only`], but for a document embedded in a card: the
/// card already bounds the width, so the engine's readable measure and top
/// margin only add blank space around the content.
pub fn render_read_only_card(editor: &Entity<LiveEditorState>, cx: &gpui::App) -> AnyElement {
    render(editor, cx, true)
}

fn render(editor: &Entity<LiveEditorState>, cx: &gpui::App, in_card: bool) -> AnyElement {
    let height = editor.read(cx).content_height().max(1.);
    let mut element = LiveEditor::new(editor)
        .read_only(true)
        .fit_content(true)
        .text_size(px(theme::font_body()));
    if in_card {
        element = element.unbounded_column().top_margin_em(0.5);
    }
    div()
        .w_full()
        .h(px(height))
        .child(element)
        .into_any_element()
}
