//! Syntax tokens for code blocks: the family's tree-sitter pass, run once per
//! block text and resolved to document offsets. A language whose grammar the
//! app did not enable simply comes back without tokens.

use crate::live::{LiveBlockKind, LiveDocument};
use gpui::{Hsla, Rgba};
use gpui_kit::component::highlighter::HighlightTheme;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

/// One colored stretch of a code block, in document offsets.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CodeToken {
    pub range: Range<usize>,
    /// 0xRRGGBBAA.
    pub rgba: u32,
    pub italic: bool,
    pub weight: u16,
}

/// Packs a color for [`super::display::Ink::Rgba`].
fn rgba_key(color: Hsla) -> u32 {
    let rgba = Rgba::from(color);
    let byte = |v: f32| (v.clamp(0., 1.) * 255.).round() as u32;
    byte(rgba.r) << 24 | byte(rgba.g) << 16 | byte(rgba.b) << 8 | byte(rgba.a)
}

/// Unpacks a color from [`rgba_key`].
pub fn rgba_color(key: u32) -> Hsla {
    warpui_kit_core::palette::rgba_hex(key).into()
}

/// Tokens per code block of the current document. Keyed by block text so an
/// edit elsewhere keeps them, and an edit in a block rehighlights only it.
#[derive(Default)]
pub struct CodeTokens {
    by_text: HashMap<u64, Rc<Vec<CodeToken>>>,
    by_block: HashMap<usize, Rc<Vec<CodeToken>>>,
}

impl CodeTokens {
    /// Rebuilds the per-block table for a new document.
    pub fn update(&mut self, text: &str, doc: &LiveDocument, theme: &Arc<HighlightTheme>) {
        let mut by_text = HashMap::new();
        self.by_block.clear();
        for (ix, block) in doc.blocks.iter().enumerate() {
            let LiveBlockKind::Code { lang: Some(lang) } = &block.kind else {
                continue;
            };
            // the body between the fences
            let first = block.lines.start + 1;
            let last = doc.lines[block.lines.clone()]
                .iter()
                .rposition(|line| line.kind == crate::live::LiveLineKind::Code)
                .map(|at| block.lines.start + at);
            let Some(last) = last.filter(|&last| last >= first) else {
                continue;
            };
            let body = doc.lines[first].range.start..doc.lines[last].range.end;
            let key = {
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                (&text[body.clone()], lang.as_str()).hash(&mut hasher);
                hasher.finish()
            };
            let tokens = self.by_text.get(&key).cloned().unwrap_or_else(|| {
                let lang = lang.to_lowercase();
                let styles = warpui_kit_diff::highlight_buffer(&text[body.clone()], &lang, theme);
                Rc::new(
                    styles
                        .iter()
                        .filter_map(|(range, style)| {
                            let color = style.color?;
                            Some(CodeToken {
                                range: range.start..range.end,
                                rgba: rgba_key(color),
                                italic: matches!(style.font_style, Some(gpui::FontStyle::Italic)),
                                weight: style.font_weight.map_or(400, |w| w.0 as u16),
                            })
                        })
                        .collect(),
                )
            });
            by_text.insert(key, tokens.clone());
            // tokens are block-relative in the shared cache, document-relative here
            let shifted = tokens
                .iter()
                .map(|token| CodeToken {
                    range: token.range.start + body.start..token.range.end + body.start,
                    ..token.clone()
                })
                .collect();
            self.by_block.insert(ix, Rc::new(shifted));
        }
        self.by_text = by_text;
    }

    /// The tokens of code block `block`, ordered by offset.
    pub fn block(&self, block: usize) -> Option<Rc<Vec<CodeToken>>> {
        self.by_block.get(&block).cloned()
    }
}
