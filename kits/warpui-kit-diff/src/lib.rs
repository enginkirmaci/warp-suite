//! warpui-kit-diff: Zed-style unified and split diff plus line-numbered code
//! rendering for gpui-kit applications.
//!
//! [`render_diff`] turns a unified patch into a [`document::DiffDocument`] and
//! draws the Zed-style unified view: one line-number column of new-file
//! numbers (deletions carry none, since their old number is not a line in the
//! file), a +/− marker
//! cell, a strong wash over the gutter and a quieter one over the code, a
//! change bar on the row's leading edge, and tree-sitter token colors inside
//! the lines. Changed words inside a paired deletion/addition are emphasized.
//!
//! [`render_diff_with`] chooses the layout. [`DiffMode::Unified`] is the
//! one-column view above; [`DiffMode::Split`] draws the old file on the left
//! and the new file on the right, paired row for row, with one shared gutter
//! width and a 45° slash-hatched filler where one side has no row. A
//! multi-file patch stays unified. Hunk headers are parsed but never drawn:
//! the `-U3` context is the only gap between hunks, and a selection never
//! carries `@@` lines.
//!
//! By default rows never wrap: one row is one line, and the body scrolls
//! horizontally so a long line keeps its shape. Split puts its two columns in
//! separate viewports, so a long line on one side never pushes the other side
//! off screen. Both viewports share one content width, and the scrolled pane
//! mirrors their offsets, so the two sides pan together and stay aligned; the
//! shorter side does not stop at its own last line. (A plain
//! [`render_diff_with`] block keeps the two viewports independent, since it
//! has no handles to hold one offset across frames.) `DiffOptions::wrap`
//! switches to a row-per-line body where the code wraps inside its column and
//! the number stays on the first visual line; in split a row takes the taller
//! side's height so both columns stay aligned.
//!
//! [`render_diff_with_scroll`] also owns the vertical viewport. That is what
//! keeps a horizontal wheel step from being read as vertical movement by a
//! parent scroller: the vertical frame is axis-restricted, the horizontal
//! viewport consumes only horizontal deltas, and the wrapped body is a
//! virtualised list that builds just the rows near its viewport. The plain
//! [`render_diff_with`] stays in flow for an embedded block and is not a
//! scrolling pane.
//!
//! Unwrapped unified is one selectable text: a drag crosses lines and Copy
//! gives the patch content without the gutter numbers or markers. Unwrapped
//! split has one participant per column, so a drag stays on its side. Wrapped
//! rows are separate selectable texts, so a drag stays on that row. The
//! gutter and row washes are drawn beside and behind the text. The host must
//! mount `gpui_kit::base::TextSelectionLayer` for the drag to register.
//!
//! [`render_code`] is the same presentation for plain code: a line-number
//! gutter plus token colors, no washes. Pre-numbered tool output (`   12\t…`
//! prefixes) gets its prefixes lifted into the real gutter instead of
//! double-numbering.
//!
//! [`patch_from_hunks`] is the entry for a backend that reports an applied
//! change as before/after text per hunk instead of patch text (dsh's `write`
//! and `edit` results do): it re-diffs each pair into the unified form the
//! parser above reads, so the same renderer draws git patches and tool hunks.
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

mod aligned;
mod document;
mod highlight;
mod selectable;

pub use aligned::{
    AlignedOptions, AlignedRow, AlignedSide, aligned_pane_head, aligned_pick_box, render_aligned,
    row_height, visible_window,
};
pub use document::{
    DiffDocument, DiffFile, DiffHunk, DiffRow, HunkText, RowKind, SplitRow, lang_from_path,
    paired_word_emphasis, patch_from_hunks,
};
pub use highlight::highlight_buffer;
use selectable::SelectableText;

use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

use gpui::prelude::*;
use gpui::{
    AbsoluteLength, AnyElement, App, Background, BorderStyle, Bounds, Corners, Div, Edges,
    ElementId, HighlightStyle, Hsla, ListAlignment, ListState, PaintQuad, Pixels, ScrollHandle,
    SharedString, StyledText, TextRun, TextStyle, Window, canvas, div, list, pattern_slash, point,
    px, relative, size, transparent_black,
};
use unicode_width::UnicodeWidthStr as _;

use warpui_kit_core::fonts;
use warpui_kit_core::palette;

/// The design's diff pitch: mono 13 / 21. A ratio, so the rows keep the same
/// rhythm at every text-size step the caller passes.
const DIFF_LINE_HEIGHT: f32 = 21.0 / 13.0;

/// Slack on the widest shaped line before it becomes both columns' minimum
/// code width: the code cell's own leading, plus a little for a token run or
/// a fallback glyph shaping slightly wider than the plain line measurement.
const SPLIT_WIDTH_SLACK: f32 = 24.0;

thread_local! {
    /// Parsed patches, keyed by (text hash, length). Parsing includes the word
    /// diff behind intra-line emphasis, so a re-render of an unchanged patch
    /// must not repeat it. A full clear on overflow is fine: entries are cheap
    /// to rebuild, never stale.
    static DOCUMENT_CACHE: RefCell<Vec<(u64, usize, Rc<DiffDocument>)>> = const { RefCell::new(Vec::new()) };
}

/// How a patch is laid out: one column (unified) or two (split).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DiffMode {
    #[default]
    Unified,
    Split,
}

/// Options for [`render_diff_with`]. `Default` is the unified, unwrapped view.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DiffOptions {
    pub mode: DiffMode,
    /// Wrap long rows instead of scrolling horizontally. A wrapped row keeps
    /// its number on the first visual line. In the scrolled pane the wrapped
    /// rows are a virtualised list, so only the visible rows are built; the
    /// plain [`render_diff_with`] still draws one element per row.
    pub wrap: bool,
}

/// Scroll handles the app keeps across frames so it can draw pinned
/// horizontal scrollbars outside the vertical scroller. The kit owns the
/// vertical scroller too (see [`render_diff_with_scroll`]), so a horizontal
/// wheel can never be turned into vertical movement by a parent scroller. In
/// split the two columns move together: [`left`](Self::left) and
/// [`right`](Self::right) mirror each other, so both bars show one position.
/// An app that does not draw bars can use the plain [`render_diff_with`],
/// which passes none and leaves the body in flow.
#[derive(Clone, Default)]
pub struct DiffScrollHandles {
    pub unified: ScrollHandle,
    pub left: ScrollHandle,
    pub right: ScrollHandle,
    /// The split columns' last horizontal offsets, so a step in either
    /// viewport is mirrored into the other on the next render.
    split_sync: Rc<RefCell<SplitSync>>,
    /// The vertical viewport of the unwrapped body. Wrapped bodies virtualise
    /// their rows with a list, which owns its own scroll offset.
    pub vertical: ScrollHandle,
    /// The wrapped body's list state, replaced when the patch or font
    /// changes. Shared through the handles so the app keeps one state across
    /// frames without the kit parsing the patch again.
    wrap_list: Rc<RefCell<Option<WrapList>>>,
}

struct WrapList {
    key: u64,
    state: ListState,
}

/// The two split offsets as the last render left them. A side that moved
/// since then is the one the user scrolled; the other follows it.
#[derive(Default)]
struct SplitSync {
    left: Option<Pixels>,
    right: Option<Pixels>,
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
    render_diff_with(
        text,
        path_hint,
        DiffOptions::default(),
        font_detail,
        window,
        cx,
    )
}

/// Like [`render_diff`], with a layout mode. Split is honoured for a
/// single-file patch; a multi-file patch stays unified (the callers that offer
/// the toggle show one file at a time).
pub fn render_diff_with(
    text: &str,
    path_hint: Option<&str>,
    options: DiffOptions,
    font_detail: f32,
    window: &mut Window,
    cx: &App,
) -> AnyElement {
    if text.is_empty() {
        return div().into_any_element();
    }
    let doc = cached_document(text);
    render_document(&doc, path_hint, options, None, font_detail, window, cx)
}

/// Like [`render_diff_with`], but tracks the horizontal viewports in `scroll`
/// so the caller can draw pinned scrollbars, and owns the vertical viewport.
/// The caller must give the returned element a height-constrained parent (a
/// pane that fills its column); it does not size to its content. The handles
/// must outlive the render and be reused; a fresh set every frame loses the
/// scroll offsets and the wrapped list's measured rows.
pub fn render_diff_with_scroll(
    text: &str,
    path_hint: Option<&str>,
    options: DiffOptions,
    scroll: &DiffScrollHandles,
    font_detail: f32,
    window: &mut Window,
    cx: &App,
) -> AnyElement {
    if text.is_empty() {
        return div().into_any_element();
    }
    let doc = cached_document(text);
    render_document(
        &doc,
        path_hint,
        options,
        Some(scroll),
        font_detail,
        window,
        cx,
    )
}

/// Draw an already parsed patch. [`render_diff_with`] and
/// [`render_diff_with_scroll`] parse and cache the text first, so a re-render of
/// an unchanged patch reaches this without reparsing.
fn render_document(
    doc: &DiffDocument,
    path_hint: Option<&str>,
    options: DiffOptions,
    scroll: Option<&DiffScrollHandles>,
    font_detail: f32,
    window: &mut Window,
    cx: &App,
) -> AnyElement {
    if doc.files.is_empty() {
        return div().into_any_element();
    }
    let hl_theme = gpui_kit::component::Theme::global(cx)
        .highlight_theme
        .clone();
    let paint_style = code_style(window, font_detail, palette::CODE_FG);
    let number_style = HighlightStyle {
        color: Some(palette::DIFF_GUTTER.into()),
        ..Default::default()
    };
    let line_height = line_height_px(&paint_style, window);
    // The filler hatch's period is half a row, so a wrapped row (always a
    // whole number of rows tall) lands on the same phase as its neighbour and
    // consecutive blank rows read as one continuous pattern.
    let scale = window.scale_factor();
    let period = (f32::from(line_height) / 2.0).max(2.0);
    let stripe = (period * 0.15).max(1.0);
    let fill_hatch = pattern_slash(
        palette::diff_fill_hatch(),
        stripe * scale,
        (period - stripe) * scale,
    );

    let split_file = if options.mode == DiffMode::Split
        && doc.files.len() == 1
        && !doc.files[0].binary
        && !doc.files[0].truncated
        && !doc.files[0].hunks.is_empty()
    {
        Some(&doc.files[0])
    } else {
        None
    };

    if let Some(file) = split_file {
        if options.wrap {
            return render_split_wrapped(
                file,
                path_hint,
                &hl_theme,
                paint_style,
                number_style,
                font_detail,
                scroll,
                fill_hatch,
            );
        }
        return render_split(
            file,
            path_hint,
            &hl_theme,
            paint_style,
            number_style,
            line_height,
            font_detail,
            scroll,
            fill_hatch,
            window,
        );
    }
    if options.wrap {
        return render_unified_wrapped(
            doc,
            path_hint,
            &hl_theme,
            paint_style,
            number_style,
            font_detail,
            scroll,
        );
    }
    render_unified(
        doc,
        path_hint,
        &hl_theme,
        paint_style,
        number_style,
        line_height,
        font_detail,
        scroll,
    )
}

/// The unified body: every file's rows in one column. File headers and state
/// rows span the column; metadata and hunk headers take no rows.
#[expect(
    clippy::too_many_arguments,
    reason = "the render call carries the theme, paint and scroll inputs"
)]
fn render_unified(
    doc: &DiffDocument,
    path_hint: Option<&str>,
    hl_theme: &Arc<gpui_kit::component::highlighter::HighlightTheme>,
    paint_style: TextStyle,
    number_style: HighlightStyle,
    line_height: Pixels,
    font_detail: f32,
    scroll: Option<&DiffScrollHandles>,
) -> AnyElement {
    let gutter_width = doc.max_number().to_string().len().max(4);
    let paint = RowPaint {
        style: paint_style,
        number_style,
        gutter_width,
    };
    let lines = unified_lines(doc, path_hint, hl_theme);
    if lines.is_empty() {
        return div().into_any_element();
    }
    let column = render_column(
        lines,
        &paint,
        line_height,
        font_detail,
        &ColumnIds::UNIFIED,
        ColumnWidth::Full,
        None,
        None,
    );
    let body = diff_scroller(
        column.into_any_element(),
        font_detail,
        scroll.map(|scroll| &scroll.unified),
    );
    match scroll {
        Some(scroll) => {
            vertical_frame(body.into_any_element(), &scroll.vertical).into_any_element()
        }
        None => body.into_any_element(),
    }
}

/// The split body: old file left, new file right, one line per paired row.
/// Only a single-file patch reaches here; state-only and multi-file patches
/// stay unified.
#[expect(
    clippy::too_many_arguments,
    reason = "the render call carries the theme, paint and scroll inputs"
)]
fn render_split(
    file: &DiffFile,
    path_hint: Option<&str>,
    hl_theme: &Arc<gpui_kit::component::highlighter::HighlightTheme>,
    paint_style: TextStyle,
    number_style: HighlightStyle,
    line_height: Pixels,
    font_detail: f32,
    scroll: Option<&DiffScrollHandles>,
    fill_hatch: Background,
    window: &Window,
) -> AnyElement {
    let gutter_width = split_gutter_width(file);
    let paint = RowPaint {
        style: paint_style,
        number_style,
        gutter_width,
    };
    let (left, right) = split_lines(file, path_hint, hl_theme);
    if left.is_empty() {
        return div().into_any_element();
    }
    if let Some(scroll) = scroll {
        sync_split_scroll(scroll);
    }
    // Both columns get one content width, so their viewports have the same
    // scroll range: with a column-wide range a short side would stop before
    // the long one and the two would no longer pan together.
    let content_width =
        widest_line(window, &paint.style, &left).max(widest_line(window, &paint.style, &right))
            + px(SPLIT_WIDTH_SLACK);
    let left = render_column(
        left,
        &paint,
        line_height,
        font_detail,
        &ColumnIds::LEFT,
        ColumnWidth::Split,
        Some(fill_hatch),
        Some(content_width),
    );
    let right = render_column(
        right,
        &paint,
        line_height,
        font_detail,
        &ColumnIds::RIGHT,
        ColumnWidth::Split,
        Some(fill_hatch),
        Some(content_width),
    );
    let body = div()
        .debug_selector(|| "diff-split".into())
        .w_full()
        .flex()
        .flex_row()
        .items_stretch()
        .child(column_viewport(
            &ColumnIds::LEFT,
            left,
            scroll.map(|scroll| &scroll.left),
        ))
        .child(div().flex_shrink_0().w(px(1.)).bg(palette::hairline()))
        .child(column_viewport(
            &ColumnIds::RIGHT,
            right,
            scroll.map(|scroll| &scroll.right),
        ));
    let body = split_frame(body.into_any_element(), font_detail);
    match scroll {
        Some(scroll) => {
            vertical_frame(body.into_any_element(), &scroll.vertical).into_any_element()
        }
        None => body.into_any_element(),
    }
}

/// The language a file's rows are highlighted with: its own path first, the
/// caller's hint only as the fallback. A multi-file patch can mix languages, so
/// the grammar is resolved per file inside the render loop.
fn file_language(file: &DiffFile, path_hint: Option<&str>) -> Option<&'static str> {
    file.path()
        .and_then(lang_from_path)
        .or_else(|| path_hint.and_then(lang_from_path))
}

/// The unified body's lines: file headers only for a multi-file patch, then
/// one line per code row. Metadata and hunk headers take no rows.
fn unified_lines(
    doc: &DiffDocument,
    path_hint: Option<&str>,
    hl_theme: &Arc<gpui_kit::component::highlighter::HighlightTheme>,
) -> Vec<Line> {
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
        let language = file_language(file, path_hint);
        for hunk in &file.hunks {
            let tokens = highlight_hunk(hunk, language, hl_theme);
            for (index, row) in hunk.rows.iter().enumerate() {
                let runs = match row.kind {
                    RowKind::Del => tokens.old[index].clone(),
                    RowKind::Context | RowKind::Add => tokens.new[index].clone(),
                };
                lines.push(Line::code(row, runs));
            }
        }
        if file.truncated {
            lines.push(Line::state("Patch truncated at 64 KB"));
        }
    }
    lines
}

/// The paired columns of one file's split body.
fn split_lines(
    file: &DiffFile,
    path_hint: Option<&str>,
    hl_theme: &Arc<gpui_kit::component::highlighter::HighlightTheme>,
) -> (Vec<Line>, Vec<Line>) {
    let mut left = Vec::new();
    let mut right = Vec::new();
    let language = file_language(file, path_hint);
    for hunk in &file.hunks {
        let tokens = highlight_hunk(hunk, language, hl_theme);
        for pair in hunk.split_rows() {
            left.push(match pair.left {
                Some(index) => {
                    let row = &hunk.rows[index];
                    Line::cell(row, row.old, tokens.old[index].clone())
                }
                None => Line::fill(),
            });
            right.push(match pair.right {
                Some(index) => {
                    let row = &hunk.rows[index];
                    Line::cell(row, row.new, tokens.new[index].clone())
                }
                None => Line::fill(),
            });
        }
    }
    (left, right)
}

fn split_gutter_width(file: &DiffFile) -> usize {
    let old = file
        .hunks
        .iter()
        .flat_map(|hunk| &hunk.rows)
        .filter_map(|row| row.old)
        .max()
        .unwrap_or(0);
    let new = file
        .hunks
        .iter()
        .flat_map(|hunk| &hunk.rows)
        .filter_map(|row| row.new)
        .max()
        .unwrap_or(0);
    old.max(new).to_string().len().max(4)
}

/// The wrapped unified body. Each source row is an element so a long line
/// wraps at the code column; the gutter number stays on its first visual line.
fn render_unified_wrapped(
    doc: &DiffDocument,
    path_hint: Option<&str>,
    hl_theme: &Arc<gpui_kit::component::highlighter::HighlightTheme>,
    paint_style: TextStyle,
    number_style: HighlightStyle,
    font_detail: f32,
    scroll: Option<&DiffScrollHandles>,
) -> AnyElement {
    let lines = unified_lines(doc, path_hint, hl_theme);
    if lines.is_empty() {
        return div().into_any_element();
    }
    let paint = RowPaint {
        style: paint_style,
        number_style,
        gutter_width: doc.max_number().to_string().len().max(4),
    };
    if let Some(scroll) = scroll {
        return render_unified_wrapped_list(lines, paint, font_detail, scroll);
    }
    let mut body = div()
        .debug_selector(|| "diff-wrap-body".into())
        .w_full()
        .flex()
        .flex_col();
    for (index, line) in lines.iter().enumerate() {
        body = body.child(wrapped_row(line, &paint, font_detail, index));
    }
    wrapped_frame(body.into_any_element(), font_detail).into_any_element()
}

/// The wrapped split body. A row's two cells are siblings of the same flex
/// row, so the row is as tall as its taller wrapped side and the next row
/// starts on the same line for both columns.
#[expect(
    clippy::too_many_arguments,
    reason = "the render call carries the theme, paint, scroll and filler inputs"
)]
fn render_split_wrapped(
    file: &DiffFile,
    path_hint: Option<&str>,
    hl_theme: &Arc<gpui_kit::component::highlighter::HighlightTheme>,
    paint_style: TextStyle,
    number_style: HighlightStyle,
    font_detail: f32,
    scroll: Option<&DiffScrollHandles>,
    fill_hatch: Background,
) -> AnyElement {
    let (left, right) = split_lines(file, path_hint, hl_theme);
    if left.is_empty() {
        return div().into_any_element();
    }
    let paint = RowPaint {
        style: paint_style,
        number_style,
        gutter_width: split_gutter_width(file),
    };
    if let Some(scroll) = scroll {
        return render_split_wrapped_list(left, right, paint, font_detail, scroll, fill_hatch);
    }
    let mut body = div()
        .debug_selector(|| "diff-wrap-body".into())
        .w_full()
        .flex()
        .flex_col();
    for index in 0..left.len() {
        body = body.child(wrapped_split_row(
            &left[index],
            &right[index],
            &paint,
            font_detail,
            index,
            fill_hatch,
        ));
    }
    wrapped_frame(body.into_any_element(), font_detail).into_any_element()
}

/// The scrolled wrapped unified body: a virtualised list, so a wheel tick
/// rebuilds only the visible rows instead of every row in the patch. The
/// list owns the vertical scroll, which is what lets a horizontal wheel event
/// pass through without also moving the body.
fn render_unified_wrapped_list(
    lines: Vec<Line>,
    paint: RowPaint,
    font_detail: f32,
    scroll: &DiffScrollHandles,
) -> AnyElement {
    let key = wrapped_key(1, font_detail, &[&lines]);
    let state = wrap_list_state(scroll, key, lines.len());
    let lines = Rc::new(lines);
    let item_paint = paint.clone();
    wrap_list_frame(state, font_detail, move |index, _, _| {
        wrapped_row(&lines[index], &item_paint, font_detail, index).into_any_element()
    })
}

/// The scrolled wrapped split body. One list row still holds both cells, so
/// the two sides stay aligned when only one side wraps.
fn render_split_wrapped_list(
    left: Vec<Line>,
    right: Vec<Line>,
    paint: RowPaint,
    font_detail: f32,
    scroll: &DiffScrollHandles,
    fill_hatch: Background,
) -> AnyElement {
    let key = wrapped_key(2, font_detail, &[&left, &right]);
    let state = wrap_list_state(scroll, key, left.len());
    let left = Rc::new(left);
    let right = Rc::new(right);
    let item_paint = paint.clone();
    wrap_list_frame(state, font_detail, move |index, _, _| {
        wrapped_split_row(
            &left[index],
            &right[index],
            &item_paint,
            font_detail,
            index,
            fill_hatch,
        )
        .into_any_element()
    })
}

/// The list element and its frame. The frame repeats the code family and the
/// size so list items, which are rendered outside the normal row tree, keep
/// the same metrics as the unwrapped column.
fn wrap_list_frame(
    state: ListState,
    font_detail: f32,
    render_item: impl FnMut(usize, &mut Window, &mut App) -> AnyElement + 'static,
) -> AnyElement {
    div()
        .debug_selector(|| "diff-wrap-body".into())
        .size_full()
        .flex()
        .flex_col()
        .min_h_0()
        .font_family(fonts::code_font())
        .text_size(px(font_detail))
        .line_height(relative(DIFF_LINE_HEIGHT))
        .whitespace_normal()
        .child(list(state, render_item).size_full())
        .into_any_element()
}

/// One persistent list state per patch and font size. A different patch
/// replaces it; a width change is handled inside the list, which remeasures
/// its items. The key hashes the line text, numbers and font step, the
/// inputs that change a row's wrapped height.
fn wrap_list_state(handles: &DiffScrollHandles, key: u64, count: usize) -> ListState {
    let mut slot = handles.wrap_list.borrow_mut();
    if let Some(wrap) = slot.as_ref()
        && wrap.key == key
        && wrap.state.item_count() == count
    {
        return wrap.state.clone();
    }
    let state = ListState::new(count, ListAlignment::Top, px(400.));
    *slot = Some(WrapList {
        key,
        state: state.clone(),
    });
    state
}

fn wrapped_key(salt: u64, font_detail: f32, columns: &[&[Line]]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    salt.hash(&mut hasher);
    font_detail.to_bits().hash(&mut hasher);
    for lines in columns {
        lines.len().hash(&mut hasher);
        for line in *lines {
            line.text.hash(&mut hasher);
            std::mem::discriminant(&line.kind).hash(&mut hasher);
            line.number.hash(&mut hasher);
        }
    }
    hasher.finish()
}

/// The wrapped frame: the code family, the caller's size and normal white
/// space. No horizontal scroller: a wrapped row is never wider than its cell.
fn wrapped_frame(child: AnyElement, font_detail: f32) -> impl IntoElement {
    div()
        .w_full()
        .flex()
        .flex_col()
        .font_family(fonts::code_font())
        .text_size(px(font_detail))
        .line_height(relative(DIFF_LINE_HEIGHT))
        .whitespace_normal()
        .child(child)
}

fn wrapped_row(line: &Line, paint: &RowPaint, font_detail: f32, index: usize) -> Div {
    div()
        .w_full()
        .flex()
        .flex_row()
        .items_stretch()
        .child(wrapped_cell(line, paint, font_detail, index, "u", None).w_full())
}

fn wrapped_split_row(
    left: &Line,
    right: &Line,
    paint: &RowPaint,
    font_detail: f32,
    index: usize,
    fill_hatch: Background,
) -> Div {
    div()
        .w_full()
        .flex()
        .flex_row()
        .items_stretch()
        .child(
            wrapped_cell(left, paint, font_detail, index, "l", Some(fill_hatch))
                .flex_1()
                .min_w_0(),
        )
        .child(div().flex_shrink_0().w(px(1.)).bg(palette::hairline()))
        .child(
            wrapped_cell(right, paint, font_detail, index, "r", Some(fill_hatch))
                .flex_1()
                .min_w_0(),
        )
}

/// One wrapped row's gutter and code cells. The gutter is a single line; the
/// code text is its own `SelectableText` and wraps at the code cell's width.
/// Wrap mode therefore selects within a row; the unwrapped body keeps the one
/// selection across the whole patch.
fn wrapped_cell(
    line: &Line,
    paint: &RowPaint,
    font_detail: f32,
    row_index: usize,
    side: &'static str,
    fill_hatch: Option<Background>,
) -> Div {
    let wash = line_wash(line.kind);
    let gutter_text = match line.kind {
        LineKind::Code(kind) => {
            let (content, highlights) = gutter_content(line.number, kind, paint);
            styled_text(&content, &paint.style, &highlights)
        }
        _ => {
            let blank = "\u{00a0}".repeat(paint.gutter_width + 2);
            styled_text(
                &blank,
                &paint.style,
                &[(0..blank.len(), paint.number_style)],
            )
        }
    };
    let mut gutter = div()
        .flex_shrink_0()
        .relative()
        .pl(px(11.))
        .pr_2()
        .when_some(wash.gutter, |cell, color| cell.bg(color));
    if let Some(bar) = wash.bar {
        gutter = gutter.child(
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .left_0()
                .w(px(3.))
                .bg(bar),
        );
    }
    let gutter = gutter.child(div().whitespace_nowrap().child(gutter_text));

    let runs = merged_runs(
        line.text.len(),
        &line.tokens,
        &line.emphasis,
        line.emphasis_bg,
    );
    let shown = if line.text.is_empty() {
        "\u{00a0}".to_string()
    } else {
        line.text.clone()
    };
    let styled = StyledText::new(shown.clone()).with_default_highlights(&paint.style, runs.clone());
    let key = styled_key(&shown, &runs, font_detail)
        .wrapping_mul(31)
        .wrapping_add(row_index as u64)
        .wrapping_add(match side {
            "l" => 1,
            "r" => 2,
            _ => 0,
        });
    let name = match side {
        "l" => "diff-wrap-left",
        "r" => "diff-wrap-right",
        _ => "diff-wrap-unified",
    };
    let selectable = SelectableText::new(
        ElementId::NamedInteger(name.into(), key),
        shown.into(),
        styled,
        key,
    );
    let code = div()
        .flex_1()
        .min_w_0()
        .when_some(wash.content, |cell, color| cell.bg(color))
        .child(
            div()
                .debug_selector(move || format!("diff-wrap-code-{row_index}-{side}"))
                .pl_2()
                .child(selectable),
        );
    let cell = div()
        .flex()
        .flex_row()
        .items_stretch()
        .child(gutter)
        .child(code);
    match (line.kind, fill_hatch) {
        (LineKind::Fill, Some(hatch)) => cell.bg(hatch),
        _ => cell,
    }
}

/// One split column's viewport. The column body keeps at least the viewport's
/// width and grows with a long line; the viewport scrolls that line instead of
/// pushing the other column off screen. Both viewports share one content
/// width, and their offsets are mirrored before layout, so they pan together.
fn column_viewport(
    ids: &'static ColumnIds,
    body: Div,
    handle: Option<&ScrollHandle>,
) -> impl IntoElement {
    let viewport = div()
        .id(ids.viewport)
        .debug_selector(move || ids.viewport.into())
        .flex_1()
        .flex()
        .flex_row()
        .min_w_0()
        .overflow_x_scroll()
        .restrict_scroll_to_axis();
    let viewport = match handle {
        Some(handle) => viewport.track_scroll(handle),
        None => viewport,
    };
    viewport.child(body)
}

/// Keep the split columns on one horizontal offset. The viewport that moved
/// since the previous render is the one the user scrolled, and the other takes
/// its offset before this frame lays out, so the two paint together.
fn sync_split_scroll(scroll: &DiffScrollHandles) {
    let mut sync = scroll.split_sync.borrow_mut();
    let left = scroll.left.offset().x;
    let right = scroll.right.offset().x;
    let (Some(last_left), Some(last_right)) = (sync.left, sync.right) else {
        // The first frame has no baseline to tell a user step from a clamp.
        sync.left = Some(left);
        sync.right = Some(right);
        return;
    };
    let target = match (left != last_left, right != last_right) {
        (true, false) => left,
        (false, true) => right,
        // Both moved: a new patch or a resize re-clamped both. Take the
        // smaller step so neither side is left past its own content.
        (true, true) => left.max(right),
        (false, false) => return,
    };
    if left != target {
        scroll
            .left
            .set_offset(point(target, scroll.left.offset().y));
    }
    if right != target {
        scroll
            .right
            .set_offset(point(target, scroll.right.offset().y));
    }
    sync.left = Some(target);
    sync.right = Some(target);
}

/// The widest line in one split column, shaped with the column's own face.
/// Both columns get the larger of the two widths, so the longest line decides
/// the shared scroll range. Picking the line with the most cells first means
/// one shape per side, not one per row.
fn widest_line(window: &Window, style: &TextStyle, lines: &[Line]) -> Pixels {
    let Some(line) = lines.iter().max_by_key(|line| visual_cells(&line.text)) else {
        return Pixels::ZERO;
    };
    if line.text.is_empty() {
        return Pixels::ZERO;
    }
    let font_size = style.font_size.to_pixels(window.rem_size());
    let run = TextRun {
        len: line.text.len(),
        font: style.font(),
        color: style.color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    window
        .text_system()
        .shape_line(
            SharedString::from(line.text.clone()),
            font_size,
            &[run],
            None,
        )
        .width
}

/// A line's width in monospace cells: the code face is monospaced, so the
/// cell count picks the widest line. A tab jumps a tab stop; Unicode width
/// counts the double-width glyphs.
fn visual_cells(text: &str) -> usize {
    text.width() + text.matches('\t').count() * 3
}

/// One column of a diff body: the gutter, the selectable code text, and the
/// canvas that paints every row's wash. Unified has one; split has two. Split
/// also passes the `hatch` it lays under an unpaired row's whole side.
#[expect(
    clippy::too_many_arguments,
    reason = "one column carries the paint, geometry, ids and split width inputs"
)]
fn render_column(
    lines: Vec<Line>,
    paint: &RowPaint,
    line_height: Pixels,
    font_detail: f32,
    ids: &'static ColumnIds,
    width: ColumnWidth,
    hatch: Option<Background>,
    code_min_width: Option<Pixels>,
) -> Div {
    // One buffer for the whole column: selection and copy work across lines
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
            LineKind::State => {
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
            LineKind::File | LineKind::Fill => {}
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
                let (content, highlights) = gutter_content(line.number, kind, paint);
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
        ElementId::NamedInteger(ids.selection.into(), selection_seed),
        text.into(),
        styled,
        layout_key,
    );

    let body = div()
        .debug_selector(move || ids.body.into())
        .relative()
        .flex()
        .flex_row();
    let mut body = match width {
        ColumnWidth::Full => body.flex_shrink_0().min_w_full(),
        ColumnWidth::Split => body.flex_shrink_0().min_w_full(),
    };

    // A row with no line on this side is filler, and the whole side reads as
    // deliberately empty only when the hatch runs unbroken across the run.
    // One element per run, behind the two text cells.
    if let Some(hatch) = hatch {
        let runs = fill_runs(&lines);
        for (index, run) in runs.iter().enumerate() {
            body = body.child(
                div()
                    .debug_selector(move || format!("{}-{index}", ids.fill))
                    .absolute()
                    .left_0()
                    .right_0()
                    .top(line_height * run.start as f32)
                    .h(line_height * run.len() as f32)
                    .bg(hatch),
            );
        }
    }

    // The code column holds the one selectable text; the canvases sit behind
    // it and behind the gutter.
    let mut code = div()
        .debug_selector(move || ids.code.into())
        .flex_1()
        .relative()
        .child(code_canvas);
    if let Some(min_width) = code_min_width {
        code = code.min_w(min_width);
    }
    body.child(
        div()
            .debug_selector(move || ids.gutter.into())
            .relative()
            .flex_shrink_0()
            .child(gutter_canvas)
            .child(div().pl(px(11.)).pr_2().child(gutter_styled)),
    )
    .child(
        code.child(
            div()
                .debug_selector(move || ids.text.into())
                .pl_2()
                .child(selectable),
        ),
    )
}

fn diff_scroller(
    child: AnyElement,
    font_detail: f32,
    handle: Option<&ScrollHandle>,
) -> impl IntoElement {
    let frame = div()
        .id("diff-x-scroll")
        .debug_selector(|| "diff-x-scroll".into())
        .w_full()
        .flex()
        .flex_row()
        .overflow_x_scroll()
        .restrict_scroll_to_axis()
        .font_family(fonts::code_font())
        .text_size(px(font_detail))
        .line_height(relative(DIFF_LINE_HEIGHT))
        .whitespace_nowrap();
    let frame = match handle {
        Some(handle) => frame.track_scroll(handle),
        None => frame,
    };
    frame.child(child)
}

/// The scrolled body's one vertical viewport. It is axis-restricted, so a
/// horizontal wheel step cannot fall through to the vertical axis the way a
/// plain `overflow_y_scroll` parent does when the pointer is over a nested
/// horizontal strip.
fn vertical_frame(child: AnyElement, handle: &ScrollHandle) -> impl IntoElement {
    div()
        .id("diff-y-scroll")
        .debug_selector(|| "diff-y-scroll".into())
        .size_full()
        .overflow_y_scroll()
        .restrict_scroll_to_axis()
        .track_scroll(handle)
        .child(child)
}

/// The split frame: no whole-body scroll. Each column's viewport owns its
/// horizontal scroll, so both columns stay visible.
fn split_frame(child: AnyElement, font_detail: f32) -> impl IntoElement {
    div()
        .debug_selector(|| "diff-split-frame".into())
        .w_full()
        .flex()
        .flex_row()
        .font_family(fonts::code_font())
        .text_size(px(font_detail))
        .line_height(relative(DIFF_LINE_HEIGHT))
        .whitespace_nowrap()
        .child(child)
}

#[derive(Clone, Copy)]
enum ColumnWidth {
    Full,
    Split,
}

/// The element ids and debug selectors of one column. Split needs two
/// selection participants and two sets of probe selectors.
struct ColumnIds {
    body: &'static str,
    fill: &'static str,
    viewport: &'static str,
    gutter: &'static str,
    code: &'static str,
    text: &'static str,
    selection: &'static str,
}

impl ColumnIds {
    const UNIFIED: Self = Self {
        body: "diff-body",
        fill: "diff-fill",
        viewport: "diff-viewport",
        gutter: "diff-gutter",
        code: "diff-code-column",
        text: "diff-text",
        selection: "diff-selection",
    };
    const LEFT: Self = Self {
        body: "diff-left-body",
        fill: "diff-left-fill",
        viewport: "diff-left-viewport",
        gutter: "diff-left-gutter",
        code: "diff-left-code-column",
        text: "diff-left-text",
        selection: "diff-selection-left",
    };
    const RIGHT: Self = Self {
        body: "diff-right-body",
        fill: "diff-right-fill",
        viewport: "diff-right-viewport",
        gutter: "diff-right-gutter",
        code: "diff-right-code-column",
        text: "diff-right-text",
        selection: "diff-selection-right",
    };
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

#[derive(Clone)]
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

/// One visual line of the diff. Code lines carry their highlight runs; state
/// and file lines are structure; a Fill line keeps one side of a split row
/// aligned when the other side has no code. The line's text is the selectable
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
    Code(RowKind),
    State,
    Fill,
}

impl Line {
    fn code(row: &DiffRow, tokens: Vec<(Range<usize>, HighlightStyle)>) -> Self {
        Self::cell(row, row.number(), tokens)
    }

    /// One side of a split row: `number` is the old number on the left and the
    /// new number on the right, and the wash still comes from the row kind.
    fn cell(
        row: &DiffRow,
        number: Option<u32>,
        tokens: Vec<(Range<usize>, HighlightStyle)>,
    ) -> Self {
        let (_, _, _, emphasis_bg) = row_colors(row.kind);
        Self {
            text: row.text.clone(),
            kind: LineKind::Code(row.kind),
            number,
            tokens,
            emphasis: row.emphasis.clone(),
            emphasis_bg,
        }
    }

    /// The empty side of an unpaired split row. The column lays the hatch
    /// behind it; a non-breaking space keeps the line height even when the
    /// filler is the last row.
    fn fill() -> Self {
        Self {
            text: "\u{00a0}".to_string(),
            kind: LineKind::Fill,
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
        LineKind::File => RowWash {
            gutter: Some(palette::surface().into()),
            bar: None,
            content: Some(palette::surface().into()),
        },
        LineKind::State | LineKind::Fill => RowWash {
            gutter: None,
            bar: None,
            content: None,
        },
    }
}

/// The runs of filler rows in one split column. Each run is one hatch
/// element, so the 45° pattern runs unbroken from the first blank row to the
/// last instead of restarting at every row.
fn fill_runs(lines: &[Line]) -> Vec<Range<usize>> {
    let mut runs: Vec<Range<usize>> = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if !matches!(line.kind, LineKind::Fill) {
            continue;
        }
        match runs.last_mut() {
            Some(run) if run.end == index => run.end = index + 1,
            _ => runs.push(index..index + 1),
        }
    }
    runs
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

/// Per-row token runs for both sides of a hunk. Unified picks the running
/// side; split draws the old runs left and the new runs right, so a context
/// row is styled from the file it is drawn in.
struct HunkHighlights {
    old: Vec<Vec<(Range<usize>, HighlightStyle)>>,
    new: Vec<Vec<(Range<usize>, HighlightStyle)>>,
}

/// Highlight each side of a hunk separately. Mixing deletions and additions in
/// one buffer lets a deleted brace or quote change the colors of the lines
/// that follow it; this keeps the old and new files as their own programs.
fn highlight_hunk(
    hunk: &DiffHunk,
    language: Option<&str>,
    theme: &Arc<gpui_kit::component::highlighter::HighlightTheme>,
) -> HunkHighlights {
    let mut out = HunkHighlights {
        old: vec![Vec::new(); hunk.rows.len()],
        new: vec![Vec::new(); hunk.rows.len()],
    };
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
        match row.kind {
            RowKind::Del => {
                if let Some((start, len)) = old_span[i] {
                    out.old[i] = slice_runs(&old_styles, start, len);
                }
            }
            RowKind::Add => {
                if let Some((start, len)) = new_span[i] {
                    out.new[i] = slice_runs(&new_styles, start, len);
                }
            }
            RowKind::Context => {
                if let Some((start, len)) = old_span[i] {
                    out.old[i] = slice_runs(&old_styles, start, len);
                }
                if let Some((start, len)) = new_span[i] {
                    out.new[i] = slice_runs(&new_styles, start, len);
                }
            }
        }
    }
    out
}

pub(crate) fn slice_runs(
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
pub(crate) fn merged_runs(
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
pub(crate) fn line_tokens(
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

/// Lifts `   12\t`-style prefixes into (base number, stripped lines). The
/// number column must be space-padded, as opencode's read tool writes it: an
/// unpadded numeric first column is exactly what tab-separated data looks
/// like, so lifting it would eat the table's first column.
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
        if !prefix.starts_with(' ') {
            return None;
        }
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

pub(crate) fn code_style(window: &mut Window, font_detail: f32, color: gpui::Rgba) -> TextStyle {
    let mut style = window.text_style();
    style.font_family = fonts::code_font().into();
    style.font_size = AbsoluteLength::Pixels(px(font_detail));
    style.line_height = relative(DIFF_LINE_HEIGHT);
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
        .line_height(relative(DIFF_LINE_HEIGHT))
}

/// One row of text on the ambient background; empty lines keep a single
/// space so the row still has line height.
pub(crate) fn styled_text(
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

    #[test]
    fn tab_separated_data_keeps_its_first_column() {
        // a table whose first column happens to be numeric is data, not a
        // gutter: the number column is not padded
        assert_eq!(strip_line_numbers(&["1\ta", "2\tb"]), None);
        assert_eq!(
            strip_line_numbers(&["   1\ta", "   2\tb"]),
            Some((1, vec!["a", "b"]))
        );
    }

    #[test]
    fn each_file_in_a_multi_file_patch_keeps_its_own_language() {
        let doc = DiffDocument::parse(
            "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-a\n+b\n\
             --- a/tool.py\n+++ b/tool.py\n@@ -1 +1 @@\n-a\n+b\n",
        );
        assert_eq!(doc.files.len(), 2);
        assert_eq!(file_language(&doc.files[0], None), Some("rust"));
        assert_eq!(file_language(&doc.files[1], None), Some("python"));
        // the document-level answer is the first file's, which is what
        // coloured both of them before the loop resolved the grammar per file
        assert_eq!(doc.language(None), Some("rust"));
        // the hint fills in only for a file whose own path names no grammar
        assert_eq!(file_language(&doc.files[1], Some("x.rs")), Some("python"));
    }

    #[test]
    fn fill_runs_group_consecutive_fillers() {
        let lines = [
            Line::fill(),
            Line::fill(),
            Line::state("break"),
            Line::fill(),
        ];
        assert_eq!(fill_runs(&lines), vec![0..2, 3..4]);
        assert!(fill_runs(&[Line::state("all code")]).is_empty());
    }
}
