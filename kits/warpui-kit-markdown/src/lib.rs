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
//!   links), plus [`live::toggle_task_at`] for the click-to-toggle task boxes
//! - [`renders_as_markdown`] — the guard for a surface fed untrusted text: asks
//!   the parser whether GPUI's markdown `TextView` can lay the text out, so the
//!   host falls back to plain prose instead of taking the window down

pub mod apply;
pub mod editor;
pub mod highlighter;
pub mod live;
pub mod read_only;

use std::cell::RefCell;
use std::collections::VecDeque;

pub use editor::read_only_selection_text;
pub use editor::{CodeBlock, LiveColors, LiveEditor, LiveEditorEvent, LiveEditorState, note_key};
pub use highlighter::{LIVE_LANGUAGE, live_highlighter_factory};
pub use live::{MarkdownLiveKind, MarkdownLiveSpan, parse, parse_document, toggle_task_at};
pub use read_only::{
    read_only_document, render_read_only_document, render_read_only_document_in_card,
};

// ---------------------------------------------------------------------------
// The parser guard
// ---------------------------------------------------------------------------

/// Verdicts by text, most recently used at the back, held up to this many
/// bytes. A repaint walks every surface a host guards; without the memo each
/// one would be parsed again only to re-learn the verdict it already had.
const VERDICT_BYTES: usize = 4 * 1024 * 1024;

thread_local! {
    static VERDICTS: RefCell<VecDeque<(String, bool)>> = const { RefCell::new(VecDeque::new()) };
}

/// One warning per run is enough to name the library bug: a streaming message
/// that carries the offending shape probes a new text every chunk.
static WARNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Whether the markdown parser survives `text`. GPUI's `TextView::markdown`
/// parses during layout with no catch, so a surface fed untrusted text must ask
/// this first and fall back to plain prose instead of taking the window down.
///
/// The answer is memoized by text; the parser's panic is the expected outcome
/// here, so it is caught at this boundary and reported once.
pub fn renders_as_markdown(text: &str) -> bool {
    if let Some(known) = remembered(text) {
        return known;
    }
    let verdict = probe(text);
    if !verdict && !WARNED.swap(true, std::sync::atomic::Ordering::Relaxed) {
        log::warn!(
            "markdown parser panics on {} bytes; rendering it as plain text",
            text.len()
        );
    }
    remember(text, verdict);
    verdict
}

/// The options gpui-base gives `TextView::markdown` (`MarkdownExtensions`
/// defaults in `gpui-base/src/text/markdown_ext.rs`): GFM plus both math
/// constructs, frontmatter off. They are not public there, so they are mirrored
/// here — the probe predicts the TextView only while the two agree, which makes
/// this the place to look when gpui-base moves.
fn parse_options() -> markdown::ParseOptions {
    let mut options = markdown::ParseOptions::gfm();
    options.constructs.math_text = true;
    options.constructs.math_flow = true;
    options
}

fn probe(text: &str) -> bool {
    let options = parse_options();
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        markdown::to_mdast(text, &options)
    }))
    .is_ok()
}

fn remembered(text: &str) -> Option<bool> {
    VERDICTS.with(|verdicts| {
        let mut verdicts = verdicts.borrow_mut();
        let at = verdicts
            .iter()
            .position(|(known, _)| known.as_str() == text)?;
        let entry = verdicts.remove(at)?;
        let verdict = entry.1;
        verdicts.push_back(entry);
        Some(verdict)
    })
}

fn remember(text: &str, verdict: bool) {
    VERDICTS.with(|verdicts| {
        let mut verdicts = verdicts.borrow_mut();
        let mut held: usize = verdicts.iter().map(|(known, _)| known.len()).sum();
        while held + text.len() > VERDICT_BYTES {
            let Some((dropped, _)) = verdicts.pop_front() else {
                break;
            };
            held -= dropped.len();
        }
        // a text that alone exceeds the budget is still kept: it is the one
        // whose re-parse costs the most
        verdicts.push_back((text.to_string(), verdict));
    });
}

#[cfg(test)]
mod guard_tests {
    use super::renders_as_markdown;

    /// The `markdown` 1.0.0 compiler bug the guard exists for. A diff of
    /// deleted Markdown spells this out exactly: its empty lines read `-`, its
    /// rules `----`.
    const PANICS: &str = "-a\n-\n----\n@@ -1 +0,0 @@\n----";

    #[test]
    fn text_that_panics_the_parser_falls_back_to_plain() {
        // a second call proves the memo answers without parsing (and without
        // panicking) again
        assert!(!renders_as_markdown(PANICS));
        assert!(!renders_as_markdown(PANICS));
    }

    #[test]
    fn ordinary_markdown_still_renders() {
        assert!(renders_as_markdown("# Title\n\n- one\n- two\n\n`code`\n"));
        // a setext heading on its own is fine: the guard is not guessing at
        // headings, it asks the parser
        assert!(renders_as_markdown("a\n-"));
    }
}
