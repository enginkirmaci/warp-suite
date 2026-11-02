//! The Obsidian-style live editor: markdown rendered in place while typing.
//!
//! The kit's code editor lays out fixed-height rows of one font, which a live
//! preview cannot live in: headings need their own size, hidden syntax must
//! take no space, and bullets, checkboxes, callouts, tables and images are
//! drawn. So this is an editor of its own, line-based like CodeMirror:
//!
//! - `display` decides what each line shows for the current selection, the
//!   reveal rules, as plain data with a map back to source offsets
//! - `layout` shapes and wraps one line's text, rows mixing font sizes
//! - `frame` places the lines and answers hit-testing questions
//! - `images` loads the pictures a note names, for views and paint to look up;
//!   `diagram` renders Mermaid blocks into more of them
//! - `state` owns the document, selection, undo and every editing command,
//!   with the pure commands themselves in `ops`
//! - `element` paints a frame and wires input

pub(crate) mod code;
pub(crate) mod diagram;
pub(crate) mod display;
pub(crate) mod element;
pub(crate) mod frame;
pub(crate) mod icons;
pub(crate) mod images;
pub(crate) mod layout;
pub(crate) mod ops;
pub(crate) mod state;
pub(crate) mod theme;

pub use element::LiveEditor;
pub use state::{LiveEditorEvent, LiveEditorState};
pub use theme::LiveColors;
