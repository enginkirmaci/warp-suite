//! Turns [`LineView`]s into positioned text. One visual row can mix font sizes
//! and carry pill padding, which a wrapped gpui line cannot, so each fragment
//! is shaped unwrapped and this module does the wrapping: a row is a list of
//! pieces cut out of the shaped fragments, all on one baseline. An image
//! flows as one wide glyph, and a row grows to hold the tallest of them.

use super::display::{Bias, Frag, IMAGE_CHAR, Pill, RunStyle, TextBox, pill_padding};
use super::images::fit_width;
use super::theme::LiveColors;
use gpui::{
    Font, FontStyle, FontWeight, ShapedLine, SharedString, StrikethroughStyle, TextRun,
    UnderlineStyle, Window, px,
};
use std::ops::Range;

/// How much bolder than regular a weight of 700 is, as added stem width in em.
const EMBOLDEN_EM: f32 = 0.07;
/// Between an item's text and its remove cross, in em.
const REMOVE_GAP_EM: f32 = 0.2;

/// The two families the editor sets text in.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Fonts {
    pub sans: Font,
    pub mono: Font,
    /// The sans family has no bold face of its own. gpui only picks among a
    /// family's static faces, so a single variable font file renders every
    /// weight as regular; such text gets emboldened at paint time.
    pub sans_lacks_bold: bool,
    pub mono_lacks_bold: bool,
}

impl Fonts {
    /// The two families, probing each for a real bold face.
    pub fn new(sans: Font, mono: Font, window: &Window) -> Self {
        let lacks_bold = |font: &Font| {
            let system = window.text_system();
            let bold = Font {
                weight: FontWeight::BOLD,
                ..font.clone()
            };
            system.resolve_font(font) == system.resolve_font(&bold)
        };
        Self {
            sans_lacks_bold: lacks_bold(&sans),
            mono_lacks_bold: lacks_bold(&mono),
            sans,
            mono,
        }
    }

    /// Stem width to add to `frag` at paint time, 0 when its font carries the
    /// weight itself.
    fn embolden(&self, frag: &Frag) -> f32 {
        let mono = frag.runs.first().is_some_and(|(_, style)| style.mono);
        let lacks = if mono {
            self.mono_lacks_bold
        } else {
            self.sans_lacks_bold
        };
        if lacks && frag.weight > 400 {
            frag.size.0 * EMBOLDEN_EM * (f32::from(frag.weight) - 400.) / 300.
        } else {
            0.
        }
    }

    fn run(&self, len: usize, style: &RunStyle, colors: &LiveColors) -> TextRun {
        let base = if style.mono { &self.mono } else { &self.sans };
        let ink = colors.ink(style.ink);
        TextRun {
            len,
            font: Font {
                weight: FontWeight(f32::from(style.weight)),
                style: if style.italic {
                    FontStyle::Italic
                } else {
                    FontStyle::Normal
                },
                ..base.clone()
            },
            color: ink,
            background_color: style.background.map(|bg| colors.ink(bg)),
            underline: style.underline.map(|color| UnderlineStyle {
                thickness: px(1.),
                color: Some(colors.ink(color)),
                wavy: false,
            }),
            strikethrough: style.strike.then_some(StrikethroughStyle {
                thickness: px(1.),
                color: Some(ink),
            }),
        }
    }
}

/// An image in a row: `key` in the image store, at its fitted size. The
/// piece's width is the image's.
pub struct PieceImage {
    pub key: String,
    pub height: f32,
}

/// The part of a fragment that lands on one row.
pub struct Piece {
    /// Flat display range.
    pub display: Range<usize>,
    pub line: ShapedLine,
    /// Pen position of the first glyph, from the row's left edge.
    pub x: f32,
    pub width: f32,
    pub size: f32,
    pub raise: f32,
    pub pill: Option<Pill>,
    pub pad_left: f32,
    pub pad_right: f32,
    /// The fragment's outer edges fall on this piece.
    pub opens: bool,
    pub closes: bool,
    pub link_icon: bool,
    /// Stem width added at paint time.
    pub embolden: f32,
    /// Paint this instead of the glyphs.
    pub image: Option<PieceImage>,
    /// A remove cross fills `pad_right`.
    pub remove: bool,
}

impl Piece {
    /// The pen position of byte `local`, from the piece's first glyph.
    fn x_of(&self, local: usize) -> f32 {
        match self.image {
            Some(_) if local > 0 => self.width,
            Some(_) => 0.,
            None => self.line.x_for_index(local).as_f32(),
        }
    }

    /// The byte closest to `x`, from the piece's first glyph.
    fn index_of(&self, x: f32) -> usize {
        match self.image {
            Some(_) if x * 2. < self.width => 0,
            Some(_) => self.display.len(),
            None => self.line.closest_index_for_x(px(x)),
        }
    }
}

/// One visual row of a text box.
pub struct Row {
    pub display: Range<usize>,
    /// Left edge from the box's left edge: the hanging indent, if any.
    pub x: f32,
    pub width: f32,
    pub pieces: Vec<Piece>,
    /// From the box's top.
    pub top: f32,
    /// One line, or the tallest image on the row.
    pub height: f32,
}

impl Row {
    /// The pen x of a display offset, from the box's left edge.
    pub fn x_at(&self, display: usize) -> f32 {
        for piece in &self.pieces {
            if display <= piece.display.end {
                let local = display.saturating_sub(piece.display.start);
                return self.x + piece.x + piece.x_of(local);
            }
        }
        self.x + self.width
    }

    /// The piece whose remove cross is at `x`, from the box's left edge.
    pub fn remove_at(&self, x: f32) -> Option<&Piece> {
        self.pieces.iter().find(|piece| {
            let start = self.x + piece.x + piece.width;
            piece.remove && (start..start + piece.pad_right).contains(&x)
        })
    }

    /// The display offset closest to `x`, measured from the box's left edge.
    pub fn display_at(&self, x: f32) -> usize {
        let x = x - self.x;
        let mut last = self.display.start;
        for piece in &self.pieces {
            last = piece.display.end;
            if x < piece.x + piece.width + piece.pad_right {
                return piece.display.start + piece.index_of(x - piece.x);
            }
        }
        last
    }
}

/// A text box laid out for one wrap width. It knows display offsets only, so
/// an edit above a line, which moves its source offsets, keeps its shaping.
pub struct ShapedBox {
    pub rows: Vec<Row>,
    /// The base font's metrics, which every row's baseline follows.
    pub ascent: f32,
    pub descent: f32,
    pub line_height: f32,
    /// All rows together.
    pub height: f32,
    /// Width of the widest row.
    pub width: f32,
    /// Width with no wrapping at all, for sizing table columns.
    pub natural_width: f32,
}

impl ShapedBox {
    /// The top of a row's line of text, from the box's top. A row grown by an
    /// image grows upward: its text and the image's bottom stay on the
    /// row's bottom, as Obsidian's `vertical-align: bottom` has them.
    pub fn text_top(&self, row: &Row) -> f32 {
        row.top + row.height - self.line_height
    }

    /// The baseline of a row, from the box's top.
    pub fn baseline(&self, row: &Row) -> f32 {
        let padding = (self.line_height - self.ascent - self.descent) / 2.;
        self.text_top(row) + padding + self.ascent
    }

    /// The row at `y` from the box's top.
    pub fn row_at(&self, y: f32) -> usize {
        self.rows
            .iter()
            .position(|row| y < row.top + row.height)
            .unwrap_or(self.rows.len().saturating_sub(1))
    }

    /// The row a display offset sits on: at a wrap, the row after it.
    pub fn row_of(&self, display: usize) -> usize {
        let last = self.rows.len().saturating_sub(1);
        self.rows
            .iter()
            .position(|row| display < row.display.end)
            .unwrap_or(last)
    }

    /// The source offset nearest `x` on row `row` of `view`, the box this was
    /// shaped from.
    pub fn source_at(&self, view: &TextBox, row: usize, x: f32) -> usize {
        let row = &self.rows[row.min(self.rows.len() - 1)];
        // at a hidden gap a click lands before the markers, as in Obsidian
        view.source_of(row.display_at(x), Bias::Left)
    }

    /// Row and x of a source offset of `view`.
    pub fn locate(&self, view: &TextBox, source: usize) -> (usize, f32) {
        let display = view.display_of(source);
        let row = self.row_of(display);
        (row, self.rows[row].x_at(display))
    }
}

/// Unwrapped pen positions of a box's fragments.
struct Flat<'a> {
    frags: &'a [ShapedFrag],
}

struct ShapedFrag {
    line: ShapedLine,
    /// Flat display start.
    start: usize,
    /// Pen x of the outer left edge, before any pill padding.
    left: f32,
    pad_left: f32,
    pad_right: f32,
    width: f32,
    /// A fitted image's height: this fragment is that image.
    image_height: Option<f32>,
}

impl ShapedFrag {
    fn end(&self) -> usize {
        self.start + self.line.len()
    }

    fn right(&self) -> f32 {
        self.left + self.pad_left + self.width + self.pad_right
    }

    fn text_x(&self, flat: usize) -> f32 {
        let local = flat - self.start;
        let x = match self.image_height {
            Some(_) if local > 0 => self.width,
            Some(_) => 0.,
            None => self.line.x_for_index(local).as_f32(),
        };
        self.left + self.pad_left + x
    }
}

impl Flat<'_> {
    /// Pen position where a row starting at `flat` begins: a pill's padding
    /// belongs to the row its first glyph lands on.
    fn start_x(&self, flat: usize) -> f32 {
        match self
            .frags
            .iter()
            .find(|f| flat >= f.start && flat < f.end())
        {
            Some(frag) if frag.start == flat => frag.left,
            Some(frag) => frag.text_x(flat),
            None => self.frags.last().map_or(0., ShapedFrag::right),
        }
    }

    /// Pen position after the glyph before `flat`.
    fn end_x(&self, flat: usize) -> f32 {
        match self
            .frags
            .iter()
            .find(|f| flat > f.start && flat <= f.end())
        {
            Some(frag) if frag.end() == flat => frag.right(),
            Some(frag) => frag.text_x(flat),
            None => self.frags.first().map_or(0., |f| f.left),
        }
    }
}

/// Where a line of text may break: after whitespace, after a hyphen inside a
/// word, and around CJK characters and images. Never returns 0 or the end.
pub fn break_points(text: &str) -> Vec<usize> {
    let mut points = Vec::new();
    let mut prev: Option<char> = None;
    for (ix, c) in text.char_indices() {
        if let Some(p) = prev {
            let after_space = p.is_whitespace() && !c.is_whitespace();
            let after_hyphen = p == '-' && c.is_alphanumeric();
            let image = p == IMAGE_CHAR || c == IMAGE_CHAR;
            if after_space || after_hyphen || image || is_cjk(p) || is_cjk(c) {
                points.push(ix);
            }
        }
        prev = Some(c);
    }
    points
}

fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x2E80..=0x303F | 0x3040..=0x30FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF |
        0xAC00..=0xD7A3 | 0xF900..=0xFAFF | 0xFF00..=0xFFEF | 0x20000..=0x2FFFF)
}

/// Greedy line filling. `start_x`/`end_x` are the unwrapped pen positions;
/// rows after the first have `hang` less room. Trailing whitespace may hang
/// past the edge, and a word wider than a whole row breaks between characters
/// (`char_ends` lists the boundaries that allows).
pub fn wrap_rows(
    text: &str,
    width: f32,
    hang: f32,
    start_x: impl Fn(usize) -> f32,
    end_x: impl Fn(usize) -> f32,
) -> Vec<Range<usize>> {
    let len = text.len();
    if len == 0 || width <= 0. {
        return std::iter::once(0..len).collect();
    }
    let mut breaks = break_points(text);
    breaks.push(len);
    let mut rows = Vec::new();
    let mut row_start = 0usize;
    let mut fit: Option<usize> = None;
    let mut ix = 0usize;
    while ix < breaks.len() {
        let point = breaks[ix];
        let available = if rows.is_empty() { width } else { width - hang };
        let visible_end = row_start + text[row_start..point].trim_end().len();
        let needed = end_x(visible_end.max(row_start)) - start_x(row_start);
        if needed <= available {
            fit = Some(point);
            ix += 1;
            continue;
        }
        if let Some(at) = fit.take() {
            rows.push(row_start..at);
            row_start = at;
            continue;
        }
        // one word wider than the row: cut it where it stops fitting
        let origin = start_x(row_start);
        let mut cut = None;
        for (offset, c) in text[row_start..point].char_indices() {
            let end = row_start + offset + c.len_utf8();
            if end_x(end) - origin > available {
                break;
            }
            cut = Some(end);
        }
        let first_char = text[row_start..]
            .chars()
            .next()
            .map_or(len, |c| row_start + c.len_utf8());
        let at = cut.unwrap_or(first_char);
        if at >= len {
            break;
        }
        rows.push(row_start..at);
        row_start = at;
    }
    rows.push(row_start..len);
    rows
}

fn shape_frag(
    frag: &Frag,
    fonts: &Fonts,
    colors: &LiveColors,
    window: &mut Window,
) -> (ShapedLine, f32, f32) {
    let runs: Vec<TextRun> = frag
        .runs
        .iter()
        .map(|(len, style)| fonts.run(*len, style, colors))
        .collect();
    let line = window.text_system().shape_line(
        SharedString::from(frag.text.clone()),
        px(frag.size.0),
        &runs,
        None,
    );
    let pad = frag
        .pill
        .map_or(0., |pill| pill_padding(pill).0 * frag.size.0);
    (line, pad, pad)
}

/// The icon width after an external link.
pub fn link_icon_width(size: f32) -> f32 {
    size * 0.9
}

/// The room before a remove cross, as wide as its text is tall.
pub fn remove_gap(size: f32) -> f32 {
    size * REMOVE_GAP_EM
}

/// The base font's vertical metrics, as a shaped line reports them: the same
/// numbers paint uses to place a baseline.
fn base_metrics(
    size: f32,
    mono: bool,
    fonts: &Fonts,
    colors: &LiveColors,
    window: &mut Window,
) -> (f32, f32) {
    let style = RunStyle {
        ink: super::display::Ink::Text,
        weight: 400,
        italic: false,
        mono,
        strike: false,
        underline: None,
        background: None,
    };
    let run = fonts.run(1, &style, colors);
    let probe =
        window
            .text_system()
            .shape_line(SharedString::new_static(" "), px(size), &[run], None);
    (probe.ascent.as_f32(), probe.descent.as_f32())
}

/// Lays `view` out for `wrap_width`, one line of text `line_height` tall.
pub fn shape_box(
    view: &TextBox,
    wrap_width: f32,
    line_height: f32,
    fonts: &Fonts,
    colors: &LiveColors,
    window: &mut Window,
) -> ShapedBox {
    let (ascent, descent) = base_metrics(view.size.0, view.mono, fonts, colors, window);
    let mut frags: Vec<ShapedFrag> = Vec::with_capacity(view.frags.len());
    let mut pen = 0f32;
    let mut flat = 0usize;
    let mut text = String::new();
    for frag in &view.frags {
        let (line, pad_left, mut pad_right) = shape_frag(frag, fonts, colors, window);
        if frag.link_icon {
            pad_right += link_icon_width(frag.size.0);
        }
        if frag.remove {
            pad_right += remove_gap(frag.size.0) + frag.size.0;
        }
        // an image never gets wider than its row; rows after a hanging
        // indent have that much less
        let fitted = frag.image.as_ref().map(|image| {
            let hang = if view.hang_after > 0 {
                Flat { frags: &frags }.end_x(view.hang_after)
            } else {
                0.
            };
            fit_width(image.width.0, image.height.0, wrap_width - hang)
        });
        let width = fitted.map_or(line.width().as_f32(), |(width, _)| width);
        let start = flat;
        flat += line.len();
        frags.push(ShapedFrag {
            line,
            start,
            left: pen,
            pad_left,
            pad_right,
            width,
            image_height: fitted.map(|(_, height)| height),
        });
        pen += pad_left + width + pad_right;
        text.push_str(&frag.text);
    }
    let natural_width = pen;
    let geometry = Flat { frags: &frags };
    let hang = if view.hang_after > 0 {
        geometry.end_x(view.hang_after)
    } else {
        0.
    };
    let ranges = wrap_rows(
        &text,
        wrap_width,
        hang,
        |ix| geometry.start_x(ix),
        |ix| geometry.end_x(ix),
    );

    let mut rows = Vec::with_capacity(ranges.len());
    let mut cursors: Vec<_> = frags.iter().map(|frag| frag.line.cursor()).collect();
    let mut widest = 0f32;
    let mut top = 0f32;
    for (row_ix, range) in ranges.iter().enumerate() {
        let origin = geometry.start_x(range.start);
        let mut pieces = Vec::new();
        let mut height = line_height;
        for (ix, frag) in frags.iter().enumerate() {
            let from = range.start.max(frag.start);
            let to = range.end.min(frag.end());
            if from >= to {
                continue;
            }
            let view_frag = &view.frags[ix];
            let local_start = from - frag.start;
            let local_end = to - frag.start;
            let line = cursors[ix].take_until(local_end);
            let image = view_frag.image.as_ref().zip(frag.image_height);
            if let Some((_, image_height)) = image {
                height = height.max(image_height);
            }
            pieces.push(Piece {
                display: from..to,
                x: frag.text_x(from) - origin,
                width: if image.is_some() {
                    frag.width
                } else {
                    line.width().as_f32()
                },
                size: view_frag.size.0,
                raise: view_frag.raise.0,
                pill: view_frag.pill,
                pad_left: frag.pad_left,
                pad_right: frag.pad_right,
                opens: local_start == 0,
                closes: local_end == frag.line.len(),
                link_icon: view_frag.link_icon && local_end == frag.line.len(),
                embolden: fonts.embolden(view_frag),
                image: image.map(|(image, height)| PieceImage {
                    key: image.key.clone(),
                    height,
                }),
                remove: view_frag.remove && local_end == frag.line.len(),
                line,
            });
        }
        let width = geometry.end_x(range.end) - origin;
        let x = if row_ix == 0 { 0. } else { hang };
        widest = widest.max(x + width);
        rows.push(Row {
            display: range.clone(),
            x,
            width,
            pieces,
            top,
            height,
        });
        top += height;
    }
    drop(cursors);
    ShapedBox {
        rows,
        ascent,
        descent,
        line_height,
        height: top,
        width: widest,
        natural_width,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fake font: every byte is 10px wide.
    fn wrap(text: &str, width: f32, hang: f32) -> Vec<String> {
        wrap_rows(
            text,
            width,
            hang,
            |ix| ix as f32 * 10.,
            |ix| ix as f32 * 10.,
        )
        .into_iter()
        .map(|range| text[range].to_string())
        .collect()
    }

    #[test]
    fn images_break_on_both_sides() {
        let text = format!("ab{IMAGE_CHAR}cd");
        let got = wrap(&text, 30., 0.);
        assert_eq!(got, ["ab", &IMAGE_CHAR.to_string(), "cd"]);
    }

    #[test]
    fn words_stay_whole_and_trailing_spaces_hang() {
        assert_eq!(wrap("aaa bbb ccc", 70., 0.), ["aaa bbb ", "ccc"]);
        assert_eq!(wrap("aaa bbb ccc", 110., 0.), ["aaa bbb ccc"]);
        assert_eq!(wrap("aaa bbb ccc", 40., 0.), ["aaa ", "bbb ", "ccc"]);
    }

    #[test]
    fn a_word_wider_than_the_row_breaks_between_characters() {
        assert_eq!(wrap("abcdefgh", 30., 0.), ["abc", "def", "gh"]);
        assert_eq!(wrap("x abcdefgh", 30., 0.), ["x ", "abc", "def", "gh"]);
    }

    #[test]
    fn rows_after_the_first_have_the_hang_less_room() {
        assert_eq!(wrap("aaa bbb ccc", 80., 20.), ["aaa bbb ", "ccc"]);
        assert_eq!(
            wrap("aaa bbb ccc ddd", 80., 20.),
            ["aaa bbb ", "ccc ", "ddd"]
        );
    }

    #[test]
    fn nothing_fits_still_makes_progress() {
        assert_eq!(wrap("abc", 5., 0.), ["a", "b", "c"]);
        assert_eq!(wrap("", 50., 0.), [""]);
        assert_eq!(wrap("abc", 0., 0.), ["abc"]);
    }

    #[test]
    fn hyphens_and_cjk_are_break_opportunities() {
        assert_eq!(break_points("a-b"), [2]);
        assert_eq!(break_points("a - b"), [2, 4]);
        assert_eq!(break_points("漢字"), [3]);
        assert_eq!(break_points("ab"), Vec::<usize>::new());
    }
}
