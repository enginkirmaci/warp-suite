//! Code-block highlighting for plain code buffers: every language goes through
//! the same tree-sitter pass the kit installs — re-hosted here on the public
//! SyntaxHighlighter API because the component's own highlighter is
//! pub(crate).
//!
//! [`highlight_buffer`] caches by (theme, language, text), so a re-render of an
//! unchanged block skips both the rope rebuild and the style pass. The theme
//! Arc sits in the key, so cached styles can never outlive their palette.

use std::{cell::RefCell, collections::HashMap, ops::Range, sync::Arc};

use gpui::{HighlightStyle, SharedString};
use gpui_kit::base::input::Rope;
use gpui_kit::component::highlighter::{HighlightTheme, SyntaxHighlighter};

thread_local! {
    static HIGHLIGHTERS: RefCell<HashMap<SharedString, SyntaxHighlighter>> = RefCell::new(HashMap::new());
    static BUFFER_CACHE: RefCell<BufferCache> = RefCell::new(HashMap::new());
}

/// (highlight theme, language, text hash) → token styles, keyed on the theme
/// Arc so cached styles can never outlive their palette.
type BufferCache =
    HashMap<(Arc<HighlightTheme>, String, u64), Arc<Vec<(Range<usize>, HighlightStyle)>>>;

/// Token styles for a plain code buffer — the entry point for the renderers
/// that draw their own gutters. Cached by (language, text): a re-render of an
/// unchanged block — every repaint of an open fold while its tile streams —
/// skips both the rope rebuild and the tree-sitter style pass.
pub fn highlight_buffer(
    code: &str,
    lang: &str,
    theme: &Arc<HighlightTheme>,
) -> Arc<Vec<(Range<usize>, HighlightStyle)>> {
    // The theme Arc sits IN the key: accent swaps replace the global theme,
    // and holding the old Arc alive keeps its address from being reused, so
    // cached styles can never outlive their palette. 64 blocks cover every
    // open fold with slack; a full clear on overflow is fine — entries only
    // get expensive to rebuild, never stale.
    const CAP: usize = 64;
    let key = (theme.clone(), lang.to_string(), hash_text(code));
    if let Some(hit) = BUFFER_CACHE.with(|c| c.borrow().get(&key).cloned()) {
        return hit;
    }
    let styles = HIGHLIGHTERS.with(|cache| {
        let mut cache = cache.borrow_mut();
        let highlighter = cache
            .entry(lang.to_string().into())
            .or_insert_with(|| SyntaxHighlighter::new(lang));
        let rope = Rope::from_str(code);
        highlighter.update(None, &rope, None);
        Arc::new(highlighter.styles(&(0..code.len()), theme.as_ref()))
    });
    BUFFER_CACHE.with(|c| {
        let mut cache = c.borrow_mut();
        if cache.len() >= CAP {
            cache.clear();
        }
        cache.insert(key, styles.clone());
    });
    styles
}

fn hash_text(text: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}
