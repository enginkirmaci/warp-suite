//! warpui-kit-diff: Zed-style unified diff and line-numbered code rendering
//! for gpui-kit applications.
//!
//! [`render_diff`] turns a unified patch into a [`document::DiffDocument`] and
//! draws the Zed-style unified view: one line-number column of new-file
//! numbers (deletions carry none, since their old number is not a line in the
//! file), a +/− marker
//! cell, a strong wash over the gutter and a quieter one over the code, a
//! change bar on the row's leading edge, and tree-sitter token colors inside
//! the lines. Changed words inside a paired deletion/addition are emphasized.
//!
//! Rows never wrap: one row is one line, and the body scrolls horizontally so
//! a long line keeps its shape. The number column currently scrolls with the
//! row — GPUI's div scroll shares one offset across both axes, and there is
//! no sticky-position support, so pinning it waits for the virtualized list.
//!
//! The body is one selectable text: a drag crosses lines and Copy gives the
//! patch content without the gutter numbers or markers. The gutter and row
//! washes are drawn beside and behind that text. The host must mount
//! `gpui_kit::base::TextSelectionLayer` for the drag to register.
//!
//! [`render_code`] is the same presentation for plain code: a line-number
//! gutter plus token colors, no washes. Pre-numbered tool output (`   12\t…`
//! prefixes) gets its prefixes lifted into the real gutter instead of
//! double-numbering.
//!
//! Both renderers pin the code family and the caller's `font_detail` size on
//! the block's own root element. GPUI shapes a `StyledText` at the *ambient*
//! text style (a `TextRun` carries a font, never a size), so a style built
//! from `window.text_style()` alone only decides the color: the rows were
//! drawn at whatever size the surrounding view happened to set, which is how
//! the diff stopped following the app's text size. The pin is what makes
//! `font_detail` mean anything.
//!
//! Colors come from `warpui-kit-core`'s palette; token colors come from the
//! kit's installed highlight theme. Enable the grammar features (this
//! crate's `tree-sitter-*` features forward to gpui-kit) for the languages
//! callers render.

mod document;
mod highlight;
mod selectable;

pub use document::{DiffDocument, DiffFile, DiffHunk, DiffRow, RowKind, lang_from_path};
pub use highlight::{CodeBlockHighlighter, highlight_buffer, shared_highlighter};
use selectable::SelectableText;

use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

use gpui::prelude::*;
use gpui::{
    AbsoluteLength, AnyElement, App, BorderStyle, Bounds, Corners, Edges, ElementId,
    HighlightStyle, Hsla, PaintQuad, Pixels, StyledText, TextStyle, Window, canvas, div, point, px,
    size, transparent_black,
};

use warpui_kit_core::fonts;
use warpui_kit_core::palette;

thread_local! {
    /// Parsed patches, keyed by (text hash, length). Parsing includes the word
    /// diff behind intra-line emphasis, so a re-render of an unchanged patch
    /// must not repeat it. A full clear on overflow is fine: entries are cheap
    /// to rebuild, never stale.
    static DOCUMENT_CACHE: RefCell<Vec<(u64, usize, Rc<DiffDocument>)>> = const { RefCell::new(Vec::new()) };
}

/// A unified diff with Zed-style gutter, marker column and two-tone washes.
/// `path_hint` (the tool's file path) is the fallback language guess when the
/// patch itself carries no recognizable file paths.
pub fn render_diff(
    text: &str,
    path_hint: Option<&str>,
    font_detail: f32,
    window: &mut Window,
    cx: &App,
) -> AnyElement {
    if text.is_empty() {
        return div().into_any_element();
    }
    let doc = cached_document(text);
    render_document(&doc, path_hint, font_detail, window, cx)
}

/// Draw an already parsed patch. The app can keep the document alongside its
/// view state and re-render without reparsing.
pub fn render_document(
    doc: &DiffDocument,
    path_hint: Option<&str>,
    font_detail: f32,
    window: &mut Window,
    cx: &App,
) -> AnyElement {
    if doc.files.is_empty() {
        return div().into_any_element();
    }

    let language = doc.language(path_hint);
    let gutter_width = doc.max_number().to_string().len().max(4);
    let hl_theme = gpui_kit::component::Theme::global(cx)
        .highlight_theme
        .clone();
    let paint = RowPaint {
        style: code_style(window, font_detail, palette::CODE_FG),
        number_style: HighlightStyle {
            color: Some(palette::DIFF_GUTTER.into()),
            ..Default::default()
        },
        gutter_width,
    };
    let line_height = line_height_px(&paint.style, window);

    // One visual line per row: code, hunk headers, state lines and (for a
    // multi-file patch) file headers. The gutter is drawn beside this text,
    // not inside it, so copying a selection yields the patch content without
    // line numbers or +/− markers.
    let mut lines = Vec::new();
    for file in &doc.files {
        if doc.files.len() > 1 {
            lines.push(Line::file(file));
        }
        if file.binary {
            lines.push(Line::state("Binary file — no textual diff"));
            continue;
        }
        if file.hunks.is_empty() {
            lines.push(Line::state("No textual changes"));
            continue;
        }
        for hunk in &file.hunks {
            lines.push(Line::hunk(hunk));
            let tokens = highlight_hunk(hunk, language, &hl_theme);
            for (i, row) in hunk.rows.iter().enumerate() {
                lines.push(Line::code(row, tokens[i].clone()));
            }
        }
        if file.truncated {
            lines.push(Line::state("Patch truncated at 64 KB"));
        }
    }
    if lines.is_empty() {
        return div().into_any_element();
    }

    // One buffer for the whole patch: selection and copy work across lines
    // only when the text is one laid-out run.
    let mut text = String::new();
    let mut starts = Vec::with_capacity(lines.len());
    for (index, line) in lines.iter().enumerate() {
        if index > 0 {
            text.push('\n');
        }
        starts.push(text.len());
        text.push_str(&line.text);
    }

    let mut runs = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let start = starts[index];
        let len = line.text.len();
        match line.kind {
            LineKind::Code(_) => {
                for (range, style) in
                    merged_runs(len, &line.tokens, &line.emphasis, line.emphasis_bg)
                {
                    runs.push((start + range.start..start + range.end, style));
                }
            }
            LineKind::Hunk | LineKind::State => {
                if len > 0 {
                    runs.push((
                        start..start + len,
                        HighlightStyle {
                            color: Some(palette::text_muted().into()),
                            ..Default::default()
                        },
                    ));
                }
            }
            LineKind::File => {}
        }
    }
    let layout_key = styled_key(&text, &runs, font_detail);
    let styled = StyledText::new(text.clone()).with_default_highlights(&paint.style, runs);

    // One canvas per column paints every row's wash; one text per column
    // carries every line. A row-per-div tree laid out a few thousand elements
    // on every render of a large diff — the canvas keeps the body to a
    // handful of elements.
    let washes: Vec<RowWash> = lines.iter().map(|line| line_wash(line.kind)).collect();
    let code_colors: Vec<Option<Hsla>> = washes.iter().map(|wash| wash.content).collect();
    let gutter_colors: Vec<(Option<Hsla>, Option<gpui::Rgba>)> =
        washes.iter().map(|wash| (wash.gutter, wash.bar)).collect();

    let code_canvas = canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            for (index, color) in code_colors.iter().enumerate() {
                if let Some(color) = color {
                    paint_spot(
                        window,
                        Bounds::new(
                            point(bounds.left(), bounds.top() + line_height * index as f32),
                            size(bounds.size.width, line_height),
                        ),
                        *color,
                    );
                }
            }
        },
    )
    .absolute()
    .inset_0();

    let gutter_canvas = canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            for (index, (cell, bar)) in gutter_colors.iter().enumerate() {
                let top = bounds.top() + line_height * index as f32;
                let row = Bounds::new(
                    point(bounds.left(), top),
                    size(bounds.size.width, line_height),
                );
                if let Some(cell) = cell {
                    paint_spot(window, row, *cell);
                }
                if let Some(bar) = bar {
                    paint_spot(
                        window,
                        Bounds::new(row.origin, size(px(3.), line_height)),
                        (*bar).into(),
                    );
                }
            }
        },
    )
    .absolute()
    .inset_0();

    let mut gutter_text = String::new();
    let mut gutter_runs = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if index > 0 {
            gutter_text.push('\n');
        }
        match line.kind {
            LineKind::Code(kind) => {
                let start = gutter_text.len();
                let (content, highlights) = gutter_content(line.number, kind, &paint);
                for (range, style) in highlights {
                    gutter_runs.push((start + range.start..start + range.end, style));
                }
                gutter_text.push_str(&content);
            }
            // a non-code row still needs a line box so the numbers below it
            // keep their row
            _ => gutter_text.push('\u{00a0}'),
        }
    }
    let gutter_styled =
        StyledText::new(gutter_text).with_default_highlights(&paint.style, gutter_runs);

    let selection_seed = hash_text(&text);
    let selectable = SelectableText::new(
        ElementId::NamedInteger("diff-selection".into(), selection_seed),
        text.into(),
        styled,
        layout_key,
    );

    // The code column holds the one selectable text; the canvases sit behind
    // it and behind the gutter.
    let body = div()
        .debug_selector(|| "diff-body".into())
        .flex()
        .flex_row()
        .flex_shrink_0()
        .min_w_full()
        .child(
            div()
                .debug_selector(|| "diff-gutter".into())
                .relative()
                .flex_shrink_0()
                .child(gutter_canvas)
                .child(div().pl(px(11.)).pr_2().child(gutter_styled)),
        )
        .child(
            div()
                .debug_selector(|| "diff-code-column".into())
                .flex_1()
                .relative()
                .child(code_canvas)
                .child(
                    div()
                        .debug_selector(|| "diff-text".into())
                        .pl_2()
                        .child(selectable),
                ),
        );

    div()
        .id("diff-x-scroll")
        .debug_selector(|| "diff-x-scroll".into())
        .w_full()
        .flex()
        .flex_row()
        .overflow_x_scroll()
        .restrict_scroll_to_axis()
        .font_family(fonts::code_font())
        .text_size(px(font_detail))
        .whitespace_nowrap()
        .child(body)
        .into_any_element()
}

/// Line-numbered code block body (no washes): used for tool detail input /
/// output. Wraps in the caller's chip block; `color` is the base text color
/// token runs don't cover.
pub fn render_code(
    text: &str,
    lang: Option<&'static str>,
    color: gpui::Rgba,
    font_detail: f32,
    window: &mut Window,
    cx: &App,
) -> AnyElement {
    if text.is_empty() {
        return div().into_any_element();
    }
    let norm = text.replace('\r', "");
    let mut lines: Vec<&str> = norm.split('\n').collect();
    if lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    // opencode's read tool prefixes output lines with `   12\t` — lift those
    // numbers into the gutter instead of rendering them twice
    let mut first_number = 1usize;
    if let Some((base, stripped)) = strip_line_numbers(&lines) {
        first_number = base;
        lines = stripped;
    }

    let gutter_width = (first_number + lines.len()).to_string().len().max(2);
    let hl_theme = gpui_kit::component::Theme::global(cx)
        .highlight_theme
        .clone();
    let kinds = vec![RowKind::Context; lines.len()];
    let tokens = match lang {
        Some(lang) => line_tokens(&lines, &kinds, lang, &hl_theme),
        None => vec![Vec::new(); lines.len()],
    };

    let style = code_style(window, font_detail, color);
    let num_style = HighlightStyle {
        color: Some(palette::DIFF_GUTTER.into()),
        ..Default::default()
    };

    let mut root = block_root(font_detail);
    for (i, line) in lines.iter().enumerate() {
        let number = first_number + i;
        let gutter = styled_text(
            &format!("{number:>gutter_width$}"),
            &style,
            &[(0..gutter_width, num_style)],
        );
        root = root.child(
            div()
                .w_full()
                .flex()
                .child(div().flex_shrink_0().child(gutter))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .pl_2()
                        .child(styled_text(line, &style, &tokens[i])),
                ),
        );
    }
    root.into_any_element()
}

/// The one language sniff for detail payloads: pretty-printed JSON is the
/// structured shape tool input reliably arrives in.
pub fn sniff_lang(text: &str) -> Option<&'static str> {
    let trimmed = text.trim_start();
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        serde_json::from_str::<serde_json::Value>(text)
            .ok()
            .map(|_| "json")
    } else {
        None
    }
}

// ---- diff rendering --------------------------------------------------------

struct RowPaint {
    style: TextStyle,
    number_style: HighlightStyle,
    gutter_width: usize,
}

fn cached_document(text: &str) -> Rc<DiffDocument> {
    let hash = hash_text(text);
    DOCUMENT_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some((_, _, doc)) = cache
            .iter()
            .find(|(cached_hash, cached_len, _)| *cached_hash == hash && *cached_len == text.len())
        {
            return doc.clone();
        }
        let doc = Rc::new(DiffDocument::parse(text));
        if cache.len() >= 8 {
            cache.clear();
        }
        cache.push((hash, text.len(), doc.clone()));
        doc
    })
}

fn hash_text(text: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

/// Identity of the shaped text: the text, its runs and the font step. The
/// same key gets the retained `StyledText` (and its cached shaping) back.
fn styled_key(text: &str, runs: &[(Range<usize>, HighlightStyle)], font_detail: f32) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    font_detail.to_bits().hash(&mut hasher);
    for (range, style) in runs {
        range.hash(&mut hasher);
        style.hash(&mut hasher);
    }
    hasher.finish()
}

/// One visual line of the diff. Code lines carry their highlight runs; hunk,
/// state and file lines are structure. The line's text is the selectable
/// content (no gutter, no +/− marker).
struct Line {
    text: String,
    kind: LineKind,
    number: Option<u32>,
    tokens: Vec<(Range<usize>, HighlightStyle)>,
    emphasis: Vec<Range<usize>>,
    emphasis_bg: Option<Hsla>,
}

#[derive(Clone, Copy)]
enum LineKind {
    File,
    Hunk,
    Code(RowKind),
    State,
}

impl Line {
    fn code(row: &DiffRow, tokens: Vec<(Range<usize>, HighlightStyle)>) -> Self {
        let (_, _, _, emphasis_bg) = row_colors(row.kind);
        Self {
            text: row.text.clone(),
            kind: LineKind::Code(row.kind),
            number: row.number(),
            tokens,
            emphasis: row.emphasis.clone(),
            emphasis_bg,
        }
    }

    fn hunk(hunk: &DiffHunk) -> Self {
        Self {
            text: hunk.title(),
            kind: LineKind::Hunk,
            number: None,
            tokens: Vec::new(),
            emphasis: Vec::new(),
            emphasis_bg: None,
        }
    }

    fn state(text: &'static str) -> Self {
        Self {
            text: text.to_string(),
            kind: LineKind::State,
            number: None,
            tokens: Vec::new(),
            emphasis: Vec::new(),
            emphasis_bg: None,
        }
    }

    fn file(file: &DiffFile) -> Self {
        let text = match file.rename() {
            Some((old, new)) => format!("{old} → {new}  +{} −{}", file.added, file.removed),
            None => format!(
                "{}  +{} −{}",
                file.path().unwrap_or("unknown file"),
                file.added,
                file.removed
            ),
        };
        Self {
            text,
            kind: LineKind::File,
            number: None,
            tokens: Vec::new(),
            emphasis: Vec::new(),
            emphasis_bg: None,
        }
    }
}

fn gutter_content(
    number: Option<u32>,
    kind: RowKind,
    paint: &RowPaint,
) -> (String, Vec<(Range<usize>, HighlightStyle)>) {
    let number = number
        .map(|number| format!("{number:>width$}", width = paint.gutter_width))
        .unwrap_or_else(|| " ".repeat(paint.gutter_width));
    let (marker, marker_color) = match kind {
        RowKind::Add => ("+", palette::ADD),
        RowKind::Del => ("−", palette::DEL),
        RowKind::Context => ("\u{00a0}", palette::DIFF_GUTTER),
    };
    let marker_start = paint.gutter_width + 1;
    let marker_end = marker_start + marker.len();
    (
        format!("{number} {marker}"),
        vec![
            (0..paint.gutter_width, paint.number_style),
            (
                marker_start..marker_end,
                HighlightStyle {
                    color: Some(marker_color.into()),
                    ..Default::default()
                },
            ),
        ],
    )
}

/// The three washes one row needs: the gutter cell, the change bar and the
/// code column.
#[derive(Clone, Copy)]
struct RowWash {
    gutter: Option<Hsla>,
    bar: Option<gpui::Rgba>,
    content: Option<Hsla>,
}

fn line_wash(kind: LineKind) -> RowWash {
    match kind {
        LineKind::Code(kind) => {
            let (bar, gutter, content, _) = row_colors(kind);
            RowWash {
                gutter,
                bar,
                content,
            }
        }
        LineKind::Hunk => RowWash {
            gutter: Some(palette::DIFF_HUNK_BG),
            bar: None,
            content: Some(palette::DIFF_HUNK_BG),
        },
        LineKind::File => RowWash {
            gutter: Some(palette::surface().into()),
            bar: None,
            content: Some(palette::surface().into()),
        },
        LineKind::State => RowWash {
            gutter: None,
            bar: None,
            content: None,
        },
    }
}

fn paint_spot(window: &mut Window, bounds: Bounds<Pixels>, color: Hsla) {
    window.paint_quad(PaintQuad {
        bounds,
        background: color.into(),
        corner_radii: Corners::default(),
        border_widths: Edges::default(),
        border_color: transparent_black(),
        border_style: BorderStyle::default(),
    });
}

fn line_height_px(style: &TextStyle, window: &Window) -> Pixels {
    let font_size = style.font_size.to_pixels(window.rem_size());
    window.pixel_snap(
        style
            .line_height
            .to_pixels(font_size.into(), window.rem_size()),
    )
}

fn row_colors(kind: RowKind) -> (Option<gpui::Rgba>, Option<Hsla>, Option<Hsla>, Option<Hsla>) {
    match kind {
        RowKind::Add => (
            Some(palette::ADD),
            Some(palette::DIFF_ADD_GUTTER_BG),
            Some(palette::DIFF_ADD_BG),
            Some(palette::DIFF_ADD_EMPHASIS),
        ),
        RowKind::Del => (
            Some(palette::DEL),
            Some(palette::DIFF_DEL_GUTTER_BG),
            Some(palette::DIFF_DEL_BG),
            Some(palette::DIFF_DEL_EMPHASIS),
        ),
        RowKind::Context => (None, None, None, None),
    }
}

/// Highlight each side of a hunk separately. Mixing deletions and additions in
/// one buffer lets a deleted brace or quote change the colors of the lines
/// that follow it; this keeps the old and new files as their own programs.
fn highlight_hunk(
    hunk: &DiffHunk,
    language: Option<&str>,
    theme: &Arc<gpui_kit::component::highlighter::HighlightTheme>,
) -> Vec<Vec<(Range<usize>, HighlightStyle)>> {
    let mut out = vec![Vec::new(); hunk.rows.len()];
    let Some(language) = language else {
        return out;
    };
    let mut old = String::new();
    let mut old_span = vec![None; hunk.rows.len()];
    let mut new = String::new();
    let mut new_span = vec![None; hunk.rows.len()];
    for (i, row) in hunk.rows.iter().enumerate() {
        match row.kind {
            RowKind::Context => {
                old_span[i] = Some((old.len(), row.text.len()));
                old.push_str(&row.text);
                old.push('\n');
                new_span[i] = Some((new.len(), row.text.len()));
                new.push_str(&row.text);
                new.push('\n');
            }
            RowKind::Del => {
                old_span[i] = Some((old.len(), row.text.len()));
                old.push_str(&row.text);
                old.push('\n');
            }
            RowKind::Add => {
                new_span[i] = Some((new.len(), row.text.len()));
                new.push_str(&row.text);
                new.push('\n');
            }
        }
    }
    let old_styles = highlight_buffer(&old, language, theme);
    let new_styles = highlight_buffer(&new, language, theme);
    for (i, row) in hunk.rows.iter().enumerate() {
        let (styles, span) = match row.kind {
            RowKind::Del => (&old_styles, old_span[i]),
            RowKind::Context | RowKind::Add => (&new_styles, new_span[i]),
        };
        if let Some((start, len)) = span {
            out[i] = slice_runs(styles, start, len);
        }
    }
    out
}

fn slice_runs(
    styles: &[(Range<usize>, HighlightStyle)],
    start: usize,
    len: usize,
) -> Vec<(Range<usize>, HighlightStyle)> {
    let end = start + len;
    let mut out = Vec::new();
    for (range, style) in styles {
        let lo = range.start.max(start);
        let hi = range.end.min(end);
        if lo < hi {
            out.push((lo - start..hi - start, *style));
        }
    }
    out
}

/// Token colors plus the emphasis band, split so the two kinds of highlight
/// never overlap (a `StyledText` run list is a partition, not a layer stack).
fn merged_runs(
    text_len: usize,
    tokens: &[(Range<usize>, HighlightStyle)],
    emphasis: &[Range<usize>],
    emphasis_bg: Option<Hsla>,
) -> Vec<(Range<usize>, HighlightStyle)> {
    let Some(emphasis_bg) = emphasis_bg else {
        return tokens.to_vec();
    };
    if emphasis.is_empty() {
        return tokens.to_vec();
    }
    let mut points = vec![0, text_len];
    for (range, _) in tokens {
        points.push(range.start);
        points.push(range.end);
    }
    for range in emphasis {
        points.push(range.start);
        points.push(range.end);
    }
    points.sort_unstable();
    points.dedup();

    let mut runs: Vec<(Range<usize>, HighlightStyle)> = Vec::new();
    for window in points.windows(2) {
        let (start, end) = (window[0], window[1]);
        if start >= end {
            continue;
        }
        let token_style = tokens
            .iter()
            .find(|(range, _)| range.start <= start && end <= range.end)
            .map(|(_, style)| *style);
        let emphasized = emphasis
            .iter()
            .any(|range| range.start <= start && end <= range.end);
        if token_style.is_none() && !emphasized {
            continue;
        }
        let mut run = token_style.unwrap_or_default();
        if emphasized {
            run.background_color = Some(emphasis_bg);
        }
        if let Some(last) = runs.last_mut()
            && last.0.end == start
            && last.1 == run
        {
            last.0.end = end;
        } else {
            runs.push((start..end, run));
        }
    }
    runs
}

// ---- code rendering --------------------------------------------------------

/// Highlights every content line in one buffer pass (block comments and
/// strings run across lines) and splits the result back per line, offsets
/// relative to each line's content start.
fn line_tokens(
    lines: &[&str],
    kinds: &[RowKind],
    lang: &str,
    theme: &Arc<gpui_kit::component::highlighter::HighlightTheme>,
) -> Vec<Vec<(Range<usize>, HighlightStyle)>> {
    let mut buffer = String::new();
    let mut starts = Vec::with_capacity(lines.len());
    let mut lens = Vec::with_capacity(lines.len());
    for (i, line) in lines.iter().enumerate() {
        starts.push(buffer.len());
        let content = match kinds[i] {
            RowKind::Add | RowKind::Del => &line[1..],
            RowKind::Context => *line,
        };
        lens.push(content.len());
        buffer.push_str(content);
        buffer.push('\n');
    }
    let styles = highlight::highlight_buffer(&buffer, lang, theme);
    let mut out: Vec<Vec<(Range<usize>, HighlightStyle)>> = vec![Vec::new(); lines.len()];
    for (i, &start) in starts.iter().enumerate() {
        out[i] = slice_runs(&styles, start, lens[i]);
    }
    out
}

/// Lifts `   12\t`-style prefixes into (base number, stripped lines). Needs a
/// tab separator and digits-only prefixes on most non-empty lines, so prose
/// or tab-separated data never trips it.
fn strip_line_numbers<'a>(lines: &[&'a str]) -> Option<(usize, Vec<&'a str>)> {
    let mut base: Option<usize> = None;
    let mut stripped = Vec::with_capacity(lines.len());
    let mut matched = 0usize;
    let mut total = 0usize;
    for line in lines {
        if line.trim().is_empty() {
            stripped.push(*line);
            continue;
        }
        total += 1;
        let (prefix, rest) = line.split_once('\t')?;
        let prefix = prefix.trim();
        if prefix.is_empty() || !prefix.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let number: usize = prefix.parse().ok()?;
        base = Some(match base {
            Some(b) => b.min(number),
            None => number,
        });
        matched += 1;
        stripped.push(rest);
    }
    (total > 1 && matched * 5 >= total * 4).then(|| (base.unwrap_or(1), stripped))
}

fn code_style(window: &mut Window, font_detail: f32, color: gpui::Rgba) -> TextStyle {
    let mut style = window.text_style();
    style.font_family = fonts::code_font().into();
    style.font_size = AbsoluteLength::Pixels(px(font_detail));
    style.color = color.into();
    style
}

/// The block's root: the code family and `font_detail` pinned as the ambient
/// style every row underneath inherits. Without this the rows are shaped at
/// whatever size the surrounding view set, and `code_style`'s size only
/// reaches the run's font, not its metrics (see the module docs).
fn block_root(font_detail: f32) -> gpui::Div {
    div()
        .w_full()
        .flex()
        .flex_col()
        .font_family(fonts::code_font())
        .text_size(px(font_detail))
}

/// One row of text on the ambient background; empty lines keep a single
/// space so the row still has line height.
fn styled_text(
    text: &str,
    style: &TextStyle,
    highlights: &[(Range<usize>, HighlightStyle)],
) -> StyledText {
    let shown: &str = if text.is_empty() { " " } else { text };
    StyledText::new(shown.to_string()).with_default_highlights(style, highlights.iter().cloned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_tool_prefixes_lift_into_gutter() {
        let text = "   1\tfn a() {}\n   2\t\n  10\tfn b() {}\n";
        let lines: Vec<&str> = text.split('\n').collect();
        let lines = &lines[..lines.len() - 1];
        assert_eq!(
            strip_line_numbers(lines),
            Some((1, vec!["fn a() {}", "", "fn b() {}"]))
        );
        // prose with a stray tab stays untouched
        assert_eq!(strip_line_numbers(&["col1\tcol2", "col3\tcol4"]), None);
        assert_eq!(strip_line_numbers(&["just one line\t1"]), None);
    }
}
