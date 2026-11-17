//! A row-aligned two-column code view: each row carries an optional line on
//! each side, with the change wash and intra-line emphasis a diff uses. The
//! merge editor's Current/Incoming panes are the first user; its pick boxes
//! are opt-in through [`AlignedOptions::lead`], so a plain aligned view draws
//! no leading column and the kit's other renderers are untouched.

use crate::document::paired_word_emphasis;
use gpui::prelude::*;
use gpui::{AnyElement, Div, Hsla, SharedString, div, px};
use std::ops::Range;
use warpui_kit_core::{fonts, palette};

/// How many pixels one gutter digit takes at the renderer's mono face.
const DIGIT_W: f32 = 7.6;
/// The leading cell's width when the host draws one.
const LEAD_W: f32 = 22.0;
/// The row height relative to the detail font: one mono line plus its gap.
const ROW_SCALE: f32 = 1.6;

/// Which column a leading cell belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlignedSide {
    Left,
    Right,
}

/// One row of an aligned view. A side with no line on that row leaves `None`;
/// the renderer draws the blank side.
pub struct AlignedRow<'a> {
    pub left: Option<(u32, &'a str)>,
    pub right: Option<(u32, &'a str)>,
    /// The row is part of a change: washed on both sides and word-highlighted.
    pub changed: bool,
}

/// How to draw an aligned view.
pub struct AlignedOptions<'a> {
    /// Draw the caller's leading cell per side, before the gutter — the merge
    /// editor puts its pick box here. `None` draws no leading column at all.
    pub lead: Option<&'a mut dyn FnMut(AlignedSide, Option<u32>) -> Option<AnyElement>>,
    /// The detail font size the body renders at.
    pub font_detail: f32,
}

/// The height one row takes at `font_detail`; hosts that scroll the body
/// themselves use it to line the viewport up with a row.
pub fn row_height(font_detail: f32) -> gpui::Pixels {
    px((font_detail * ROW_SCALE).max(16.))
}

/// Draw the rows as two aligned columns under one body. The caller owns the
/// scroller and the pane heads; this element is in flow and sizes to its rows.
pub fn render_aligned(rows: &[AlignedRow<'_>], options: AlignedOptions<'_>) -> AnyElement {
    let AlignedOptions { lead, font_detail } = options;
    let mut lead = lead;
    let row_h = row_height(font_detail);
    let gutter_w = px(gutter_width(rows) as f32 * DIGIT_W + 14.);
    let mut body = div()
        .debug_selector(|| "aligned-panes".into())
        .w_full()
        .flex()
        .flex_col()
        .font_family(fonts::code_font())
        .text_size(px(font_detail));
    for row in rows {
        let (left_emphasis, right_emphasis) = if row.changed {
            paired_word_emphasis(
                row.left.map(|(_, text)| text).unwrap_or(""),
                row.right.map(|(_, text)| text).unwrap_or(""),
                true,
            )
        } else {
            (Vec::new(), Vec::new())
        };
        let left_wash = row.changed.then_some(palette::DIFF_DEL_BG);
        let right_wash = row.changed.then_some(palette::DIFF_ADD_BG);
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
        body = body.child(
            div()
                .w_full()
                .flex()
                .flex_row()
                .items_stretch()
                .h(row_h)
                .child(column(
                    row.left,
                    gutter_w,
                    left_wash,
                    left_emphasis_bg,
                    &left_emphasis,
                    lead.is_some(),
                    lead_cell_left,
                ))
                .child(div().flex_shrink_0().w(px(1.)).bg(palette::hairline()))
                .child(column(
                    row.right,
                    gutter_w,
                    right_wash,
                    right_emphasis_bg,
                    &right_emphasis,
                    lead.is_some(),
                    lead_cell_right,
                )),
        );
    }
    body.into_any_element()
}

fn gutter_width(rows: &[AlignedRow<'_>]) -> usize {
    let top = rows
        .iter()
        .flat_map(|row| {
            [
                row.left.map(|(line, _)| line),
                row.right.map(|(line, _)| line),
            ]
        })
        .flatten()
        .max()
        .unwrap_or(0);
    top.to_string().len().max(2)
}

/// One side of a row: the optional leading cell, the number, then the code.
fn column(
    line: Option<(u32, &str)>,
    gutter_w: gpui::Pixels,
    wash: Option<Hsla>,
    emphasis_bg: Option<Hsla>,
    emphasis: &[Range<usize>],
    has_lead: bool,
    lead: Option<AnyElement>,
) -> Div {
    let mut cell = div()
        .flex_1()
        .min_w_0()
        .h_full()
        .flex()
        .flex_row()
        .items_stretch()
        .overflow_hidden()
        .when_some(wash, |cell, wash| cell.bg(wash));
    if has_lead {
        cell = cell.child(
            div()
                .w(px(LEAD_W))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .children(lead.or_else(|| Some(div().into_any_element()))),
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
                line.map(|(number, _)| SharedString::from(number.to_string()))
                    .unwrap_or_default(),
            ),
    );
    let text = line.map(|(_, text)| text).unwrap_or("");
    cell.child(code(text, emphasis, emphasis_bg))
}

/// The code cell, with the changed words carried on their own spans so the
/// emphasis band sits behind the text without a second text layout.
fn code(text: &str, emphasis: &[Range<usize>], bg: Option<Hsla>) -> AnyElement {
    let mut cell = div()
        .flex_1()
        .min_w_0()
        .px_2()
        .overflow_hidden()
        .whitespace_nowrap()
        .text_color(Hsla::from(palette::CODE_FG));
    let Some(bg) = bg.filter(|_| !emphasis.is_empty()) else {
        return cell.child(text.to_string()).into_any_element();
    };
    let mut at = 0;
    let mut parts: Vec<AnyElement> = Vec::new();
    for range in emphasis {
        if range.start > at {
            parts.push(
                div()
                    .child(text[at..range.start].to_string())
                    .into_any_element(),
            );
        }
        if range.start < range.end {
            parts.push(
                div()
                    .bg(bg)
                    .child(text[range.clone()].to_string())
                    .into_any_element(),
            );
        }
        at = range.end.max(at);
    }
    if at < text.len() {
        parts.push(div().child(text[at..].to_string()).into_any_element());
    }
    cell = cell.children(parts);
    cell.into_any_element()
}
