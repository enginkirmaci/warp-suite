//! A row-aligned two-column code view: each row carries an optional line on
//! each side, with the change wash and intra-line emphasis a diff uses. The
//! merge editor's Current/Incoming panes are the first user; its per-screen
//! extras are opt-in on [`AlignedOptions`] — pick boxes through `lead`, the
//! ancestor column through `base`, token colors through `syntax`, word
//! emphasis off through `emphasis` — so a plain aligned view and the kit's
//! other renderers are untouched.

use crate::document::{RowKind, paired_word_emphasis};
use gpui::prelude::*;
use gpui::{
    AnyElement, App, Div, FontWeight, HighlightStyle, Hsla, SharedString, StyledText, TextStyle,
    Window, div, px,
};
use std::ops::Range;
use std::sync::Arc;
use warpui_kit_core::{fonts, palette};

/// How many pixels one gutter digit takes at the renderer's mono face.
const DIGIT_W: f32 = 7.6;
/// The leading cell's width when the host draws one.
const LEAD_W: f32 = 22.0;
/// The row height relative to the detail font: one mono line plus its gap.
const ROW_SCALE: f32 = 1.6;
/// The pick box's square, inside the leading cell.
const PICK_BOX: f32 = 14.0;
/// The strip above the columns: one label row per column.
const HEAD_H: f32 = 28.0;
/// Rows drawn above and below the viewport's own range.
const WINDOW_OVERSCAN: usize = 8;
/// The window a first frame draws before the viewport has a height.
const WINDOW_FALLBACK_ROWS: usize = 200;

/// Which column a leading cell belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlignedSide {
    Left,
    Right,
}

impl AlignedSide {
    /// The wash a line that came from this side carries: the merge editor
    /// composes its Result from picked lines and bands each with the side's
    /// own color, so the panes and the result read as one.
    pub fn wash(self) -> Hsla {
        match self {
            AlignedSide::Left => palette::DIFF_DEL_BG,
            AlignedSide::Right => palette::DIFF_ADD_BG,
        }
    }
}

/// One row of an aligned view. A side with no line on that row leaves `None`;
/// the renderer draws the blank side.
pub struct AlignedRow<'a> {
    pub left: Option<(u32, &'a str)>,
    pub right: Option<(u32, &'a str)>,
    /// The common-ancestor line, drawn as a middle column when `base` is on.
    pub base: Option<(u32, &'a str)>,
    /// The row is part of a change: washed on both sides and word-highlighted.
    pub changed: bool,
}

/// How to draw an aligned view. Everything past `font_detail` is an opt-in
/// the plain view and the kit's other renderers never turn on.
pub struct AlignedOptions<'a> {
    /// Draw the caller's leading cell per side, before the gutter — the merge
    /// editor puts its pick box here. `None` draws no leading column at all.
    pub lead: Option<&'a mut dyn FnMut(AlignedSide, Option<u32>) -> Option<AnyElement>>,
    /// The detail font size the body renders at.
    pub font_detail: f32,
    /// Rows the caller left out above and below the slice it passed, so the
    /// body keeps the full height while only the window is drawn.
    pub padding: (usize, usize),
    /// Token colors inside the code, from a language key
    /// [`crate::lang_from_path`] returns. `None` keeps the plain mono
    /// foreground.
    pub syntax: Option<&'static str>,
    /// Draw the middle ancestor column from each row's `base` line.
    pub base: bool,
    /// Word-highlight the differing words of a changed row. On by default.
    pub emphasis: bool,
}

impl<'a> AlignedOptions<'a> {
    /// The plain aligned view: no leading column, no ancestor column, no
    /// token colors, word emphasis on.
    pub fn new(font_detail: f32) -> Self {
        Self {
            lead: None,
            font_detail,
            padding: (0, 0),
            syntax: None,
            base: false,
            emphasis: true,
        }
    }
}

/// The height one row takes at `font_detail`; hosts that scroll the body
/// themselves use it to line the viewport up with a row.
pub fn row_height(font_detail: f32) -> gpui::Pixels {
    px((font_detail * ROW_SCALE).max(16.))
}

/// The row slice a scrolled viewport shows, with overscan either side so a
/// wheel step never exposes a blank band while `padding` keeps the body's
/// height honest. A viewport with no height yet draws the fallback window.
pub fn visible_window(total: usize, offset: f32, viewport: f32, row_h: f32) -> (usize, usize) {
    if total == 0 {
        return (0, 0);
    }
    let row_h = row_h.max(1.);
    let offset = offset.max(0.);
    let first = (offset / row_h).floor() as usize;
    let visible = if viewport > 0. {
        (viewport / row_h).ceil() as usize + 1
    } else {
        WINDOW_FALLBACK_ROWS
    };
    let start = first.saturating_sub(WINDOW_OVERSCAN);
    let end = first
        .saturating_add(visible)
        .saturating_add(WINDOW_OVERSCAN)
        .min(total);
    (start, end)
}

/// The strip above the aligned columns: one `label · n lines` cell per column,
/// in draw order, with the leading cell's width reserved when the body draws
/// one so the heads line up with the gutters.
pub fn aligned_pane_head(entries: &[(&str, usize)], lead: bool) -> AnyElement {
    let mut head = div()
        .debug_selector(|| "aligned-head".into())
        .flex()
        .flex_row()
        .flex_none()
        .h(px(HEAD_H))
        .items_center()
        .border_b_1()
        .border_color(palette::hairline());
    for (index, (label, lines)) in entries.iter().enumerate() {
        if index > 0 {
            head = head.child(div().w(px(1.)).h_full().bg(palette::hairline()));
        }
        if lead {
            head = head.child(div().w(px(LEAD_W)).flex_none());
        }
        head = head.child(
            div()
                .flex()
                .items_center()
                .flex_1()
                .min_w_0()
                .px_2()
                .h_full()
                .text_size(px(fonts::font_micro()))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(palette::text_muted())
                .child(format!("{label} · {lines} lines")),
        );
    }
    head.into_any_element()
}

/// The pick box visual: the accent fills the frame when the line is taken.
/// The caller owns the click; this is only the box.
pub fn aligned_pick_box(checked: bool) -> AnyElement {
    let frame = div()
        .debug_selector(|| "aligned-pick-box".into())
        .w(px(PICK_BOX))
        .h(px(PICK_BOX))
        .rounded(px(3.))
        .border_1()
        .border_color(if checked {
            palette::accent()
        } else {
            palette::text_muted()
        })
        .when(checked, |frame| frame.bg(palette::accent()))
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(10.));
    if checked {
        frame
            .text_color(palette::on_accent())
            .child(StyledText::new("✓"))
            .into_any_element()
    } else {
        frame.into_any_element()
    }
}

/// Draw the rows as aligned columns under one body. The caller owns the
/// scroller and the pane heads; this element is in flow and sizes to its rows.
pub fn render_aligned(
    rows: &[AlignedRow<'_>],
    options: AlignedOptions<'_>,
    window: &mut Window,
    cx: &App,
) -> AnyElement {
    let AlignedOptions {
        lead,
        font_detail,
        padding,
        syntax,
        base,
        emphasis,
    } = options;
    let mut lead = lead;
    let row_h = row_height(font_detail);
    let gutter_w = px(gutter_width(rows, base) as f32 * DIGIT_W + 14.);
    let code_style = crate::code_style(window, font_detail, palette::CODE_FG);
    // One highlighting pass per column, over the window the caller passed:
    // multi-line constructs resolve inside it and the cache skips repeats.
    let syntax = syntax.map(|lang| {
        let theme = gpui_kit::component::Theme::global(cx)
            .highlight_theme
            .clone();
        (
            side_tokens(rows, |row| row.left, lang, &theme),
            side_tokens(rows, |row| row.base, lang, &theme),
            side_tokens(rows, |row| row.right, lang, &theme),
        )
    });
    let (mut left_at, mut base_at, mut right_at) = (0usize, 0usize, 0usize);
    let mut body = div()
        .debug_selector(|| "aligned-panes".into())
        .w_full()
        .flex()
        .flex_col()
        .font_family(fonts::code_font())
        .text_size(px(font_detail));
    if padding.0 > 0 {
        body = body.child(div().h(row_h * padding.0 as f32).flex_none());
    }
    for row in rows {
        let (left_emphasis, right_emphasis) = if row.changed {
            paired_word_emphasis(
                row.left.map(|(_, text)| text).unwrap_or(""),
                row.right.map(|(_, text)| text).unwrap_or(""),
                emphasis,
            )
        } else {
            (Vec::new(), Vec::new())
        };
        let left_wash = row.changed.then_some(palette::DIFF_DEL_BG);
        let right_wash = row.changed.then_some(palette::DIFF_ADD_BG);
        let base_wash = row.changed.then_some(palette::DIFF_HUNK_BG);
        let left_emphasis_bg = row.changed.then_some(palette::DIFF_DEL_EMPHASIS);
        let right_emphasis_bg = row.changed.then_some(palette::DIFF_ADD_EMPHASIS);
        let lead_cell_left = match (lead.as_deref_mut(), row.left) {
            (Some(draw), Some((line, _))) => draw(AlignedSide::Left, Some(line)),
            _ => None,
        };
        let lead_cell_right = match (lead.as_deref_mut(), row.right) {
            (Some(draw), Some((line, _))) => draw(AlignedSide::Right, Some(line)),
            _ => None,
        };
        let base_line = base.then_some(row.base).flatten();
        let left_runs = side_runs(
            syntax.as_ref().map(|(tokens, _, _)| tokens),
            &mut left_at,
            row.left,
            &left_emphasis,
            left_emphasis_bg,
        );
        let base_runs = side_runs(
            syntax.as_ref().map(|(_, tokens, _)| tokens),
            &mut base_at,
            base_line,
            &[],
            None,
        );
        let right_runs = side_runs(
            syntax.as_ref().map(|(_, _, tokens)| tokens),
            &mut right_at,
            row.right,
            &right_emphasis,
            right_emphasis_bg,
        );
        let mut row_div = div().w_full().flex().flex_row().items_stretch().h(row_h);
        row_div = row_div.child(column(
            Side {
                line: row.left,
                wash: left_wash,
                runs: &left_runs,
                lead: lead_cell_left,
            },
            gutter_w,
            lead.is_some(),
            "aligned-left",
            &code_style,
        ));
        if base {
            row_div = row_div.child(divider()).child(column(
                Side {
                    line: base_line,
                    wash: base_wash,
                    runs: &base_runs,
                    lead: None,
                },
                gutter_w,
                false,
                "aligned-base",
                &code_style,
            ));
        }
        row_div = row_div.child(divider()).child(column(
            Side {
                line: row.right,
                wash: right_wash,
                runs: &right_runs,
                lead: lead_cell_right,
            },
            gutter_w,
            lead.is_some(),
            "aligned-right",
            &code_style,
        ));
        body = body.child(row_div);
    }
    if padding.1 > 0 {
        body = body.child(div().h(row_h * padding.1 as f32).flex_none());
    }
    body.into_any_element()
}

fn divider() -> Div {
    div().flex_shrink_0().w(px(1.)).bg(palette::hairline())
}

fn gutter_width(rows: &[AlignedRow<'_>], base: bool) -> usize {
    let mut top = 0u32;
    for row in rows {
        for line in [row.left, base.then_some(row.base).flatten(), row.right]
            .into_iter()
            .flatten()
        {
            top = top.max(line.0);
        }
    }
    top.to_string().len().max(2)
}

/// Token runs for one column's lines, the shape [`crate::line_tokens`]
/// returns: per line, the byte ranges the theme styles.
type ColumnTokens = Vec<Vec<(Range<usize>, HighlightStyle)>>;

/// Token runs for one column's texts: one buffer pass per column so
/// multi-line constructs resolve, sliced back per line. An empty pass when
/// the column holds no lines.
fn side_tokens<'a>(
    rows: &[AlignedRow<'a>],
    side: fn(&AlignedRow<'a>) -> Option<(u32, &'a str)>,
    lang: &str,
    theme: &Arc<gpui_kit::component::highlighter::HighlightTheme>,
) -> ColumnTokens {
    let texts: Vec<&str> = rows.iter().filter_map(side).map(|(_, text)| text).collect();
    let kinds = vec![RowKind::Context; texts.len()];
    crate::line_tokens(&texts, &kinds, lang, theme)
}

/// The code runs for one side of a row: its token slice when syntax is on,
/// with the emphasis band layered over them. `at` walks the column's token
/// pass in row order; a row without a line on this side consumes none.
fn side_runs(
    tokens: Option<&ColumnTokens>,
    at: &mut usize,
    line: Option<(u32, &str)>,
    emphasis: &[Range<usize>],
    emphasis_bg: Option<Hsla>,
) -> Vec<(Range<usize>, HighlightStyle)> {
    let Some((_, text)) = line else {
        return Vec::new();
    };
    let empty = Vec::new();
    let tokens = match tokens {
        Some(all) => {
            let slice = all.get(*at).unwrap_or(&empty);
            *at += 1;
            slice.as_slice()
        }
        None => &[],
    };
    crate::merged_runs(text.len(), tokens, emphasis, emphasis_bg)
}

/// One side of a row: the optional leading cell, the number, then the code.
struct Side<'a> {
    line: Option<(u32, &'a str)>,
    wash: Option<Hsla>,
    runs: &'a [(Range<usize>, HighlightStyle)],
    lead: Option<AnyElement>,
}

fn column(
    side: Side<'_>,
    gutter_w: gpui::Pixels,
    has_lead: bool,
    name: &'static str,
    style: &TextStyle,
) -> Div {
    let mut cell = div()
        .debug_selector(move || name.into())
        .flex_1()
        .min_w_0()
        .h_full()
        .flex()
        .flex_row()
        .items_stretch()
        .overflow_hidden()
        .when_some(side.wash, |cell, wash| cell.bg(wash));
    if has_lead {
        cell = cell.child(
            div()
                .debug_selector(|| "aligned-lead".into())
                .w(px(LEAD_W))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .children(side.lead.or_else(|| Some(div().into_any_element()))),
        );
    }
    cell = cell.child(
        div()
            .w(gutter_w)
            .flex_none()
            .flex()
            .justify_end()
            .pr_2()
            .items_center()
            .text_color(palette::DIFF_GUTTER)
            .child(
                side.line
                    .map(|(number, _)| SharedString::from(number.to_string()))
                    .unwrap_or_default(),
            ),
    );
    let text = side.line.map(|(_, text)| text).unwrap_or("");
    cell.child(code(text, side.runs, style))
}

/// The code cell: one laid-out run carries token colors and the emphasis
/// band together, so the text keeps its own inline layout.
fn code(text: &str, runs: &[(Range<usize>, HighlightStyle)], style: &TextStyle) -> Div {
    div()
        .flex_1()
        .min_w_0()
        .px_2()
        .overflow_hidden()
        .whitespace_nowrap()
        .flex()
        .flex_row()
        .items_center()
        .child(crate::styled_text(text, style, runs))
}
