//! One frame's geometry: every line placed in document coordinates, plus the
//! questions the editor asks of it (what is under this point, where is this
//! offset, which rectangles does this selection cover). Coordinates are
//! relative to the text column's left edge and the top of the document.

use super::display::{
    Bias, Decor, LineRole, LineView, TableRowView, TextBox, property_key_x, property_text_line,
    property_value_x, table_cell_pad_x,
};
use super::images::fit_width;
use super::layout::{Fonts, Row, ShapedBox, shape_box};
use super::theme::LiveColors;
use crate::live::{InlineObject, LiveDocument, LiveLineKind, TableAlign};
use gpui::Window;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::rc::Rc;

/// The readable line length, Obsidian's 700px at the default 16px.
const MAX_COLUMN_EM: f32 = 43.75;
/// Side margins of the text column.
const MARGIN_X_EM: f32 = 2.;
/// Space above the first line.
const MARGIN_TOP_EM: f32 = 2.;
/// Past the last line the view keeps scrolling by this share of its height,
/// so the end of a note can sit in the middle of the screen.
const TAIL_SHARE: f32 = 0.5;
/// Height of a table row's text lines, in em.
const TABLE_ROW_LINE_EM: f32 = 1.3;
/// Vertical cell padding, in em.
const TABLE_ROW_PAD_EM: f32 = 0.25;
/// The depth of a table's add strips, in em.
const TABLE_STRIP_EM: f32 = 1.;
/// A rendered diagram's edit button, in em.
const DIAGRAM_BUTTON_W_EM: f32 = 1.75;
const DIAGRAM_BUTTON_H_EM: f32 = 1.5;
const DIAGRAM_BUTTON_INSET_EM: f32 = 0.25;
/// Between a property's key and its value column, in em.
const PROPERTY_KEY_GAP_EM: f32 = 0.5;
/// The Add property button past its text, in em.
const ADD_BUTTON_PAD_EM: f32 = 0.375;

/// A shaped box placed in its line.
pub struct LaidBox {
    pub shaped: Rc<ShapedBox>,
    /// What was shaped, with this frame's source offsets.
    pub text: Rc<TextBox>,
    pub x: f32,
    /// From the line's top.
    pub y: f32,
    pub source: Range<usize>,
    /// The x span clicks in this box's column resolve to.
    pub column: Range<f32>,
}

impl LaidBox {
    /// Doc-space top of `row`.
    fn row_top(&self, line_top: f32, row: &Row) -> f32 {
        line_top + self.y + row.top
    }
}

/// An image placed in its line.
pub struct LaidImage {
    /// Its key in the image store.
    pub key: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// One table cell's frame.
pub struct LaidCell {
    pub x: f32,
    pub width: f32,
}

/// A table's outline, in document coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LaidTable {
    pub block: usize,
    pub rect: Rect,
}

/// Which of a table's add strips.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableStrip {
    /// Under the table.
    Row,
    /// On its right.
    Column,
}

/// What the pointer rests on, for what shows only under it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Hover {
    /// A table's block, and the add strip when on one.
    pub table: Option<(usize, Option<TableStrip>)>,
    /// A rendered diagram's block, and whether on its edit button.
    pub diagram: Option<(usize, bool)>,
}

/// One placed source line.
pub struct LaidLine {
    pub source: Range<usize>,
    pub top: f32,
    pub height: f32,
    pub view: Rc<LineView>,
    pub boxes: Vec<LaidBox>,
    pub image: Option<LaidImage>,
    /// Column frames of a table row.
    pub cells: Vec<LaidCell>,
}

/// One frame of layout.
pub struct Frame {
    /// The text column's left edge inside the element.
    pub column_x: f32,
    pub column_width: f32,
    pub lines: Vec<LaidLine>,
    /// Bottom of the last line.
    pub height: f32,
    /// The furthest the view scrolls.
    pub max_scroll: f32,
    pub tables: Vec<LaidTable>,
    pub em: f32,
}

/// What is under a point.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HitKind {
    Text,
    /// A task box, by its `[ ]` source range.
    Checkbox(Range<usize>),
    /// A rendered diagram, by block: inert.
    Diagram(usize),
    /// A rendered diagram's edit button, by block.
    EditDiagram(usize),
    /// A table's add strip, by block.
    TableStrip(usize, TableStrip),
    /// Something a click does nothing on.
    Inert,
    /// The Add property button.
    AddProperty,
    /// Past a list property's items, by the property's line.
    AddItem(usize),
    /// An item's remove cross, by an offset in the item.
    RemoveItem(usize),
}

/// The result of a hit test.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hit {
    pub offset: usize,
    pub kind: HitKind,
    /// The display offset under the point lies past the row's last glyph.
    pub past_end: bool,
}

/// Where the caret goes.
#[derive(Clone, Debug, PartialEq)]
pub struct CaretSpot {
    pub x: f32,
    pub y: f32,
    pub height: f32,
    /// How far the caret's row reaches up and down: past the line's padding
    /// on its first and last rows, and past an image taller than the caret.
    /// A step just beyond these lands on the next row.
    pub reach: Range<f32>,
}

/// A doc-space rectangle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    fn contains(&self, x: f32, y: f32) -> bool {
        (self.x..self.x + self.width).contains(&x) && (self.y..self.y + self.height).contains(&y)
    }
}

/// Shaped boxes kept from the last frame: a keystroke reshapes the lines it
/// changed, not the note. Keys leave out source offsets, so text pushed down
/// by an edit above it still hits.
#[derive(Default)]
pub struct ShapeCache {
    current: HashMap<u64, Rc<ShapedBox>>,
    previous: HashMap<u64, Rc<ShapedBox>>,
}

impl ShapeCache {
    fn shape(
        &mut self,
        view: &TextBox,
        wrap: f32,
        line_height: f32,
        style: &Style<'_>,
        window: &mut Window,
    ) -> Rc<ShapedBox> {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        view.frags.hash(&mut hasher);
        (view.hang_after, view.size, view.mono).hash(&mut hasher);
        wrap.to_bits().hash(&mut hasher);
        line_height.to_bits().hash(&mut hasher);
        let key = hasher.finish();
        if let Some(hit) = self.current.get(&key) {
            return hit.clone();
        }
        let shaped = self.previous.remove(&key).unwrap_or_else(|| {
            Rc::new(shape_box(
                view,
                wrap,
                line_height,
                style.fonts,
                style.colors,
                window,
            ))
        });
        self.current.insert(key, shaped.clone());
        shaped
    }

    fn end_frame(&mut self) {
        self.previous = std::mem::take(&mut self.current);
    }

    /// Forgets everything: the colors or fonts it was shaped with changed.
    pub fn clear(&mut self) {
        self.current.clear();
        self.previous.clear();
    }
}

/// The fixed inputs of one layout.
pub struct Style<'a> {
    pub em: f32,
    pub fonts: &'a Fonts,
    pub colors: &'a LiveColors,
}

/// Lays out `views` for an element `width` wide and `viewport` tall.
pub fn build(
    doc: &LiveDocument,
    views: &[Rc<LineView>],
    width: f32,
    viewport: f32,
    style: &Style<'_>,
    cache: &mut ShapeCache,
    window: &mut Window,
) -> Frame {
    let em = style.em;
    let column_width = (width - MARGIN_X_EM * em * 2.)
        .min(MAX_COLUMN_EM * em)
        .max(em * 4.);
    let column_x = ((width - column_width) / 2.).max(0.);
    let mut lines = Vec::with_capacity(views.len());
    let mut y = MARGIN_TOP_EM * em;
    let mut columns: HashMap<usize, Vec<f32>> = HashMap::new();
    let mut tables: Vec<LaidTable> = Vec::new();

    for (ix, view) in views.iter().enumerate() {
        let source = doc.lines[ix].range.clone();
        if view.hidden {
            lines.push(LaidLine {
                source,
                top: y,
                height: 0.,
                view: view.clone(),
                boxes: Vec::new(),
                image: None,
                cells: Vec::new(),
            });
            continue;
        }
        let top_pad = view.height.top.0;
        let line_height = view.height.line.0;
        let mut boxes = Vec::new();
        let mut cells = Vec::new();
        let mut image = None;
        let body_height;

        if let Some(row) = &view.table {
            let widths = columns.entry(row.block).or_insert_with(|| {
                table_columns(views, row.block, column_width, style, cache, window)
            });
            let (placed, frames, row_height) =
                table_row(row, widths, top_pad, style, cache, window);
            boxes = placed;
            cells = frames;
            body_height = row_height;
        } else if let Some(property) = &view.property {
            let key_x = property_key_x(em);
            let value_x = property_value_x(em);
            let row = property_text_line(em);
            let key_width = value_x - key_x - em * PROPERTY_KEY_GAP_EM;
            let key = cache.shape(&property.key, key_width, row, style, window);
            let value_width = column_width - value_x;
            let value = cache.shape(&property.value, value_width, row, style, window);
            let center = |shaped: &ShapedBox| top_pad + (line_height - shaped.height).max(0.) / 2.;
            body_height = line_height.max(value.height + line_height - row);
            boxes.push(LaidBox {
                y: center(&key),
                x: key_x,
                source: property.key_source.clone(),
                column: 0.0..value_x,
                shaped: key,
                text: property.key.clone(),
            });
            boxes.push(LaidBox {
                y: center(&value),
                x: value_x,
                source: property.value_source.clone(),
                column: value_x..f32::MAX,
                shaped: value,
                text: property.value.clone(),
            });
        } else {
            let wrap = column_width - view.x.0 - view.right.0;
            let shaped = cache.shape(&view.text, wrap, line_height, style, window);
            let has_text = !view.text.frags.is_empty();
            // an image alone on its line takes the line's place
            let text_height = if view.image.is_some() && !has_text {
                0.
            } else {
                shaped.height
            };
            image = view.image.as_ref().map(|request| {
                let (width, height) = fit_width(request.width.0, request.height.0, column_width);
                LaidImage {
                    key: request.key.clone(),
                    x: 0.,
                    y: top_pad + text_height,
                    width,
                    height,
                }
            });
            body_height = text_height + image.as_ref().map_or(0., |i| i.height);
            boxes.push(LaidBox {
                x: view.x.0,
                y: top_pad,
                source: source.clone(),
                column: 0.0..f32::MAX,
                shaped,
                text: view.text.clone(),
            });
        }

        if let (Some(row), Some(last)) = (&view.table, cells.last()) {
            let bottom = y + top_pad + body_height;
            match tables.last_mut().filter(|table| table.block == row.block) {
                Some(table) => table.rect.height = bottom - table.rect.y,
                None => tables.push(LaidTable {
                    block: row.block,
                    rect: Rect {
                        x: 0.,
                        y: y + top_pad,
                        width: last.x + last.width + 1.,
                        height: body_height,
                    },
                }),
            }
        }
        let height = top_pad + body_height + view.height.bottom.0;
        lines.push(LaidLine {
            source,
            top: y,
            height,
            view: view.clone(),
            boxes,
            image,
            cells,
        });
        y += height;
    }
    cache.end_frame();

    let tail = viewport * TAIL_SHARE;
    Frame {
        column_x,
        column_width,
        lines,
        height: y,
        max_scroll: (y + tail - viewport).max(0.),
        tables,
        em,
    }
}

/// Column widths of table `block`: every cell's text plus padding, scaled
/// down together when the table is wider than the column.
fn table_columns(
    views: &[Rc<LineView>],
    block: usize,
    column_width: f32,
    style: &Style<'_>,
    cache: &mut ShapeCache,
    window: &mut Window,
) -> Vec<f32> {
    let pad = table_cell_pad_x(style.em) * 2.;
    let row_line = style.em * TABLE_ROW_LINE_EM;
    let mut widths: Vec<f32> = Vec::new();
    for view in views {
        let Some(row) = view.table.as_ref().filter(|row| row.block == block) else {
            continue;
        };
        if widths.len() < row.cells.len() {
            widths.resize(row.cells.len(), 0.);
        }
        for (ix, cell) in row.cells.iter().enumerate() {
            let natural = cache
                .shape(&cell.text, f32::MAX, row_line, style, window)
                .natural_width;
            widths[ix] = widths[ix].max(natural.ceil());
        }
    }
    let padding = pad * widths.len() as f32;
    let total: f32 = widths.iter().sum();
    let room = column_width - padding - widths.len() as f32 - 1.;
    if total > room && total > 0. {
        let scale = (room / total).max(0.1);
        for width in &mut widths {
            *width = (*width * scale).floor();
        }
    }
    widths.iter().map(|width| width + pad).collect()
}

fn table_row(
    row: &TableRowView,
    widths: &[f32],
    top_pad: f32,
    style: &Style<'_>,
    cache: &mut ShapeCache,
    window: &mut Window,
) -> (Vec<LaidBox>, Vec<LaidCell>, f32) {
    let em = style.em;
    let pad_x = table_cell_pad_x(em);
    let pad_y = em * TABLE_ROW_PAD_EM;
    let row_line = em * TABLE_ROW_LINE_EM;
    let mut x = 0.5;
    let mut boxes = Vec::with_capacity(row.cells.len());
    let mut frames = Vec::with_capacity(row.cells.len());
    let mut tallest = row_line;
    for (ix, cell) in row.cells.iter().enumerate() {
        let width = widths.get(ix).copied().unwrap_or(pad_x * 2.);
        let inner = width - pad_x * 2.;
        let shaped = cache.shape(&cell.text, inner.max(em), row_line, style, window);
        tallest = tallest.max(shaped.height);
        let free = (inner - shaped.width).max(0.);
        let shift = match cell.align {
            TableAlign::Left => 0.,
            TableAlign::Center => free / 2.,
            TableAlign::Right => free,
        };
        frames.push(LaidCell { x, width });
        boxes.push(LaidBox {
            shaped,
            text: cell.text.clone(),
            x: x + pad_x + shift,
            y: top_pad + pad_y,
            source: cell.source.clone(),
            column: x..x + width + 1.,
        });
        x += width + 1.;
    }
    (boxes, frames, tallest + pad_y * 2. + 1.)
}

impl Frame {
    /// An add strip of `table`.
    pub fn table_strip(&self, table: &LaidTable, strip: TableStrip) -> Rect {
        let depth = self.em * TABLE_STRIP_EM;
        let rect = &table.rect;
        match strip {
            TableStrip::Row => Rect {
                y: rect.y + rect.height,
                height: depth,
                ..*rect
            },
            TableStrip::Column => Rect {
                x: rect.x + rect.width,
                width: depth,
                ..*rect
            },
        }
    }

    /// The table or add strip under a point.
    pub fn table_at(&self, x: f32, y: f32) -> Option<(usize, Option<TableStrip>)> {
        self.tables.iter().find_map(|table| {
            let strip = [TableStrip::Row, TableStrip::Column]
                .into_iter()
                .find(|strip| self.table_strip(table, *strip).contains(x, y));
            (strip.is_some() || table.rect.contains(x, y)).then_some((table.block, strip))
        })
    }

    /// A rendered diagram's area: the whole column, like a block in the flow.
    fn diagram_rect(&self, line: &LaidLine) -> Rect {
        Rect {
            x: 0.,
            y: line.top,
            width: self.column_width,
            height: line.height,
        }
    }

    /// The edit button of the rendered diagram on `line`.
    pub fn diagram_button(&self, line: &LaidLine) -> Rect {
        let width = self.em * DIAGRAM_BUTTON_W_EM;
        let inset = self.em * DIAGRAM_BUTTON_INSET_EM;
        Rect {
            x: self.column_width - width - inset,
            y: line.top + inset,
            width,
            height: self.em * DIAGRAM_BUTTON_H_EM,
        }
    }

    /// The rendered diagram under a point, and whether on its edit button.
    pub fn diagram_at(&self, x: f32, y: f32) -> Option<(usize, bool)> {
        let line = &self.lines[self.line_at(y)];
        let block = line.view.diagram?;
        self.diagram_rect(line)
            .contains(x, y)
            .then(|| (block, self.diagram_button(line).contains(x, y)))
    }

    /// What the pointer rests on at a point.
    pub fn hover_at(&self, x: f32, y: f32) -> Hover {
        Hover {
            table: self.table_at(x, y),
            diagram: self.diagram_at(x, y),
        }
    }

    /// The line at document height `y`.
    pub fn line_at(&self, y: f32) -> usize {
        self.lines
            .partition_point(|line| line.top + line.height <= y)
            .min(self.lines.len().saturating_sub(1))
    }

    /// The first line reaching below `y`.
    pub fn first_visible(&self, y: f32) -> usize {
        self.lines
            .partition_point(|line| line.top + line.height < y)
    }

    fn box_for(&self, line: &LaidLine, offset: usize) -> Option<usize> {
        line.boxes
            .iter()
            .position(|b| b.source.start <= offset && offset <= b.source.end)
            .or_else(|| (!line.boxes.is_empty()).then_some(0))
    }

    /// Where the caret sits for source `offset`.
    pub fn caret(&self, doc: &LiveDocument, offset: usize) -> Option<CaretSpot> {
        let mut index = doc.line_at(offset);
        if let Some(host) = self.lines.get(index)?.view.host {
            index = host;
        }
        // a hidden line has no place of its own; the row under it does
        while self.lines.get(index)?.view.hidden && index + 1 < self.lines.len() {
            index += 1;
        }
        let line = self.lines.get(index)?;
        let laid = &line.boxes[self.box_for(line, offset)?];
        let (row, x) = laid.shaped.locate(&laid.text, offset);
        let shaped_row = &laid.shaped.rows[row];
        let row_top = laid.row_top(line.top, shaped_row);
        let reach_top = if row == 0 { line.top } else { row_top };
        let reach_bottom = if row + 1 == laid.shaped.rows.len() {
            line.top + line.height
        } else {
            row_top + shaped_row.height
        };
        Some(CaretSpot {
            x: laid.x + x,
            y: line.top + laid.y + laid.shaped.text_top(shaped_row),
            height: laid.shaped.line_height,
            reach: reach_top..reach_bottom,
        })
    }

    /// What lies under column-space `x`, document-space `y`.
    pub fn hit(&self, x: f32, y: f32) -> Hit {
        let index = self.line_at(y);
        let line = &self.lines[index];
        let widget = |kind| Hit {
            offset: line.source.start,
            kind,
            past_end: false,
        };
        if let Some((block, Some(strip))) = self.table_at(x, y) {
            return widget(HitKind::TableStrip(block, strip));
        }
        if let Some((block, on_button)) = self.diagram_at(x, y) {
            return widget(if on_button {
                HitKind::EditDiagram(block)
            } else {
                HitKind::Diagram(block)
            });
        }
        match line.view.role {
            LineRole::Text => {}
            LineRole::Inert => return widget(HitKind::Inert),
            LineRole::AddProperty => {
                let reach = line
                    .boxes
                    .first()
                    .map_or(0., |laid| laid.x + laid.shaped.width);
                if x <= reach + self.em * ADD_BUTTON_PAD_EM {
                    return widget(HitKind::AddProperty);
                }
            }
        }
        for decor in &line.view.decor {
            if let Decor::Checkbox {
                x: box_x, source, ..
            } = decor
            {
                let size = line.view.text.size.0;
                let inside_x = x >= box_x.0 && x <= box_x.0 + size;
                let inside_y = y >= line.top && y <= line.top + line.height;
                if inside_x && inside_y {
                    return Hit {
                        offset: source.start,
                        kind: HitKind::Checkbox(source.clone()),
                        past_end: false,
                    };
                }
            }
        }
        let Some(laid) = line
            .boxes
            .iter()
            .find(|b| b.column.contains(&x))
            .or_else(|| line.boxes.last())
        else {
            // a hidden line: its neighbor will do
            return Hit {
                offset: line.source.start,
                kind: HitKind::Text,
                past_end: false,
            };
        };
        let row = laid.shaped.row_at(y - line.top - laid.y);
        let box_x = x - laid.x;
        let shaped_row = &laid.shaped.rows[row];
        let past_end = box_x > shaped_row.x + shaped_row.width;
        let offset = laid.shaped.source_at(&laid.text, row, box_x);
        let kind = match &line.view.property {
            Some(property) if std::ptr::eq(laid, &line.boxes[1]) => {
                match shaped_row.remove_at(box_x) {
                    Some(piece) => {
                        HitKind::RemoveItem(laid.text.source_of(piece.display.end, Bias::Left))
                    }
                    None if past_end && property.list => HitKind::AddItem(index),
                    None => HitKind::Text,
                }
            }
            _ => HitKind::Text,
        };
        Hit {
            offset,
            kind,
            past_end,
        }
    }

    /// The rectangles covering source `range`, on lines reaching into
    /// `visible`.
    pub fn selection_rects(
        &self,
        doc: &LiveDocument,
        range: &Range<usize>,
        visible: Range<f32>,
    ) -> Vec<Rect> {
        let mut rects = Vec::new();
        if range.is_empty() {
            return rects;
        }
        let first = doc.line_at(range.start);
        let first = self.lines[first].view.host.unwrap_or(first);
        let last = doc.line_at(range.end);
        for index in first..=last {
            let line = &self.lines[index];
            if line.top + line.height < visible.start {
                continue;
            }
            if line.top > visible.end {
                break;
            }
            let continues = range.end > line.source.end;
            for laid in &line.boxes {
                let start = range.start.max(laid.source.start);
                let end = range.end.min(laid.source.end);
                if start > end {
                    continue;
                }
                let display = laid.text.display_of(start)..laid.text.display_of(end);
                for (ix, row) in laid.shaped.rows.iter().enumerate() {
                    let from = display.start.max(row.display.start);
                    let to = display.end.min(row.display.end);
                    let is_last_row = ix + 1 == laid.shaped.rows.len();
                    let past_row = display.end > row.display.end;
                    let newline = continues && is_last_row && laid.source.end == line.source.end;
                    if from > to || (from == to && !newline && !past_row) {
                        continue;
                    }
                    let x0 = row.x_at(from);
                    let x1 = row.x_at(to);
                    // the line break itself selects as a sliver past the text
                    let tail = if newline {
                        laid.shaped.line_height / 4.
                    } else {
                        0.
                    };
                    rects.push(Rect {
                        x: laid.x + x0,
                        y: laid.row_top(line.top, row),
                        width: x1 - x0 + tail,
                        height: row.height,
                    });
                }
            }
        }
        rects
    }
}

/// The inline object covering `offset` on a line, if any.
pub fn object_at(doc: &LiveDocument, offset: usize) -> Option<&InlineObject> {
    let line = &doc.lines[doc.line_at(offset)];
    if matches!(line.kind, LiveLineKind::Code | LiveLineKind::Fence) {
        return None;
    }
    line.objects
        .iter()
        .find(|object| object.range.start <= offset && offset <= object.range.end)
}
