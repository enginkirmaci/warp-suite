//! What one source line looks like right now: the pure step between the parsed
//! document and the window. Given the selection, it decides which source
//! markers are hidden (Obsidian's live preview rules), and produces the
//! display text with a map back to source offsets, the style runs, and the
//! line's decorations. All sizes derive from the base font size, in the
//! proportions of Obsidian's default theme.

use super::code::CodeToken;
use super::diagram::{DiagramStatus, MERMAID};
use super::images::{ImageStatus, requested_size};
use crate::live::{
    InlineObject, InlineObjectKind, LiveBlock, LiveBlockKind, LiveDocument, LiveLine, LiveLineKind,
    MarkdownLiveKind, MarkdownLiveSpan, Property, PropertyType, TableAlign,
};
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::rc::Rc;

/// A length in pixels that can key the layout cache.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Px(pub f32);

impl Eq for Px {}

impl Hash for Px {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.to_bits().hash(state);
    }
}

/// A color by role, so views stay comparable and follow the app's preset.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Ink {
    Text,
    Muted,
    Faint,
    Accent,
    Link,
    /// A `[[wikilink]]` to a note that does not exist.
    Unresolved,
    /// The underline of one.
    UnresolvedLine,
    /// Occupies its space without painting: a parked quote `>`.
    Clear,
    Highlight,
    TagBackground,
    Tone(Tone),
    /// A syntax token's own color, 0xRRGGBBAA.
    Rgba(u32),
}

/// A callout's color family; the palette has one hue per family.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tone {
    Blue,
    Accent,
    Green,
    Amber,
    Orange,
    Red,
    Purple,
    Gray,
}

/// A callout's title icon.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Glyph {
    Pencil,
    ClipboardList,
    Info,
    CircleCheck,
    Flame,
    Check,
    CircleHelp,
    TriangleAlert,
    X,
    Zap,
    Bug,
    List,
    Quote,
    Text,
    Binary,
    SquareCheck,
    Calendar,
    Clock,
    Tags,
    Forward,
    Plus,
}

/// How a run of text paints.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RunStyle {
    pub ink: Ink,
    pub weight: u16,
    pub italic: bool,
    pub mono: bool,
    pub strike: bool,
    pub underline: Option<Ink>,
    pub background: Option<Ink>,
}

impl RunStyle {
    const fn plain(ink: Ink) -> Self {
        Self {
            ink,
            weight: 400,
            italic: false,
            mono: false,
            strike: false,
            underline: None,
            background: None,
        }
    }
}

/// A pill painted behind a fragment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Pill {
    Code,
    Tag,
    /// A tag in the Properties panel.
    Item,
}

/// Stands in for an image in display text: one unbreakable "glyph".
pub const IMAGE_CHAR: char = '\u{FFFC}';

/// An image flowing with the text, at its requested size.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FragImage {
    /// Its key in the image store.
    pub key: String,
    pub width: Px,
    pub height: Px,
}

/// A stretch of one line sharing a font size and backdrop.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Frag {
    pub text: String,
    pub size: Px,
    /// (byte length, style), covering `text`.
    pub runs: Vec<(usize, RunStyle)>,
    pub pill: Option<Pill>,
    /// Upward baseline shift, for superscripts.
    pub raise: Px,
    /// An external-link arrow after the fragment.
    pub link_icon: bool,
    /// The font weight every run shares: a family without that weight
    /// emboldens the whole fragment.
    pub weight: u16,
    /// This fragment is an image, its text a single [`IMAGE_CHAR`].
    pub image: Option<FragImage>,
    /// A cross after the text takes it out of its list property.
    pub remove: bool,
}

/// One visible stretch: display bytes and the source bytes they show. The two
/// lengths differ only where a tab stands in as spaces or synthetic text
/// stands for nothing.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Seg {
    pub display: Range<usize>,
    pub source: Range<usize>,
}

/// Which side of a hidden gap a display offset resolves to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bias {
    Left,
    Right,
}

/// One run of flowing text and its way back to the source.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct TextBox {
    pub frags: Vec<Frag>,
    pub map: Vec<Seg>,
    /// Wrapped rows align under this display offset: the text after a list
    /// number or a quote prefix.
    pub hang_after: usize,
    /// Font metrics the row's baseline follows.
    pub size: Px,
    pub mono: bool,
    /// The source position an empty box stands for.
    pub anchor: usize,
}

impl TextBox {
    /// The whole display text.
    #[cfg(test)]
    pub fn display_text(&self) -> String {
        self.frags.iter().map(|f| f.text.as_str()).collect()
    }

    pub fn display_len(&self) -> usize {
        self.frags.iter().map(|f| f.text.len()).sum()
    }

    /// The display offset showing source offset `source`. A hidden source
    /// offset lands on the visible text after it.
    pub fn display_of(&self, source: usize) -> usize {
        for seg in &self.map {
            if source < seg.source.start {
                return seg.display.start;
            }
            if source <= seg.source.end {
                return if seg.display.len() == seg.source.len() {
                    seg.display.start + (source - seg.source.start)
                } else if source == seg.source.start {
                    seg.display.start
                } else {
                    seg.display.end
                };
            }
        }
        self.display_len()
    }

    /// The source offset display offset `display` stands for. At a hidden
    /// gap `bias` picks the side; inside a widened tab, the closer end.
    pub fn source_of(&self, display: usize, bias: Bias) -> usize {
        let mut best = None;
        for seg in &self.map {
            if display < seg.display.start {
                break;
            }
            if display > seg.display.end {
                continue;
            }
            let source = if seg.display.len() == seg.source.len() {
                seg.source.start + (display - seg.display.start)
            } else if display * 2 < seg.display.start + seg.display.end {
                seg.source.start
            } else {
                seg.source.end
            };
            match bias {
                Bias::Left => return source,
                Bias::Right => best = Some(source),
            }
        }
        best.unwrap_or_else(|| self.map.last().map_or(self.anchor, |seg| seg.source.end))
    }
}

/// A line's vertical metrics.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Height {
    pub line: Px,
    pub top: Px,
    pub bottom: Px,
}

/// What sits behind a run of lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Band {
    Code,
    Callout(Tone),
}

/// Something painted around a line's text.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Decor {
    Bullet {
        center: Px,
    },
    Checkbox {
        x: Px,
        checked: bool,
        /// The `[ ]` in the source.
        source: Range<usize>,
    },
    /// A quote bar at the x of this display offset.
    QuoteBar {
        offset: usize,
    },
    Guide {
        x: Px,
    },
    Rule,
    Band {
        band: Band,
        first: bool,
        last: bool,
    },
    /// A code block's language, right-aligned on the fence row.
    Label(String),
    Icon {
        glyph: Glyph,
        tone: Tone,
        x: Px,
    },
}

/// An image rendered under (or instead of) a line's text, at its requested
/// size.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ImageView {
    /// Its key in the image store.
    pub key: String,
    pub width: Px,
    pub height: Px,
}

/// One table cell's text.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CellView {
    pub text: Rc<TextBox>,
    pub align: TableAlign,
    pub source: Range<usize>,
}

/// Where a table row sits in its table.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TableRowView {
    pub block: usize,
    pub cells: Vec<CellView>,
    pub header: bool,
    pub first: bool,
    pub last: bool,
}

/// One property row: the key on the left, its value in a fixed column. The
/// boxes hold the caret the way table cells do.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PropertyView {
    pub key: Rc<TextBox>,
    pub value: Rc<TextBox>,
    /// The source each box edits.
    pub key_source: Range<usize>,
    pub value_source: Range<usize>,
    /// A list: a click past its items starts another.
    pub list: bool,
}

/// What a click on a line does besides placing the caret.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum LineRole {
    #[default]
    Text,
    /// Nothing at all: the Properties heading.
    Inert,
    /// The Add property button.
    AddProperty,
}

/// Everything the layout needs for one source line.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct LineView {
    pub height: Height,
    /// Text box left edge in the content column.
    pub x: Px,
    /// Space kept free on the right of the text.
    pub right: Px,
    pub text: Rc<TextBox>,
    pub decor: Vec<Decor>,
    /// A line that takes no space: a table's delimiter row.
    pub hidden: bool,
    pub image: Option<ImageView>,
    pub table: Option<TableRowView>,
    pub property: Option<PropertyView>,
    /// This line's image is the rendered block of this index.
    pub diagram: Option<usize>,
    /// A hidden line whose text shows on this other line: a list item in its
    /// property's row.
    pub host: Option<usize>,
    pub role: LineRole,
}

/// The selection the reveal rules test against; `None` while the editor has no
/// focus, which parks every construct.
#[derive(Clone, Debug, Default)]
pub struct Reveal {
    pub selection: Option<Range<usize>>,
}

impl Reveal {
    /// Whether the selection touches `range`, both ends inclusive: a caret
    /// right after `**bold**` still counts.
    pub fn touches(&self, range: &Range<usize>) -> bool {
        self.selection
            .as_ref()
            .is_some_and(|sel| sel.start <= range.end && sel.end >= range.start)
    }

    /// The narrower test for list markers: the caret right after the marker
    /// is already in the item's text and leaves the bullet rendered.
    pub fn in_marker(&self, range: &Range<usize>) -> bool {
        self.selection
            .as_ref()
            .is_some_and(|sel| sel.start < range.end && sel.end >= range.start)
    }
}

/// Everything [`build_view`] reads.
pub struct ViewContext<'a> {
    pub text: &'a str,
    pub doc: &'a LiveDocument,
    pub reveal: Reveal,
    /// The base font size; all metrics are em-relative.
    pub em: f32,
    /// Syntax tokens per code block, as offsets into the document.
    pub tokens: &'a dyn Fn(usize) -> Option<std::rc::Rc<Vec<CodeToken>>>,
    /// The notes `[[wikilinks]]` can resolve to, by [`note_key`].
    pub notes: Option<&'a std::collections::HashSet<String>>,
    /// How an image source stands right now.
    pub images: &'a dyn Fn(&str) -> ImageStatus,
    /// How a Mermaid block stands right now.
    pub diagrams: &'a dyn Fn(usize) -> DiagramStatus,
}

// Obsidian's default theme metrics, in em of the base size unless noted.
const LINE_HEIGHT: f32 = 1.5;
const HEADING_SIZES: [f32; 6] = [1.618, 1.462, 1.318, 1.188, 1.076, 1.0];
const HEADING_WEIGHTS: [u16; 6] = [700, 680, 660, 640, 620, 600];
const HEADING_LINE_HEIGHTS: [f32; 6] = [1.2, 1.2, 1.3, 1.4, 1.5, 1.5];
/// H1 and H2 carry one extra paragraph gap above them.
const HEADING_GAP_LEVELS: u8 = 2;
const BOLD_WEIGHT: u16 = 600;
const CODE_SCALE: f32 = 0.875;
const FOOTNOTE_REF_SCALE: f32 = 0.8;
const FOOTNOTE_REF_RAISE: f32 = 0.2;
const LIST_PAD: f32 = 0.075;
const LEVEL_INDENT: f32 = 2.25;
const LIST_MARKER_X: f32 = 0.75;
const BULLET_TEXT_X: f32 = 1.3125;
const BULLET_CENTER_X: f32 = 0.9163;
const GUIDE_X: f32 = 0.9;
const CHECKBOX_X: f32 = 0.4581;
const TASK_TEXT_X: f32 = 1.9375;
const CODE_PAD: f32 = 1.0;
/// Below a rendered diagram.
const DIAGRAM_GAP: f32 = 0.4375;
const CALLOUT_PAD_LEFT: f32 = 1.5;
const CALLOUT_PAD: f32 = 0.75;
const CALLOUT_TITLE_LINE_HEIGHT: f32 = 1.3;
const CALLOUT_ICON_GAP: f32 = 1.25;
const PARAGRAPH_GAP: f32 = 1.0;
const TABLE_MARGIN: f32 = 1.0;
const TABLE_LINE_HEIGHT: f32 = 1.3;
const TABLE_CELL_PAD_Y: f32 = 0.25;
const PROPERTY_SIZE: f32 = 0.875;
const PROPERTY_ICON_X: f32 = 0.;
const PROPERTY_KEY_X: f32 = 1.5;
const PROPERTY_VALUE_X: f32 = 9.25;
const PROPERTY_ROW_HEIGHT: f32 = 1.8125;
const PROPERTY_ROW_GAP: f32 = 0.1875;
const PROPERTY_TEXT_LINE: f32 = 1.3125;
const PROPERTY_PANEL_PAD: f32 = 0.5;
const PROPERTY_HEADING_LINE: f32 = 1.7;
const PROPERTY_HEADING_WEIGHT: u16 = 500;
const PROPERTY_ADD_TOP: f32 = 0.25;
const PROPERTY_ADD_ICON_X: f32 = 0.125;
const PROPERTY_ADD_TEXT_X: f32 = 1.375;
/// Below the panel: its padding and Obsidian's margin.
const PROPERTY_ADD_BOTTOM: f32 = 2.5;

/// Spaces a tab takes in display text; hanging text stays aligned without a
/// text system that understands tab stops.
const TAB_SPACES: &str = "    ";

/// Text under construction.
struct Builder<'a> {
    text: &'a str,
    frags: Vec<Frag>,
    map: Vec<Seg>,
    flat: usize,
    closed: bool,
}

impl<'a> Builder<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            text,
            frags: Vec::new(),
            map: Vec::new(),
            flat: 0,
            closed: true,
        }
    }

    /// Starts a new fragment for the next push even when it could continue
    /// this one: pills never merge.
    fn close(&mut self) {
        self.closed = true;
    }

    fn frag(&mut self, size: Px, pill: Option<Pill>, raise: Px, weight: u16) -> &mut Frag {
        let reuse = !self.closed
            && self.frags.last().is_some_and(|f| {
                f.size == size && f.pill == pill && f.raise == raise && f.weight == weight
            });
        if !reuse {
            self.frags.push(Frag {
                size,
                pill,
                raise,
                weight,
                ..Frag::default()
            });
            self.closed = false;
        }
        self.frags.last_mut().expect("a fragment was just ensured")
    }

    /// Appends `text` as display, recording it as showing `source`.
    fn raw(
        &mut self,
        text: &str,
        source: Range<usize>,
        style: RunStyle,
        size: Px,
        pill: Option<Pill>,
        raise: Px,
    ) {
        if text.is_empty() {
            return;
        }
        let frag = self.frag(size, pill, raise, style.weight);
        frag.text.push_str(text);
        match frag.runs.last_mut() {
            Some((len, last)) if *last == style => *len += text.len(),
            _ => frag.runs.push((text.len(), style)),
        }
        let display = self.flat..self.flat + text.len();
        self.flat = display.end;
        match self.map.last_mut() {
            Some(seg)
                if seg.display.end == display.start
                    && seg.source.end == source.start
                    && seg.display.len() == seg.source.len()
                    && display.len() == source.len() =>
            {
                seg.display.end = display.end;
                seg.source.end = source.end;
            }
            _ => self.map.push(Seg { display, source }),
        }
    }

    /// Appends `image` as its own fragment, standing for `source`.
    fn image(&mut self, image: FragImage, source: Range<usize>, base: InlineBase) {
        let display = self.flat..self.flat + IMAGE_CHAR.len_utf8();
        self.flat = display.end;
        self.frags.push(Frag {
            text: IMAGE_CHAR.to_string(),
            size: base.size,
            runs: vec![(display.len(), base.style)],
            weight: base.style.weight,
            image: Some(image),
            ..Frag::default()
        });
        self.closed = true;
        self.map.push(Seg { display, source });
    }

    /// Gives the last fragment a remove cross, and keeps it to itself.
    fn removable(&mut self) {
        if let Some(frag) = self.frags.last_mut() {
            frag.remove = true;
        }
        self.closed = true;
    }

    /// Appends the source range `range`, widening tabs.
    fn source(
        &mut self,
        range: Range<usize>,
        style: RunStyle,
        size: Px,
        pill: Option<Pill>,
        raise: Px,
    ) {
        let text = self.text;
        let mut start = range.start;
        for (ix, _) in text[range.clone()].match_indices('\t') {
            let at = range.start + ix;
            self.raw(&text[start..at], start..at, style, size, pill, raise);
            self.raw(TAB_SPACES, at..at + 1, style, size, pill, raise);
            start = at + 1;
        }
        self.raw(
            &text[start..range.end],
            start..range.end,
            style,
            size,
            pill,
            raise,
        );
    }

    /// Appends display text that stands for no source at all.
    fn synthetic(&mut self, text: &str, at: usize, style: RunStyle, size: Px) {
        self.raw(text, at..at, style, size, None, Px(0.));
    }

    fn finish(self, size: Px, mono: bool, anchor: usize) -> Rc<TextBox> {
        self.finish_hanging(size, mono, anchor, 0)
    }

    /// Finishes a box whose wrapped rows align under display offset
    /// `hang_after`.
    fn finish_hanging(self, size: Px, mono: bool, anchor: usize, hang_after: usize) -> Rc<TextBox> {
        Rc::new(TextBox {
            frags: self.frags,
            map: self.map,
            hang_after,
            size,
            mono,
            anchor,
        })
    }
}

/// The styling a line's inline text starts from.
#[derive(Clone, Copy)]
struct InlineBase {
    style: RunStyle,
    size: Px,
}

impl InlineBase {
    fn body(em: f32) -> Self {
        Self {
            style: RunStyle::plain(Ink::Text),
            size: Px(em),
        }
    }
}

/// Appends a line's spans from `from` on, hiding parked syntax.
fn push_spans(
    b: &mut Builder<'_>,
    cx: &ViewContext<'_>,
    line: &LiveLine,
    range: Range<usize>,
    base: InlineBase,
) {
    for (ix, span) in line.spans.iter().enumerate() {
        let start = span.range.start.max(range.start);
        let end = span.range.end.min(range.end);
        if start >= end {
            continue;
        }
        let clipped = start..end;
        // an image speaks for every span of its syntax
        if let Some(object) = line.objects.iter().find(|object| {
            matches!(object.kind, InlineObjectKind::Image { .. })
                && object.range.start <= start
                && end <= object.range.end
        }) {
            if span.range.start == object.range.start {
                push_image(b, cx, object, base);
            }
            continue;
        }
        if span
            .owner
            .as_ref()
            .is_some_and(|owner| !cx.reveal.touches(owner))
        {
            // the arrow after a parked external link
            if span.kind == MarkdownLiveKind::Marker
                && line.spans.get(ix.wrapping_sub(1)).is_some_and(|prev| {
                    prev.kind == MarkdownLiveKind::LinkUrl
                        && prev.owner.is_some()
                        && is_external(line, prev)
                })
                && let Some(frag) = b.frags.last_mut()
            {
                frag.link_icon = true;
                b.close();
            }
            continue;
        }
        push_span(b, cx, line, ix, span, clipped, base);
    }
}

/// An image inside a sentence: the picture, with its source in front of it
/// while the selection touches it. One that cannot be shown leaves its source.
fn push_image(b: &mut Builder<'_>, cx: &ViewContext<'_>, object: &InlineObject, base: InlineBase) {
    let InlineObjectKind::Image { src, width, height } = &object.kind else {
        return;
    };
    let status = (cx.images)(src);
    let touched = cx.reveal.touches(&object.range);
    if touched || status == ImageStatus::Missing {
        let faint = RunStyle::plain(Ink::Faint);
        b.source(object.range.clone(), faint, base.size, None, Px(0.));
    }
    if let ImageStatus::Ready {
        width: natural_w,
        height: natural_h,
    } = status
    {
        let (w, h) = requested_size(natural_w, natural_h, *width, *height);
        let image = FragImage {
            key: src.clone(),
            width: Px(w),
            height: Px(h),
        };
        let end = object.range.end;
        let source = if touched {
            end..end
        } else {
            object.range.clone()
        };
        b.image(image, source, base);
    }
}

/// `https://…` and friends get the arrow; a relative target does not.
fn is_external(line: &LiveLine, url: &MarkdownLiveSpan) -> bool {
    line.objects.iter().any(|object| {
        object.range.start <= url.range.start
            && url.range.end <= object.range.end
            && matches!(&object.kind, InlineObjectKind::Link { url } if url.contains("://"))
    })
}

/// Whether the `[[wikilink]]` at `range` names a known note. With no note list
/// from the host every link counts as resolved.
fn wiki_target_exists(cx: &ViewContext<'_>, line: &LiveLine, range: &Range<usize>) -> bool {
    let Some(notes) = cx.notes else {
        return true;
    };
    line.objects.iter().any(|object| {
        object.range.start <= range.start
            && range.end <= object.range.end
            && matches!(&object.kind, InlineObjectKind::WikiLink { target } if notes.contains(&note_key(target)))
    })
}

/// The name a link target resolves by: the file stem, lowercased, with any
/// `#heading` or `^block` part gone.
pub fn note_key(target: &str) -> String {
    let name = target.split(['#', '^']).next().unwrap_or(target).trim();
    let file = name.rsplit(['/', '\\']).next().unwrap_or(name);
    file.strip_suffix(".md").unwrap_or(file).to_lowercase()
}

fn emphasized(mut style: RunStyle, span: &MarkdownLiveSpan) -> RunStyle {
    if span.emphasis.bold {
        style.weight = style.weight.max(BOLD_WEIGHT);
    }
    style.italic |= span.emphasis.italic;
    style.strike |= span.emphasis.strike;
    if span.emphasis.highlight {
        style.background = Some(Ink::Highlight);
    }
    style
}

fn push_span(
    b: &mut Builder<'_>,
    cx: &ViewContext<'_>,
    line: &LiveLine,
    ix: usize,
    span: &MarkdownLiveSpan,
    range: Range<usize>,
    base: InlineBase,
) {
    let styled = emphasized(base.style, span);
    let none = Px(0.);
    let code_neighbor = |ix: Option<usize>| {
        ix.and_then(|ix| line.spans.get(ix))
            .is_some_and(|s| s.kind == MarkdownLiveKind::InlineCode)
    };
    match span.kind {
        MarkdownLiveKind::Marker => {
            // a revealed backtick sits inside the pill with its code
            let opens_code = code_neighbor(Some(ix + 1));
            let closes_code = code_neighbor(ix.checked_sub(1));
            if opens_code || closes_code {
                let code = code_style(styled, base.size);
                if opens_code {
                    b.close();
                }
                b.source(range, code.0, code.1, Some(Pill::Code), none);
                if closes_code {
                    b.close();
                }
            } else {
                // emphasis marks look like what they emphasize; link brackets
                // and escapes are the quiet kind of syntax
                let emphasis = matches!(cx.text.as_bytes()[range.start], b'*' | b'_' | b'~' | b'=');
                let style = if emphasis {
                    styled
                } else {
                    RunStyle {
                        ink: Ink::Faint,
                        ..styled
                    }
                };
                b.source(range, style, base.size, None, none);
            }
        }
        MarkdownLiveKind::InlineCode => {
            let revealed = code_neighbor(None)
                || line
                    .spans
                    .get(ix.wrapping_sub(1))
                    .is_some_and(|prev| prev.owner.as_ref().is_some_and(|o| cx.reveal.touches(o)));
            let code = code_style(styled, base.size);
            if !revealed {
                b.close();
            }
            b.source(range, code.0, code.1, Some(Pill::Code), none);
            if !revealed {
                b.close();
            }
        }
        MarkdownLiveKind::LinkText => {
            let link = RunStyle {
                ink: Ink::Link,
                underline: Some(Ink::Link),
                ..styled
            };
            b.source(range, link, base.size, None, none);
        }
        MarkdownLiveKind::WikiLink => {
            let (ink, underline) = if wiki_target_exists(cx, line, &range) {
                (Ink::Link, Ink::Link)
            } else {
                (Ink::Unresolved, Ink::UnresolvedLine)
            };
            let link = RunStyle {
                ink,
                underline: Some(underline),
                ..styled
            };
            b.source(range, link, base.size, None, none);
        }
        MarkdownLiveKind::LinkUrl => {
            let style = if span.owner.is_some() {
                RunStyle {
                    ink: Ink::Faint,
                    underline: Some(Ink::Faint),
                    ..styled
                }
            } else {
                RunStyle {
                    ink: Ink::Link,
                    underline: Some(Ink::Link),
                    ..styled
                }
            };
            b.source(range, style, base.size, None, none);
        }
        MarkdownLiveKind::Embed => {
            let style = RunStyle {
                ink: Ink::Link,
                ..styled
            };
            b.source(range, style, base.size, None, none);
        }
        MarkdownLiveKind::Tag => {
            let size = Px(base.size.0 * CODE_SCALE);
            let style = RunStyle {
                ink: Ink::Link,
                ..styled
            };
            b.close();
            b.source(range, style, size, Some(Pill::Tag), none);
            b.close();
        }
        MarkdownLiveKind::FootnoteRef => {
            let size = Px(base.size.0 * FOOTNOTE_REF_SCALE);
            let raise = Px(base.size.0 * FOOTNOTE_REF_RAISE);
            let style = RunStyle {
                ink: Ink::Faint,
                ..styled
            };
            b.source(range, style, size, None, raise);
        }
        MarkdownLiveKind::Comment => {
            let style = RunStyle {
                ink: Ink::Faint,
                ..styled
            };
            b.source(range, style, base.size, None, none);
        }
        _ => b.source(range, styled, base.size, None, none),
    }
}

/// Inline code: mono at 0.875em of its surroundings.
fn code_style(style: RunStyle, size: Px) -> (RunStyle, Px) {
    (
        RunStyle {
            mono: true,
            ..style
        },
        Px(size.0 * CODE_SCALE),
    )
}

fn body_height(em: f32) -> Height {
    Height {
        line: Px(em * LINE_HEIGHT),
        ..Height::default()
    }
}

/// Builds the view of line `ix`.
pub fn build_view(cx: &ViewContext<'_>, ix: usize) -> LineView {
    let line = &cx.doc.lines[ix];
    if let Some(block_ix) = line.block {
        let block = &cx.doc.blocks[block_ix];
        match &block.kind {
            LiveBlockKind::Code { lang } => {
                return code_line(cx, ix, block_ix, block, lang.as_deref());
            }
            LiveBlockKind::Table { .. } => return table_line(cx, ix, block_ix, block),
            LiveBlockKind::Callout { .. } if !cx.reveal.touches(&block.range) => {
                return callout_line(cx, ix, block);
            }
            LiveBlockKind::Frontmatter { .. } => return frontmatter_line(cx, ix, block),
            LiveBlockKind::Callout { .. } => {}
        }
    }
    if line.quote.is_some() {
        return quote_line(cx, ix);
    }
    match &line.kind {
        LiveLineKind::Blank => empty_line(cx, line),
        LiveLineKind::Heading { level, marker } => heading_line(cx, line, *level, marker),
        LiveLineKind::List(_) | LiveLineKind::ListContinuation { .. } => list_line(cx, line),
        LiveLineKind::FootnoteDefinition { label, marker } => {
            footnote_line(cx, line, label, marker)
        }
        LiveLineKind::Rule => rule_line(cx, line),
        LiveLineKind::SetextUnderline => source_line(cx, line, Ink::Faint),
        LiveLineKind::IndentedCode => indented_code_line(cx, line),
        LiveLineKind::Fence | LiveLineKind::Code | LiveLineKind::Frontmatter => {
            source_line(cx, line, Ink::Text)
        }
        LiveLineKind::Paragraph => paragraph_line(cx, line),
    }
}

fn empty_line(cx: &ViewContext<'_>, line: &LiveLine) -> LineView {
    LineView {
        height: body_height(cx.em),
        text: empty_box(Px(cx.em), false, line.range.start),
        ..LineView::default()
    }
}

fn empty_box(size: Px, mono: bool, anchor: usize) -> Rc<TextBox> {
    Rc::new(TextBox {
        size,
        mono,
        anchor,
        ..TextBox::default()
    })
}

/// The line's source shown verbatim.
fn source_line(cx: &ViewContext<'_>, line: &LiveLine, ink: Ink) -> LineView {
    let mut b = Builder::new(cx.text);
    let size = Px(cx.em);
    b.source(line.range.clone(), RunStyle::plain(ink), size, None, Px(0.));
    LineView {
        height: body_height(cx.em),
        text: b.finish(size, false, line.range.start),
        ..LineView::default()
    }
}

/// A paragraph, or a block image when an image is all the line holds.
fn paragraph_line(cx: &ViewContext<'_>, line: &LiveLine) -> LineView {
    if let Some(image) = block_image(cx, line) {
        return image_line(cx, line, image);
    }
    let mut b = Builder::new(cx.text);
    let base = InlineBase::body(cx.em);
    push_spans(&mut b, cx, line, line.range.clone(), base);
    LineView {
        height: body_height(cx.em),
        text: b.finish(base.size, false, line.range.start),
        ..LineView::default()
    }
}

/// The image object filling the whole line, if it can be shown.
fn block_image<'l>(cx: &ViewContext<'_>, line: &'l LiveLine) -> Option<&'l InlineObject> {
    let body = &cx.text[line.range.clone()];
    let trimmed = body.trim();
    let start = line.range.start + (body.len() - body.trim_start().len());
    let whole = start..start + trimmed.len();
    line.objects.iter().find(|o| {
        o.range == whole
            && matches!(&o.kind, InlineObjectKind::Image { src, .. }
                if (cx.images)(src) != ImageStatus::Missing)
    })
}

/// A line holding only an image: the image itself, with the source shown
/// above it while the caret is inside so it can be edited.
fn image_line(cx: &ViewContext<'_>, line: &LiveLine, object: &InlineObject) -> LineView {
    let InlineObjectKind::Image { src, width, height } = &object.kind else {
        unreachable!("block_image only returns image objects");
    };
    let text = if cx.reveal.touches(&line.range) {
        let mut b = Builder::new(cx.text);
        let faint = RunStyle::plain(Ink::Faint);
        b.source(line.range.clone(), faint, Px(cx.em), None, Px(0.));
        b.finish(Px(cx.em), false, line.range.start)
    } else {
        empty_box(Px(cx.em), false, line.range.start)
    };
    let image = match (cx.images)(src) {
        ImageStatus::Ready {
            width: natural_w,
            height: natural_h,
        } => {
            let (w, h) = requested_size(natural_w, natural_h, *width, *height);
            Some(ImageView {
                key: src.clone(),
                width: Px(w),
                height: Px(h),
            })
        }
        ImageStatus::Loading | ImageStatus::Missing => None,
    };
    LineView {
        height: body_height(cx.em),
        text,
        image,
        ..LineView::default()
    }
}

fn heading_line(
    cx: &ViewContext<'_>,
    line: &LiveLine,
    level: u8,
    marker: &Range<usize>,
) -> LineView {
    let index = usize::from(level.clamp(1, 6) - 1);
    let size = Px(cx.em * HEADING_SIZES[index]);
    let weight = HEADING_WEIGHTS[index];
    let base = InlineBase {
        style: RunStyle {
            weight,
            ..RunStyle::plain(Ink::Text)
        },
        size,
    };
    let mut b = Builder::new(cx.text);
    if cx.reveal.touches(&line.range) {
        let faint = RunStyle {
            ink: Ink::Faint,
            ..base.style
        };
        b.source(marker.clone(), faint, size, None, Px(0.));
    }
    push_spans(&mut b, cx, line, marker.end..line.range.end, base);
    let top = if level <= HEADING_GAP_LEVELS {
        cx.em * PARAGRAPH_GAP
    } else {
        0.
    };
    LineView {
        height: Height {
            line: Px(size.0 * HEADING_LINE_HEIGHTS[index]),
            top: Px(top),
            bottom: Px(0.),
        },
        text: b.finish(size, false, line.range.start),
        ..LineView::default()
    }
}

/// Level indent plus the guides of every ancestor level.
fn list_guides(em: f32, level: u8) -> Vec<Decor> {
    (0..level)
        .map(|k| Decor::Guide {
            x: Px(em * (GUIDE_X + LEVEL_INDENT * f32::from(k))),
        })
        .collect()
}

fn list_line(cx: &ViewContext<'_>, line: &LiveLine) -> LineView {
    let em = cx.em;
    let height = Height {
        line: Px(em * LINE_HEIGHT),
        top: Px(em * LIST_PAD),
        bottom: Px(em * LIST_PAD),
    };
    let base = InlineBase::body(em);
    let size = Px(em);
    let mut b = Builder::new(cx.text);

    let item = match &line.kind {
        LiveLineKind::List(item) => item,
        LiveLineKind::ListContinuation { level } => {
            let ws = cx.text[line.range.clone()].len()
                - cx.text[line.range.clone()]
                    .trim_start_matches([' ', '\t'])
                    .len();
            push_spans(
                &mut b,
                cx,
                line,
                line.range.start + ws..line.range.end,
                base,
            );
            return LineView {
                height,
                x: Px(em * (BULLET_TEXT_X + LEVEL_INDENT * f32::from(*level))),
                text: b.finish(size, false, line.range.start),
                decor: list_guides(em, *level),
                ..LineView::default()
            };
        }
        _ => unreachable!("list_line only gets list lines"),
    };

    let indent = em * LEVEL_INDENT * f32::from(item.level);
    let content = item.content_start().min(line.range.end);
    let faint = RunStyle::plain(Ink::Faint);
    let mut decor = list_guides(em, item.level);
    let done = item.task.as_ref().is_some_and(|t| t.is_done());
    let inline_base = if done {
        InlineBase {
            style: RunStyle::plain(Ink::Muted),
            size,
        }
    } else {
        base
    };

    // the marker itself shows raw only with the caret inside it
    let prefix = item.marker.start..content;
    if cx.reveal.in_marker(&prefix) || item.number.is_some() {
        let marker_style = RunStyle {
            strike: done,
            ..faint
        };
        b.source(item.marker.clone(), marker_style, size, None, Px(0.));
        if let Some(task) = &item.task {
            let muted = RunStyle {
                strike: done,
                ..RunStyle::plain(Ink::Muted)
            };
            let mono = RunStyle {
                mono: true,
                ..muted
            };
            b.source(task.range.clone(), mono, size, None, Px(0.));
            b.source(task.range.end..content, muted, size, None, Px(0.));
        }
        let hang_after = b.flat;
        push_spans(&mut b, cx, line, content..line.range.end, inline_base);
        let text = b.finish_hanging(size, false, line.range.start, hang_after);
        return LineView {
            height,
            x: Px(indent + em * LIST_MARKER_X),
            text,
            decor,
            ..LineView::default()
        };
    }

    let x = match &item.task {
        Some(task) => {
            decor.push(Decor::Checkbox {
                x: Px(indent + em * CHECKBOX_X),
                checked: task.is_checked(),
                source: task.range.clone(),
            });
            indent + em * TASK_TEXT_X
        }
        None => {
            decor.push(Decor::Bullet {
                center: Px(indent + em * BULLET_CENTER_X),
            });
            indent + em * BULLET_TEXT_X
        }
    };
    push_spans(&mut b, cx, line, content..line.range.end, inline_base);
    LineView {
        height,
        x: Px(x),
        text: b.finish(size, false, content),
        decor,
        ..LineView::default()
    }
}

fn quote_line(cx: &ViewContext<'_>, ix: usize) -> LineView {
    let line = &cx.doc.lines[ix];
    let quote = line.quote.as_ref().expect("quote_line gets quoted lines");
    let em = cx.em;
    let size = Px(em);
    let base = InlineBase::body(em);
    let mut b = Builder::new(cx.text);

    // the `>` keeps its place either way, drawn only with the caret on the
    // line: the bar stands in for it, and the text never shifts
    let touched = cx.reveal.touches(&line.range);
    let prefix_ink = if touched { Ink::Faint } else { Ink::Clear };
    b.source(
        quote.range.clone(),
        RunStyle::plain(prefix_ink),
        size,
        None,
        Px(0.),
    );
    let hang_after = b.flat;
    let decor: Vec<Decor> = cx.text[quote.range.clone()]
        .match_indices('>')
        .map(|(offset, _)| Decor::QuoteBar { offset })
        .collect();

    let mut from = quote.range.end;
    // a revealed callout header: its `[!type]` stays quiet
    if let Some(block) = line.block.map(|b| &cx.doc.blocks[b])
        && let LiveBlockKind::Callout { marker, .. } = &block.kind
        && block.lines.start == ix
    {
        b.source(
            from..marker.end,
            RunStyle::plain(Ink::Faint),
            size,
            None,
            Px(0.),
        );
        from = marker.end;
    }

    match &line.kind {
        LiveLineKind::List(item) if !cx.reveal.in_marker(&(item.marker.start..item.marker.end)) => {
            // a bullet inside a quote stays in the text flow
            let bullet = if item.number.is_some() {
                None
            } else {
                Some("• ")
            };
            let content = item.content_start().min(line.range.end);
            match bullet {
                Some(glyph) => {
                    b.synthetic(glyph, item.marker.end, RunStyle::plain(Ink::Faint), size)
                }
                None => b.source(
                    item.marker.clone(),
                    RunStyle::plain(Ink::Faint),
                    size,
                    None,
                    Px(0.),
                ),
            }
            push_spans(&mut b, cx, line, content..line.range.end, base);
        }
        LiveLineKind::Heading { level, marker } => {
            let index = usize::from((*level).clamp(1, 6) - 1);
            let heading = InlineBase {
                style: RunStyle {
                    weight: HEADING_WEIGHTS[index],
                    ..RunStyle::plain(Ink::Text)
                },
                size: Px(em * HEADING_SIZES[index]),
            };
            if touched {
                b.source(
                    marker.clone(),
                    RunStyle::plain(Ink::Faint),
                    heading.size,
                    None,
                    Px(0.),
                );
            }
            push_spans(&mut b, cx, line, marker.end..line.range.end, heading);
        }
        _ => push_spans(&mut b, cx, line, from..line.range.end, base),
    }

    let text = b.finish_hanging(size, false, line.range.start, hang_after);
    LineView {
        height: body_height(em),
        text,
        decor,
        ..LineView::default()
    }
}

fn footnote_line(
    cx: &ViewContext<'_>,
    line: &LiveLine,
    label: &Range<usize>,
    marker: &Range<usize>,
) -> LineView {
    let size = Px(cx.em * CODE_SCALE);
    let base = InlineBase {
        style: RunStyle::plain(Ink::Text),
        size,
    };
    let mut b = Builder::new(cx.text);
    let label_size = Px(size.0 * FOOTNOTE_REF_SCALE);
    if cx.reveal.in_marker(marker) {
        let faint = RunStyle::plain(Ink::Faint);
        let muted = RunStyle::plain(Ink::Muted);
        b.source(marker.start..label.start, faint, label_size, None, Px(0.));
        b.source(label.clone(), muted, label_size, None, Px(0.));
        b.source(label.end..marker.end, faint, label_size, None, Px(0.));
    } else {
        b.source(
            label.clone(),
            RunStyle::plain(Ink::Muted),
            label_size,
            None,
            Px(size.0 * FOOTNOTE_REF_RAISE),
        );
        b.synthetic(" ", marker.end, RunStyle::plain(Ink::Text), size);
    }
    push_spans(&mut b, cx, line, marker.end..line.range.end, base);
    LineView {
        height: Height {
            line: Px(size.0 * LINE_HEIGHT),
            ..Height::default()
        },
        text: b.finish(size, false, line.range.start),
        ..LineView::default()
    }
}

fn rule_line(cx: &ViewContext<'_>, line: &LiveLine) -> LineView {
    if cx.reveal.touches(&line.range) {
        return source_line(cx, line, Ink::Muted);
    }
    LineView {
        height: body_height(cx.em),
        text: empty_box(Px(cx.em), false, line.range.start),
        decor: vec![Decor::Rule],
        ..LineView::default()
    }
}

fn code_height(em: f32) -> Height {
    Height {
        line: Px(em * CODE_SCALE * LINE_HEIGHT),
        ..Height::default()
    }
}

/// Each leading tab stop of a code line draws an indent guide.
fn code_guides(em: f32, body: &str) -> Vec<Decor> {
    let columns = body
        .bytes()
        .take_while(|b| matches!(b, b' ' | b'\t'))
        .fold(0, |column, byte| match byte {
            b'\t' => (column / 4 + 1) * 4,
            _ => column + 1,
        });
    // one mono cell of the code size, in em: DejaVu and friends run 0.6
    let cell = em * CODE_SCALE * 0.6;
    (0..columns / 4)
        .map(|k| Decor::Guide {
            x: Px(em * CODE_PAD + cell * (k * 4) as f32),
        })
        .collect()
}

fn code_line(
    cx: &ViewContext<'_>,
    ix: usize,
    block_ix: usize,
    block: &LiveBlock,
    lang: Option<&str>,
) -> LineView {
    let line = &cx.doc.lines[ix];
    let em = cx.em;
    let touched = cx.reveal.touches(&block.range);
    let diagram = if lang == Some(MERMAID) && !touched {
        (cx.diagrams)(block_ix)
    } else {
        DiagramStatus::None
    };
    if let DiagramStatus::Ready { key, width, height } = diagram {
        return diagram_line(cx, ix, block_ix, block, key, Px(width), Px(height));
    }
    let size = Px(em * CODE_SCALE);
    let first = ix == block.lines.start;
    let last = ix + 1 == block.lines.end;
    let fence = line.kind == LiveLineKind::Fence;
    let mut decor = vec![Decor::Band {
        band: Band::Code,
        first,
        last,
    }];
    let mut b = Builder::new(cx.text);
    let mono = RunStyle {
        mono: true,
        ..RunStyle::plain(Ink::Text)
    };
    if fence {
        if touched {
            let faint = RunStyle {
                ink: Ink::Faint,
                ..mono
            };
            b.source(line.range.clone(), faint, size, None, Px(0.));
        } else if first && let Some(lang) = lang {
            decor.push(Decor::Label(capitalized(lang)));
        } else if let DiagramStatus::Failed(message) = diagram {
            // why the source shows instead of its diagram
            let error = RunStyle {
                ink: Ink::Tone(Tone::Red),
                ..mono
            };
            b.synthetic(&message, line.range.start, error, size);
        }
    } else {
        decor.extend(code_guides(em, &cx.text[line.range.clone()]));
        let tokens = line.block.and_then(|block| (cx.tokens)(block));
        let tokens = tokens.as_deref().map_or(&[][..], Vec::as_slice);
        let mut cursor = line.range.start;
        for token in tokens {
            let start = token.range.start.max(cursor);
            let end = token.range.end.min(line.range.end);
            if start >= end {
                continue;
            }
            b.source(cursor..start, mono, size, None, Px(0.));
            let style = RunStyle {
                ink: Ink::Rgba(token.rgba),
                italic: token.italic,
                weight: token.weight,
                ..mono
            };
            b.source(start..end, style, size, None, Px(0.));
            cursor = end;
        }
        b.source(cursor..line.range.end, mono, size, None, Px(0.));
    }
    LineView {
        height: code_height(em),
        x: Px(em * CODE_PAD),
        right: Px(em * CODE_PAD),
        text: b.finish(size, true, line.range.start),
        decor,
        ..LineView::default()
    }
}

/// A parked Mermaid block: the picture on its opening fence's line, and no
/// other line taking any room.
fn diagram_line(
    cx: &ViewContext<'_>,
    ix: usize,
    block_ix: usize,
    block: &LiveBlock,
    key: String,
    width: Px,
    height: Px,
) -> LineView {
    let line = &cx.doc.lines[ix];
    let text = empty_box(Px(cx.em), false, line.range.start);
    if ix != block.lines.start {
        return LineView {
            hidden: true,
            text,
            ..LineView::default()
        };
    }
    LineView {
        height: Height {
            bottom: Px(cx.em * DIAGRAM_GAP),
            ..Height::default()
        },
        text,
        image: Some(ImageView { key, width, height }),
        diagram: Some(block_ix),
        ..LineView::default()
    }
}

fn indented_code_line(cx: &ViewContext<'_>, line: &LiveLine) -> LineView {
    let body = &cx.text[line.range.clone()];
    let ws = body.len() - body.trim_start_matches([' ', '\t']).len();
    let mut b = Builder::new(cx.text);
    let mono = RunStyle {
        mono: true,
        ..RunStyle::plain(Ink::Text)
    };
    let (style, size) = code_style(mono, Px(cx.em));
    b.source(
        line.range.start..line.range.start + ws,
        RunStyle::plain(Ink::Clear),
        Px(cx.em),
        None,
        Px(0.),
    );
    b.close();
    b.source(
        line.range.start + ws..line.range.end,
        style,
        size,
        Some(Pill::Code),
        Px(0.),
    );
    LineView {
        height: body_height(cx.em),
        text: b.finish(Px(cx.em), false, line.range.start),
        decor: vec![Decor::Guide {
            x: Px(cx.em * GUIDE_X),
        }],
        ..LineView::default()
    }
}

fn capitalized(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// A callout type's color and icon, Obsidian's type table in the app's hues.
pub fn callout_style(callout: &str) -> (Tone, Glyph) {
    match callout {
        "abstract" | "summary" | "tldr" => (Tone::Accent, Glyph::ClipboardList),
        "info" => (Tone::Blue, Glyph::Info),
        "todo" => (Tone::Blue, Glyph::CircleCheck),
        "tip" | "hint" | "important" => (Tone::Accent, Glyph::Flame),
        "success" | "check" | "done" => (Tone::Green, Glyph::Check),
        "question" | "help" | "faq" => (Tone::Amber, Glyph::CircleHelp),
        "warning" | "caution" | "attention" => (Tone::Orange, Glyph::TriangleAlert),
        "failure" | "fail" | "missing" => (Tone::Red, Glyph::X),
        "danger" | "error" => (Tone::Red, Glyph::Zap),
        "bug" => (Tone::Red, Glyph::Bug),
        "example" => (Tone::Purple, Glyph::List),
        "quote" | "cite" => (Tone::Gray, Glyph::Quote),
        _ => (Tone::Blue, Glyph::Pencil),
    }
}

/// A parked callout: the source lines become the rendered box's rows.
fn callout_line(cx: &ViewContext<'_>, ix: usize, block: &LiveBlock) -> LineView {
    let LiveBlockKind::Callout {
        callout,
        marker,
        title,
    } = &block.kind
    else {
        unreachable!("callout_line gets callout blocks");
    };
    let line = &cx.doc.lines[ix];
    let em = cx.em;
    let (tone, glyph) = callout_style(callout);
    let first = ix == block.lines.start;
    let last = ix + 1 == block.lines.end;
    let band = Decor::Band {
        band: Band::Callout(tone),
        first,
        last,
    };
    let x = Px(em * CALLOUT_PAD_LEFT);
    let right = Px(em * CALLOUT_PAD);
    let pad = em * CALLOUT_PAD;

    if first {
        let size = Px(em);
        let style = RunStyle {
            ink: Ink::Tone(tone),
            weight: BOLD_WEIGHT,
            ..RunStyle::plain(Ink::Tone(tone))
        };
        let base = InlineBase { style, size };
        let mut b = Builder::new(cx.text);
        match title {
            Some(title) => push_spans(&mut b, cx, line, title.clone(), base),
            None => b.synthetic(&capitalized(callout), marker.end, style, size),
        }
        return LineView {
            height: Height {
                line: Px(em * CALLOUT_TITLE_LINE_HEIGHT),
                top: Px(pad),
                bottom: Px(if last { pad } else { 0. }),
            },
            x: Px(x.0 + em * CALLOUT_ICON_GAP),
            right,
            text: b.finish(size, false, marker.end),
            decor: vec![band, Decor::Icon { glyph, tone, x }],
            ..LineView::default()
        };
    }

    let quote_end = line
        .quote
        .as_ref()
        .map_or(line.range.start, |quote| quote.range.end);
    let base = InlineBase::body(em);
    let mut b = Builder::new(cx.text);
    push_spans(&mut b, cx, line, quote_end..line.range.end, base);
    let gap = em * PARAGRAPH_GAP;
    let is_blank = quote_end >= line.range.end;
    LineView {
        height: Height {
            line: Px(if is_blank { gap } else { em * LINE_HEIGHT }),
            top: Px(if ix == block.lines.start + 1 { gap } else { 0. }),
            bottom: Px(if last { gap + pad } else { 0. }),
        },
        x,
        right,
        text: b.finish(Px(em), false, quote_end),
        decor: vec![band],
        ..LineView::default()
    }
}

fn table_line(cx: &ViewContext<'_>, ix: usize, block_ix: usize, block: &LiveBlock) -> LineView {
    let LiveBlockKind::Table {
        columns,
        rows,
        delimiter,
    } = &block.kind
    else {
        unreachable!("table_line gets table blocks");
    };
    let line = &cx.doc.lines[ix];
    let em = cx.em;
    if ix == *delimiter {
        return LineView {
            hidden: true,
            text: empty_box(Px(em), false, line.range.start),
            ..LineView::default()
        };
    }
    let row = rows
        .iter()
        .find(|row| row.line == ix)
        .expect("every table line but the delimiter is a row");
    let header = ix == block.lines.start;
    let last = ix + 1 == block.lines.end;
    let base = InlineBase {
        style: RunStyle {
            weight: if header { BOLD_WEIGHT } else { 400 },
            ..RunStyle::plain(Ink::Text)
        },
        size: Px(em),
    };
    let cells: Vec<CellView> = (0..columns.len())
        .map(|column| {
            let range = row
                .cells
                .get(column)
                .cloned()
                .unwrap_or(line.range.end..line.range.end);
            let mut b = Builder::new(cx.text);
            push_spans(&mut b, cx, line, range.clone(), base);
            CellView {
                text: b.finish(Px(em), false, range.start),
                align: columns[column],
                source: range,
            }
        })
        .collect();
    let line_height = em * TABLE_LINE_HEIGHT;
    let pad = em * TABLE_CELL_PAD_Y;
    let margin = em * TABLE_MARGIN;
    LineView {
        height: Height {
            line: Px(line_height + pad * 2.),
            top: Px(if header { margin } else { 0. }),
            bottom: Px(if last { margin } else { 0. }),
        },
        text: empty_box(
            Px(em),
            false,
            row.cells.first().map_or(line.range.start, |c| c.start),
        ),
        table: Some(TableRowView {
            block: block_ix,
            cells,
            header,
            first: header,
            last,
        }),
        ..LineView::default()
    }
}

/// The frontmatter is a panel, never YAML: a heading on the opening fence,
/// a row per property, the Add property button on the closing fence. List
/// items show in their property's row, their own lines taking no room.
fn frontmatter_line(cx: &ViewContext<'_>, ix: usize, block: &LiveBlock) -> LineView {
    let LiveBlockKind::Frontmatter { properties } = &block.kind else {
        unreachable!("frontmatter_line gets frontmatter blocks");
    };
    let line = &cx.doc.lines[ix];
    if ix == block.lines.start {
        return properties_heading(cx, line);
    }
    if ix + 1 == block.lines.end {
        return add_property_line(cx, line);
    }
    if let Some(property) = properties.iter().find(|p| p.line == ix) {
        return property_line(cx, line, property);
    }
    let host = properties
        .iter()
        .find(|p| p.items.iter().any(|item| item.line == ix));
    LineView {
        hidden: true,
        host: host.map(|p| p.line),
        text: empty_box(Px(cx.em), false, line.range.start),
        ..LineView::default()
    }
}

fn properties_heading(cx: &ViewContext<'_>, line: &LiveLine) -> LineView {
    let em = cx.em;
    let size = Px(em);
    let style = RunStyle {
        weight: PROPERTY_HEADING_WEIGHT,
        ..RunStyle::plain(Ink::Text)
    };
    let mut b = Builder::new(cx.text);
    b.synthetic("Properties", line.range.start, style, size);
    LineView {
        height: Height {
            line: Px(em * PROPERTY_HEADING_LINE),
            top: Px(em * PROPERTY_PANEL_PAD),
            bottom: Px(em * PROPERTY_PANEL_PAD),
        },
        text: b.finish(size, false, line.range.start),
        role: LineRole::Inert,
        ..LineView::default()
    }
}

fn add_property_line(cx: &ViewContext<'_>, line: &LiveLine) -> LineView {
    let em = cx.em;
    let size = Px(em * PROPERTY_SIZE);
    let mut b = Builder::new(cx.text);
    let muted = RunStyle::plain(Ink::Muted);
    b.synthetic("Add property", line.range.end, muted, size);
    LineView {
        height: Height {
            line: Px(em * PROPERTY_ROW_HEIGHT),
            top: Px(em * PROPERTY_ADD_TOP),
            bottom: Px(em * PROPERTY_ADD_BOTTOM),
        },
        x: Px(em * PROPERTY_ADD_TEXT_X),
        text: b.finish(size, false, line.range.end),
        decor: vec![Decor::Icon {
            glyph: Glyph::Plus,
            tone: Tone::Gray,
            x: Px(em * PROPERTY_ADD_ICON_X),
        }],
        role: LineRole::AddProperty,
        ..LineView::default()
    }
}

/// A property type's icon, Obsidian's choices.
fn property_glyph(kind: PropertyType) -> Glyph {
    match kind {
        PropertyType::Text => Glyph::Text,
        PropertyType::Number => Glyph::Binary,
        PropertyType::Checkbox => Glyph::SquareCheck,
        PropertyType::Date => Glyph::Calendar,
        PropertyType::DateTime => Glyph::Clock,
        PropertyType::List => Glyph::List,
        PropertyType::Tags => Glyph::Tags,
        PropertyType::Aliases => Glyph::Forward,
    }
}

fn property_line(cx: &ViewContext<'_>, line: &LiveLine, property: &Property) -> LineView {
    let em = cx.em;
    let size = Px(em * PROPERTY_SIZE);
    let faint = RunStyle::plain(Ink::Faint);
    let mut decor = Vec::new();
    let mut key = Builder::new(cx.text);
    if !property.bare {
        decor.push(Decor::Icon {
            glyph: property_glyph(property.kind),
            tone: Tone::Gray,
            x: Px(em * PROPERTY_ICON_X),
        });
        if property.key.is_empty() {
            key.synthetic("Property name", property.key.start, faint, size);
        } else {
            let muted = RunStyle::plain(Ink::Muted);
            key.source(property.key.clone(), muted, size, None, Px(0.));
        }
    }
    let mut value = Builder::new(cx.text);
    let checkbox = property.kind == PropertyType::Checkbox;
    if checkbox {
        decor.push(Decor::Checkbox {
            x: Px(property_value_x(em)),
            checked: cx.text[property.value.clone()] == *"true",
            source: property.value.clone(),
        });
    } else if !property.items.is_empty() {
        for item in &property.items {
            push_item(&mut value, item.text.clone(), property.kind, size);
        }
    } else if property.value.is_empty() {
        value.synthetic("Empty", property.value.start, faint, size);
    } else if property.is_list() {
        push_item(&mut value, property.value.clone(), property.kind, size);
    } else {
        let ink = if property.bare { Ink::Muted } else { Ink::Text };
        let style = RunStyle::plain(ink);
        value.source(property.value.clone(), style, size, None, Px(0.));
    }
    let value_source = match (property.items.first(), property.items.last()) {
        (Some(first), Some(last)) => first.text.start..last.text.end,
        _ => property.value.clone(),
    };
    LineView {
        height: Height {
            line: Px(em * PROPERTY_ROW_HEIGHT),
            top: Px(0.),
            bottom: Px(em * PROPERTY_ROW_GAP),
        },
        text: empty_box(Px(em), false, line.range.start),
        decor,
        property: Some(PropertyView {
            key: key.finish(size, false, property.key.start),
            value: value.finish(size, false, value_source.start),
            key_source: property.key.clone(),
            value_source,
            list: property.is_list(),
        }),
        ..LineView::default()
    }
}

/// One item of a list property: a tag's pill, or plain text, each with its
/// remove cross. An item still empty shows nothing but the caret.
fn push_item(b: &mut Builder<'_>, range: Range<usize>, kind: PropertyType, size: Px) {
    if range.is_empty() {
        return;
    }
    let (ink, pill) = if kind == PropertyType::Tags {
        (Ink::Link, Some(Pill::Item))
    } else {
        (Ink::Text, None)
    };
    let end = range.end;
    b.close();
    b.source(range, RunStyle::plain(ink), size, pill, Px(0.));
    b.removable();
    b.synthetic(" ", end, RunStyle::plain(Ink::Text), size);
}

/// Column of a property row's value, in px.
pub fn property_value_x(em: f32) -> f32 {
    em * PROPERTY_VALUE_X
}

/// Column of a property row's key, in px.
pub fn property_key_x(em: f32) -> f32 {
    em * PROPERTY_KEY_X
}

/// Height of a line of property text, in px.
pub fn property_text_line(em: f32) -> f32 {
    em * PROPERTY_TEXT_LINE
}

/// Text inset of a table cell, in px.
pub fn table_cell_pad_x(em: f32) -> f32 {
    em * 0.5
}

/// Scale of a pill's padding: (x, y) of inline code then tags, em of the pill.
pub fn pill_padding(pill: Pill) -> (f32, f32) {
    match pill {
        Pill::Code => (0.3, 0.15),
        Pill::Tag => (0.65, 0.25),
        Pill::Item => (0.6, 0.15),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live::parse_document;

    /// The display text of `line` with the caret at `caret`: `None` parks
    /// everything, as an unfocused editor does.
    fn shown(text: &str, line: usize, selection: Option<Range<usize>>) -> String {
        let doc = parse_document(text);
        let cx = ViewContext {
            text,
            doc: &doc,
            reveal: Reveal { selection },
            em: 16.,
            tokens: &|_| None,
            notes: None,
            images: &|_| ImageStatus::Missing,
            diagrams: &|_| DiagramStatus::None,
        };
        let view = build_view(&cx, line);
        if let Some(table) = &view.table {
            return table
                .cells
                .iter()
                .map(|cell| cell.text.display_text())
                .collect::<Vec<_>>()
                .join("|");
        }
        view.text.display_text()
    }

    fn at(text: &str, line: usize, caret: usize) -> String {
        shown(text, line, Some(caret..caret))
    }

    /// Asserts the display text at every caret position of a one-line doc,
    /// from a table of (first caret, expected).
    fn sweep(text: &str, table: &[(usize, &str)]) {
        for caret in 0..=text.len() {
            if !text.is_char_boundary(caret) {
                continue;
            }
            let (_, want) = table
                .iter()
                .rev()
                .find(|(from, _)| *from <= caret)
                .expect("the table starts at caret 0");
            assert_eq!(&at(text, 0, caret), want, "caret {caret} in {text:?}");
        }
    }

    #[test]
    fn unfocused_parks_everything() {
        let text = "# Title\n**bold** `c`\n- item";
        assert_eq!(shown(text, 0, None), "Title");
        assert_eq!(shown(text, 1, None), "bold c");
        assert_eq!(shown(text, 2, None), "item");
    }

    #[test]
    fn heading_markers_reveal_for_the_whole_line() {
        sweep("# Heading one", &[(0, "# Heading one")]);
        assert_eq!(at("# Heading\n\nx", 0, 11), "Heading");
    }

    #[test]
    fn emphasis_reveals_per_element_with_inclusive_ends() {
        // the positions Obsidian switched at, from a caret sweep
        let text = "x **bold**, *italic*, ***both***, ~~gone~~, ==mark== `code` y";
        sweep(
            text,
            &[
                (0, "x bold, italic, both, gone, mark code y"),
                (2, "x **bold**, italic, both, gone, mark code y"),
                (11, "x bold, italic, both, gone, mark code y"),
                (12, "x bold, *italic*, both, gone, mark code y"),
                (21, "x bold, italic, both, gone, mark code y"),
                (22, "x bold, italic, ***both***, gone, mark code y"),
                (33, "x bold, italic, both, gone, mark code y"),
                (34, "x bold, italic, both, ~~gone~~, mark code y"),
                (43, "x bold, italic, both, gone, mark code y"),
                (44, "x bold, italic, both, gone, ==mark== code y"),
                (53, "x bold, italic, both, gone, mark `code` y"),
                (60, "x bold, italic, both, gone, mark code y"),
            ],
        );
    }

    #[test]
    fn links_reveal_whole_and_bare_urls_stay() {
        sweep(
            "a [link](https://e.co) b",
            &[
                (0, "a link b"),
                (2, "a [link](https://e.co) b"),
                (23, "a link b"),
            ],
        );
        sweep(
            "[[wiki]] [[t|alias]] x",
            &[
                (0, "[[wiki]] alias x"),
                (9, "wiki [[t|alias]] x"),
                (21, "wiki alias x"),
            ],
        );
        sweep("see https://e.co/x now", &[(0, "see https://e.co/x now")]);
        sweep("a #tag b", &[(0, "a #tag b")]);
    }

    #[test]
    fn selections_reveal_everything_they_touch() {
        let text = "**a**, *b*, ***c***";
        assert_eq!(shown(text, 0, Some(3..8)), "**a**, *b*, c");
        let text = "# H\nx **y**\n## I";
        assert_eq!(shown(text, 0, Some(0..text.len())), "# H");
        assert_eq!(shown(text, 1, Some(0..text.len())), "x **y**");
        assert_eq!(shown(text, 2, Some(0..text.len())), "## I");
    }

    #[test]
    fn list_markers_reveal_only_inside_the_marker() {
        // bullets render as a decoration, so the text alone shows when parked
        sweep("- Bullet one", &[(0, "- Bullet one"), (2, "Bullet one")]);
        sweep(
            "\t- Nested",
            &[(0, "Nested"), (1, "- Nested"), (3, "Nested")],
        );
        sweep(
            "- [ ] Open task",
            &[(0, "- [ ] Open task"), (6, "Open task")],
        );
        sweep("- [x] **b**", &[(0, "- [x] b"), (6, "**b**")]);
        // numbers are text in both states
        sweep("1. First", &[(0, "1. First")]);
    }

    #[test]
    fn quotes_keep_their_prefix_as_clear_ink() {
        let text = "> a **b**\nx";
        let doc = parse_document(text);
        let view = |caret| {
            let cx = ViewContext {
                text,
                doc: &doc,
                reveal: Reveal {
                    selection: Some(caret..caret),
                },
                em: 16.,
                tokens: &|_| None,
                notes: None,
                images: &|_| ImageStatus::Missing,
                diagrams: &|_| DiagramStatus::None,
            };
            build_view(&cx, 0)
        };
        let parked = view(text.len());
        assert_eq!(parked.text.display_text(), "> a b");
        assert_eq!(parked.text.frags[0].runs[0].1.ink, Ink::Clear);
        assert_eq!(parked.text.hang_after, 2);
        assert!(matches!(parked.decor[0], Decor::QuoteBar { offset: 0 }));
        let active = view(3);
        assert_eq!(active.text.frags[0].runs[0].1.ink, Ink::Faint);
    }

    #[test]
    fn nested_quotes_draw_a_bar_per_level() {
        let doc = parse_document("> > a");
        let cx = ViewContext {
            text: "> > a",
            doc: &doc,
            reveal: Reveal::default(),
            em: 16.,
            tokens: &|_| None,
            notes: None,
            images: &|_| ImageStatus::Missing,
            diagrams: &|_| DiagramStatus::None,
        };
        let bars = build_view(&cx, 0)
            .decor
            .iter()
            .filter(|d| matches!(d, Decor::QuoteBar { .. }))
            .count();
        assert_eq!(bars, 2);
    }

    #[test]
    fn callouts_stay_rendered_until_the_caret_enters() {
        let text = "x\n\n> [!note] Title\n> Body\n\ny";
        assert_eq!(at(text, 2, 0), "Title");
        assert_eq!(at(text, 3, 0), "Body");
        assert_eq!(at(text, 2, 5), "> [!note] Title");
        assert_eq!(at(text, 3, 5), "> Body");
        assert_eq!(at(text, 3, 22), "> Body");
        // a titleless callout shows its capitalized type
        assert_eq!(at("> [!warning]\n> b\n\nz", 0, 18), "Warning");
    }

    #[test]
    fn code_fences_hide_until_the_caret_is_inside_the_block() {
        let text = "x\n\n```rust\ncode\n```\n\ny";
        let lines = [2, 4];
        for line in lines {
            assert_eq!(at(text, line, 0), "", "caret above, line {line}");
            assert_eq!(at(text, line, text.len()), "", "caret below, line {line}");
        }
        for caret in [3, 11, 16, 18] {
            assert_eq!(at(text, 2, caret), "```rust", "caret {caret}");
            assert_eq!(at(text, 4, caret), "```", "caret {caret}");
        }
        assert_eq!(at(text, 3, 0), "code", "the body is always shown");
    }

    #[test]
    fn code_blocks_label_their_language_when_parked() {
        let text = "```rust\ncode\n```\n\nx";
        let doc = parse_document(text);
        let cx = ViewContext {
            text,
            doc: &doc,
            reveal: Reveal {
                selection: Some(18..18),
            },
            em: 16.,
            tokens: &|_| None,
            notes: None,
            images: &|_| ImageStatus::Missing,
            diagrams: &|_| DiagramStatus::None,
        };
        let view = build_view(&cx, 0);
        assert!(
            view.decor
                .iter()
                .any(|d| matches!(d, Decor::Label(label) if label == "Rust")),
            "{:?}",
            view.decor
        );
    }

    /// Line `line` of a note with one Mermaid block, in `status`.
    fn mermaid_view(line: usize, caret: usize, status: &DiagramStatus) -> LineView {
        let text = "```mermaid\ngraph TD\n```\n\nx";
        let doc = parse_document(text);
        let cx = ViewContext {
            text,
            doc: &doc,
            reveal: Reveal {
                selection: Some(caret..caret),
            },
            em: 16.,
            tokens: &|_| None,
            notes: None,
            images: &|_| ImageStatus::Missing,
            diagrams: &|_| status.clone(),
        };
        build_view(&cx, line)
    }

    #[test]
    fn a_rendered_diagram_replaces_its_block_until_the_caret_enters() {
        let ready = DiagramStatus::Ready {
            key: "k".to_string(),
            width: 100.,
            height: 50.,
        };
        let parked = mermaid_view(0, 25, &ready);
        assert_eq!(parked.image.as_ref().unwrap().key, "k");
        assert_eq!(parked.diagram, Some(0));
        assert!(parked.decor.is_empty(), "no code band behind the picture");
        assert!(mermaid_view(1, 25, &ready).hidden);
        assert!(mermaid_view(2, 25, &ready).hidden);
        // the caret on any of its lines brings the source back
        for caret in [0, 15, 23] {
            let active = mermaid_view(1, caret, &ready);
            assert!(active.image.is_none() && !active.hidden, "caret {caret}");
            assert_eq!(active.text.display_text(), "graph TD");
        }
    }

    #[test]
    fn a_block_waiting_for_its_diagram_stays_code() {
        let view = mermaid_view(0, 25, &DiagramStatus::None);
        assert!(view.image.is_none());
        assert!(
            view.decor
                .iter()
                .any(|d| matches!(d, Decor::Label(label) if label == "Mermaid"))
        );
    }

    #[test]
    fn a_broken_diagram_says_why_under_its_source() {
        let failed = DiagramStatus::Failed("bad arrow".to_string());
        assert_eq!(mermaid_view(1, 25, &failed).text.display_text(), "graph TD");
        assert_eq!(
            mermaid_view(2, 25, &failed).text.display_text(),
            "bad arrow"
        );
        // while editing, the fence is back and the message makes way
        assert_eq!(mermaid_view(2, 15, &failed).text.display_text(), "```");
    }

    #[test]
    fn rules_reveal_with_the_caret_on_their_line() {
        let text = "a\n\n---\n\nb";
        assert_eq!(at(text, 2, 3), "---");
        assert_eq!(at(text, 2, 8), "");
    }

    #[test]
    fn footnote_definitions_reveal_inside_their_marker() {
        sweep(
            "[^1]: The body",
            &[(0, "[^1]: The body"), (6, "1 The body")],
        );
    }

    #[test]
    fn escapes_hide_their_backslash() {
        sweep(r"a \*b c", &[(0, "a *b c"), (2, r"a \*b c"), (5, "a *b c")]);
    }

    #[test]
    fn autolinks_hide_their_brackets() {
        sweep(
            "a <https://e.co> b",
            &[
                (0, "a https://e.co b"),
                (2, "a <https://e.co> b"),
                (17, "a https://e.co b"),
            ],
        );
    }

    #[test]
    fn tables_render_cells_whatever_the_caret_does() {
        let text = "| a | **b** |\n| - | - |\n| c | d |\n\nx";
        assert_eq!(at(text, 0, text.len()), "a|b");
        assert_eq!(at(text, 2, 20), "c|d");
        // the caret in a cell reveals inline syntax of that cell only
        assert_eq!(at(text, 0, 7), "a|**b**");
        let doc = parse_document(text);
        let cx = ViewContext {
            text,
            doc: &doc,
            reveal: Reveal::default(),
            em: 16.,
            tokens: &|_| None,
            notes: None,
            images: &|_| ImageStatus::Missing,
            diagrams: &|_| DiagramStatus::None,
        };
        assert!(
            build_view(&cx, 1).hidden,
            "the delimiter row takes no space"
        );
    }

    /// The view of `line` with every image 100x50.
    fn with_images(text: &str, line: usize, caret: Option<usize>) -> LineView {
        let doc = parse_document(text);
        let cx = ViewContext {
            text,
            doc: &doc,
            reveal: Reveal {
                selection: caret.map(|at| at..at),
            },
            em: 16.,
            tokens: &|_| None,
            notes: None,
            images: &|_| ImageStatus::Ready {
                width: 100.,
                height: 50.,
            },
            diagrams: &|_| DiagramStatus::None,
        };
        build_view(&cx, line)
    }

    #[test]
    fn block_images_replace_their_source_until_the_caret_arrives() {
        let text = "![alt](a.png)\n\nx";
        let parked = with_images(text, 0, Some(text.len()));
        assert_eq!(parked.image.as_ref().unwrap().key, "a.png");
        assert_eq!(parked.text.display_text(), "");
        let active = with_images(text, 0, Some(3));
        assert!(active.image.is_some(), "the image stays while editing");
        assert_eq!(active.text.display_text(), "![alt](a.png)");
    }

    #[test]
    fn images_that_cannot_load_leave_their_source() {
        for text in ["see ![[a.png]] here", "![alt](a.png)", "a ![alt|40](a.png)"] {
            assert_eq!(shown(text, 0, None), text);
        }
    }

    #[test]
    fn images_in_a_sentence_flow_with_it() {
        let text = "see ![[a.png|60]] here\n\nx";
        let parked = with_images(text, 0, Some(text.len()));
        let image = format!("see {IMAGE_CHAR} here");
        assert_eq!(parked.text.display_text(), image);
        let frag = &parked.text.frags[1];
        let got = frag.image.as_ref().unwrap();
        assert_eq!((got.width, got.height), (Px(60.), Px(30.)));
        // the two sides of the image reach both ends of its syntax
        let t = &parked.text;
        assert_eq!(t.source_of(4, Bias::Left), 4);
        assert_eq!(t.source_of(4 + IMAGE_CHAR.len_utf8(), Bias::Left), 17);
        assert_eq!(t.display_of(4), 4);
        assert_eq!(t.display_of(17), 4 + IMAGE_CHAR.len_utf8());
    }

    #[test]
    fn a_touched_image_shows_its_source_before_it() {
        let text = "see ![[a.png|60]] here\n\nx";
        for caret in [4, 10, 17] {
            let view = with_images(text, 0, Some(caret));
            let want = format!("see ![[a.png|60]]{IMAGE_CHAR} here");
            assert_eq!(view.text.display_text(), want, "caret {caret}");
        }
        assert_eq!(
            with_images(text, 0, Some(18)).text.display_text(),
            format!("see {IMAGE_CHAR} here"),
        );
    }

    #[test]
    fn markdown_images_speak_for_all_their_spans() {
        let text = "a ![alt|40](b.png) c\n\nx";
        let view = with_images(text, 0, Some(text.len()));
        assert_eq!(view.text.display_text(), format!("a {IMAGE_CHAR} c"));
    }

    #[test]
    fn images_in_lists_and_quotes_flow_too() {
        let text = "- item ![[a.png]]\n\nx";
        let view = with_images(text, 0, Some(text.len()));
        assert_eq!(view.text.display_text(), format!("item {IMAGE_CHAR}"));
    }

    #[test]
    fn only_labelled_external_links_get_the_arrow() {
        let icon = |text: &str| {
            let doc = parse_document(text);
            let cx = ViewContext {
                text,
                doc: &doc,
                reveal: Reveal::default(),
                em: 16.,
                tokens: &|_| None,
                notes: None,
                images: &|_| ImageStatus::Missing,
                diagrams: &|_| DiagramStatus::None,
            };
            build_view(&cx, 0).text.frags.iter().any(|f| f.link_icon)
        };
        assert!(icon("[a](https://e.co) b"));
        assert!(!icon("[a](local.md) b"));
        assert!(!icon("<https://e.co> b"));
        assert!(!icon("https://e.co b"));
    }

    #[test]
    fn unresolved_wikilinks_are_marked_when_the_host_lists_notes() {
        let text = "[[known]] [[Sub/Known#part|x]] [[missing]]";
        let doc = parse_document(text);
        let notes = std::collections::HashSet::from(["known".to_string()]);
        let cx = ViewContext {
            text,
            doc: &doc,
            reveal: Reveal::default(),
            em: 16.,
            tokens: &|_| None,
            notes: Some(&notes),
            images: &|_| ImageStatus::Missing,
            diagrams: &|_| DiagramStatus::None,
        };
        let view = build_view(&cx, 0);
        let inks: Vec<Ink> = view.text.frags[0].runs.iter().map(|(_, s)| s.ink).collect();
        assert_eq!(
            inks,
            [Ink::Link, Ink::Text, Ink::Link, Ink::Text, Ink::Unresolved]
        );
    }

    /// The views of a frontmatter's lines, with the caret on the last one.
    fn panel(text: &str) -> Vec<LineView> {
        let doc = parse_document(text);
        let cx = ViewContext {
            text,
            doc: &doc,
            reveal: Reveal {
                selection: Some(text.len()..text.len()),
            },
            em: 16.,
            tokens: &|_| None,
            notes: None,
            images: &|_| ImageStatus::Missing,
            diagrams: &|_| DiagramStatus::None,
        };
        (0..doc.lines.len()).map(|ix| build_view(&cx, ix)).collect()
    }

    fn row(view: &LineView) -> (String, String) {
        let property = view.property.as_ref().expect("a property row");
        (
            property.key.display_text(),
            property.value.display_text().trim_end().to_string(),
        )
    }

    fn icon(view: &LineView) -> Option<Glyph> {
        view.decor.iter().find_map(|decor| match decor {
            Decor::Icon { glyph, .. } => Some(*glyph),
            _ => None,
        })
    }

    #[test]
    fn the_frontmatter_is_a_panel_of_rows() {
        let text = "---\ntitle: Note\ncount: 12\n---\nbody";
        let views = panel(text);
        assert_eq!(views[0].text.display_text(), "Properties");
        assert_eq!(views[0].role, LineRole::Inert);
        assert_eq!(row(&views[1]), ("title".to_string(), "Note".to_string()));
        assert_eq!(row(&views[2]), ("count".to_string(), "12".to_string()));
        assert_eq!(views[3].text.display_text(), "Add property");
        assert_eq!(views[3].role, LineRole::AddProperty);
    }

    #[test]
    fn every_property_type_gets_its_icon() {
        let text = "---\nt: x\nn: 1\nc: true\nd: 2026-11-01\ndt: 2026-11-01T10:30\nl:\n  - a\ntags:\n  - b\naliases:\n  - c\n---\n";
        let views = panel(text);
        let icons: Vec<Glyph> = views.iter().filter_map(icon).collect();
        assert_eq!(
            icons,
            [
                Glyph::Text,
                Glyph::Binary,
                Glyph::SquareCheck,
                Glyph::Calendar,
                Glyph::Clock,
                Glyph::List,
                Glyph::Tags,
                Glyph::Forward,
                Glyph::Plus,
            ]
        );
    }

    #[test]
    fn list_items_show_in_their_propertys_row() {
        let text = "---\ntags:\n  - alpha\n  - beta\nother:\n  - x\n---\n";
        let views = panel(text);
        assert_eq!(row(&views[1]).1, "alpha beta");
        let frags = &views[1].property.as_ref().unwrap().value.frags;
        assert_eq!(frags[0].pill, Some(Pill::Item));
        assert!(frags[0].remove);
        assert!(views[2].hidden && views[3].hidden);
        assert_eq!(views[2].host, Some(1));
        assert_eq!(views[5].host, Some(4));
        let other = &views[4].property.as_ref().unwrap().value.frags;
        assert_eq!(other[0].pill, None, "only tags are pills");
        assert!(other[0].remove);
    }

    #[test]
    fn empty_fields_show_placeholders_the_caret_sits_before() {
        let text = "---\nkey:\n: \n---\n";
        let views = panel(text);
        assert_eq!(row(&views[1]), ("key".to_string(), "Empty".to_string()));
        assert_eq!(
            row(&views[2]),
            ("Property name".to_string(), "Empty".to_string())
        );
        let value = &views[1].property.as_ref().unwrap().value;
        assert_eq!(value.display_of(8), 0);
    }

    #[test]
    fn checkbox_properties_draw_a_box_for_their_value() {
        let views = panel("---\ndone: true\n---\n");
        assert_eq!(row(&views[1]).1, "");
        assert!(views[1].decor.iter().any(|decor| matches!(
            decor,
            Decor::Checkbox { checked: true, source, .. } if *source == (10..14)
        )));
    }

    #[test]
    fn yaml_the_panel_cannot_read_stays_visible() {
        let views = panel("---\nmap:\n  nested: 1\n---\n");
        assert_eq!(row(&views[2]), (String::new(), "nested: 1".to_string()));
        assert_eq!(icon(&views[2]), None);
    }

    #[test]
    fn the_panel_never_turns_back_into_yaml() {
        let text = "---\na: 1\n---\nb";
        let doc = parse_document(text);
        for caret in 0..=text.len() {
            let cx = ViewContext {
                text,
                doc: &doc,
                reveal: Reveal {
                    selection: Some(caret..caret),
                },
                em: 16.,
                tokens: &|_| None,
                notes: None,
                images: &|_| ImageStatus::Missing,
                diagrams: &|_| DiagramStatus::None,
            };
            assert!(build_view(&cx, 1).property.is_some(), "caret {caret}");
        }
    }

    #[test]
    fn note_keys_ignore_folders_headings_and_case() {
        assert_eq!(note_key("Folder/My Note.md#Top"), "my note");
        assert_eq!(note_key("plain"), "plain");
    }

    #[test]
    fn display_and_source_offsets_round_trip_around_hidden_text() {
        let text = "a **b** c";
        let doc = parse_document(text);
        let cx = ViewContext {
            text,
            doc: &doc,
            reveal: Reveal::default(),
            em: 16.,
            tokens: &|_| None,
            notes: None,
            images: &|_| ImageStatus::Missing,
            diagrams: &|_| DiagramStatus::None,
        };
        let view = build_view(&cx, 0);
        let t = &view.text;
        assert_eq!(t.display_text(), "a b c");
        assert_eq!(t.display_of(0), 0);
        assert_eq!(t.display_of(5), 3, "after the opening marker");
        // the gap: the left side stops before the markers, the right after
        assert_eq!(t.source_of(2, Bias::Left), 2);
        assert_eq!(t.source_of(2, Bias::Right), 4);
        assert_eq!(t.source_of(3, Bias::Left), 5);
        assert_eq!(t.source_of(3, Bias::Right), 7);
        // a hidden offset resolves to the text after it
        assert_eq!(t.display_of(3), 2);
        assert_eq!(t.display_of(text.len()), 5);
    }

    #[test]
    fn tabs_widen_and_map_as_one_source_byte() {
        let text = "a\tb";
        let doc = parse_document(text);
        let cx = ViewContext {
            text,
            doc: &doc,
            reveal: Reveal::default(),
            em: 16.,
            tokens: &|_| None,
            notes: None,
            images: &|_| ImageStatus::Missing,
            diagrams: &|_| DiagramStatus::None,
        };
        let view = build_view(&cx, 0);
        assert_eq!(view.text.display_text(), "a    b");
        assert_eq!(view.text.display_of(2), 5);
        assert_eq!(view.text.source_of(5, Bias::Left), 2);
        assert_eq!(view.text.source_of(2, Bias::Left), 1);
    }

    #[test]
    fn metrics_follow_the_base_size() {
        let doc = parse_document("# a\n\n## b\n\n### c");
        let cx = ViewContext {
            text: "# a\n\n## b\n\n### c",
            doc: &doc,
            reveal: Reveal::default(),
            em: 16.,
            tokens: &|_| None,
            notes: None,
            images: &|_| ImageStatus::Missing,
            diagrams: &|_| DiagramStatus::None,
        };
        let h1 = build_view(&cx, 0);
        assert!((h1.text.size.0 - 25.888).abs() < 0.01);
        assert_eq!(h1.height.top, Px(16.));
        let h3 = build_view(&cx, 4);
        assert!((h3.text.size.0 - 21.088).abs() < 0.01);
        assert_eq!(h3.height.top, Px(0.));
    }
}
