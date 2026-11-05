//! warpui-kit-markdown: the Obsidian-style live markdown engine behind the
//! notes editor.
//!
//! - [`editor`] — the live editor itself: markdown rendered in place with its
//!   syntax hidden until the caret touches it, Obsidian's live preview. A
//!   `read_only` element renders the same surface with no caret, no input
//!   and no formatting — selection, highlighted code and links stay, and a
//!   link click follows its target (a web page through the platform's
//!   opener, a note through the host) — so hosts (chat answers) can show
//!   rendered markdown over their own state
//! - [`live`] — the pure parser: a deliberately line-based CommonMark subset
//!   plus the Obsidian flavor, covering how notes are actually written,
//!   splitting only at ASCII syntax bytes so byte ranges never land inside a
//!   multi-byte character
//! - [`highlighter`] — a highlighter for a kit code editor that maps those
//!   spans onto text styles, fading source markers on lines the caret is not on
//! - [`apply`] — the toolbar's pure string ops (bold/italic/headings/lists/
//!   links, `continue_list`), plus [`live::toggle_task_at`] for the click-to-
//!   toggle task boxes

pub mod apply;
pub mod editor;
pub mod highlighter;
pub mod live;

pub use editor::read_only_selection_text;
pub use editor::{CodeBlock, LiveColors, LiveEditor, LiveEditorEvent, LiveEditorState};
pub use highlighter::{LIVE_LANGUAGE, live_highlighter_factory};
pub use live::{MarkdownLiveKind, MarkdownLiveSpan, parse, parse_document, toggle_task_at};
