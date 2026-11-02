//! Pure markdown parser for the live editor — an Obsidian-style live preview:
//! the document is styled in place while editing, and source markers
//! (emphasis/inline-code delimiters, link brackets and URLs) disappear
//! wherever the caret is not touching them.
//!
//! The parser is deliberately line-based and heuristic (a CommonMark subset
//! that covers how notes are actually written, plus the Obsidian flavor:
//! wikilinks, tags, highlights, callouts, footnotes, frontmatter). It only
//! ever splits at ASCII syntax bytes, so byte ranges never land inside a
//! multi-byte character.
//!
//! [`parse_document`] is the full model the editor renders from: one
//! [`LiveLine`] per source line plus the multi-line [`LiveBlock`]s. [`parse`]
//! flattens that into the span list the code-editor highlighter consumes.

use std::ops::Range;

/// Emphasis a span renders with; the flags combine when emphasis nests.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct InlineEmphasis {
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub highlight: bool,
}

impl InlineEmphasis {
    fn merge(self, other: Self) -> Self {
        Self {
            bold: self.bold || other.bold,
            italic: self.italic || other.italic,
            strike: self.strike || other.strike,
            highlight: self.highlight || other.highlight,
        }
    }
}

/// What a span is, and how the editor should paint it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkdownLiveKind {
    /// Unstyled text.
    Plain,
    /// The `#`/`##`… run of an ATX heading.
    HeadingMarker,
    /// Heading text after the marker.
    HeadingText,
    /// Rendered emphasis content; flags combine when emphasis nests.
    Emphasis {
        bold: bool,
        italic: bool,
        strike: bool,
        highlight: bool,
    },
    /// Source syntax that hides when the caret is away: `**`, backticks,
    /// link brackets/parens, escapes.
    Marker,
    /// Inline code content (between the backticks).
    InlineCode,
    /// A fenced-code fence line, including the info string.
    CodeFence,
    /// Fenced code body.
    CodeBlock,
    /// A link label (also image alt text).
    LinkText,
    /// A link URL / autolink / bare URL.
    LinkUrl,
    /// The visible text of a `[[wikilink]]`.
    WikiLink,
    /// A `![[embed]]`.
    Embed,
    /// A `#tag`.
    Tag,
    /// A footnote reference: `[^1]`, or a definition's `[^1]:` label.
    FootnoteRef,
    /// A `%%comment%%`.
    Comment,
    /// The `>` of a blockquote.
    QuoteMarker,
    /// Quoted text (styled base inside a blockquote).
    QuoteText,
    /// A list bullet or ordered marker (`-`, `*`, `+`, `1.`).
    ListMarker,
    /// An open task box `[ ]`.
    TaskOpen,
    /// A completed task box `[x]`.
    TaskDone,
    /// A horizontal rule line.
    HorizontalRule,
}

impl MarkdownLiveKind {
    const fn emphasis(flags: InlineEmphasis) -> Self {
        Self::Emphasis {
            bold: flags.bold,
            italic: flags.italic,
            strike: flags.strike,
            highlight: flags.highlight,
        }
    }

    const fn emphasis_flags(self) -> InlineEmphasis {
        match self {
            Self::Emphasis {
                bold,
                italic,
                strike,
                highlight,
            } => InlineEmphasis {
                bold,
                italic,
                strike,
                highlight,
            },
            _ => InlineEmphasis {
                bold: false,
                italic: false,
                strike: false,
                highlight: false,
            },
        }
    }
}

/// One styled byte range into the parsed document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkdownLiveSpan {
    pub range: Range<usize>,
    pub kind: MarkdownLiveKind,
    /// The emphasis around this span — atomic constructs (code, links) inside
    /// `**…**` carry it even though their kind says what they are.
    pub emphasis: InlineEmphasis,
    /// For hideable syntax: the whole construct it belongs to. The live editor
    /// reveals the span while the selection touches this range, Obsidian's
    /// per-construct rule; `None` never hides.
    pub owner: Option<Range<usize>>,
}

/// A checkbox of a task list item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskBox {
    /// The `[ ]`.
    pub range: Range<usize>,
    /// The ASCII character between the brackets.
    pub mark: char,
}

impl TaskBox {
    /// Any non-space mark renders as a checked box.
    pub fn is_checked(&self) -> bool {
        self.mark != ' '
    }

    /// Only `x` counts as completed; custom marks keep their text unstruck.
    pub fn is_done(&self) -> bool {
        matches!(self.mark, 'x' | 'X')
    }
}

/// A list item line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListItem {
    /// Nesting depth, 0 for a top-level item.
    pub level: u8,
    /// Marker plus its separator: `- ` or `12. `.
    pub marker: Range<usize>,
    /// The number of an ordered item.
    pub number: Option<u32>,
    pub task: Option<TaskBox>,
}

impl ListItem {
    /// Where the item's text starts, past marker and checkbox.
    pub fn content_start(&self) -> usize {
        match &self.task {
            Some(task) => task.range.end + 1,
            None => self.marker.end,
        }
    }
}

/// What a line is, independent of any quote prefix in front of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LiveLineKind {
    Blank,
    Paragraph,
    /// `marker` is the `## ` prefix, empty for a setext heading.
    Heading {
        level: u8,
        marker: Range<usize>,
    },
    /// The `===`/`---` under a setext heading.
    SetextUnderline,
    List(ListItem),
    /// An indented line continuing the list item above.
    ListContinuation {
        level: u8,
    },
    /// `[^1]: text`; `label` is the `1`, `marker` the whole `[^1]: `.
    FootnoteDefinition {
        label: Range<usize>,
        marker: Range<usize>,
    },
    Rule,
    /// The opening or closing line of a fenced code block.
    Fence,
    /// A line inside a fenced code block.
    Code,
    IndentedCode,
    /// One `key: value` (or `---`) line of the frontmatter.
    Frontmatter,
}

/// The `>` run in front of a quoted line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuotePrefix {
    pub depth: u8,
    /// Every `>` plus the whitespace around it.
    pub range: Range<usize>,
}

/// Something clickable or renderable inside a line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InlineObject {
    pub range: Range<usize>,
    pub kind: InlineObjectKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InlineObjectKind {
    /// `[label](url)`, an autolink or a bare URL.
    Link { url: String },
    /// `[[target]]`/`[[target|alias]]`.
    WikiLink { target: String },
    /// `![alt](src)` or `![[src]]`; `width` comes from the `|100` size suffix.
    Image {
        src: String,
        width: Option<u32>,
        height: Option<u32>,
    },
}

/// One source line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveLine {
    /// The line's bytes, without its line ending.
    pub range: Range<usize>,
    pub kind: LiveLineKind,
    pub quote: Option<QuotePrefix>,
    /// Ordered, non-overlapping, covering `range`.
    pub spans: Vec<MarkdownLiveSpan>,
    pub objects: Vec<InlineObject>,
    /// The [`LiveBlock`] this line belongs to.
    pub block: Option<usize>,
}

/// GFM column alignment.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TableAlign {
    #[default]
    Left,
    Center,
    Right,
}

/// One table line and its cells' content ranges.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableRow {
    pub line: usize,
    pub cells: Vec<Range<usize>>,
}

/// What a frontmatter property holds, inferred from its key and value the way
/// Obsidian picks a property's icon and editor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PropertyType {
    Text,
    Number,
    Checkbox,
    Date,
    DateTime,
    List,
    Tags,
    Aliases,
}

/// One item of a list property: its own `- item` line, or a part of `[a, b]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PropertyItem {
    pub line: usize,
    /// The item's text, without the dash or quotes.
    pub text: Range<usize>,
}

/// One top-level `key: value` of the frontmatter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Property {
    pub line: usize,
    /// The key; empty on a line that is not `key: value` YAML, or on a
    /// property being added.
    pub key: Range<usize>,
    pub kind: PropertyType,
    /// A scalar's text without its quotes; empty at the line's end when the
    /// property has no value.
    pub value: Range<usize>,
    pub items: Vec<PropertyItem>,
    /// Not `key: value` YAML at all: `value` is the whole line.
    pub bare: bool,
}

impl Property {
    /// Whether its value is a set of items, even a set of none yet.
    pub fn is_list(&self) -> bool {
        matches!(
            self.kind,
            PropertyType::List | PropertyType::Tags | PropertyType::Aliases
        )
    }

    /// The text ranges the caret may sit in, in document order.
    pub fn fields(&self) -> Vec<Range<usize>> {
        let mut fields = Vec::new();
        if !self.bare {
            fields.push(self.key.clone());
        }
        if !self.items.is_empty() {
            fields.extend(self.items.iter().map(|item| item.text.clone()));
        } else if self.kind != PropertyType::Checkbox {
            fields.push(self.value.clone());
        }
        fields
    }
}

/// What a multi-line block is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LiveBlockKind {
    Code {
        lang: Option<String>,
    },
    Callout {
        /// The lowercased type: `note`, `warning`…
        callout: String,
        /// The `> [!note] ` prefix the rendered title hides.
        marker: Range<usize>,
        /// The explicit title text, when it has one.
        title: Option<Range<usize>>,
    },
    Table {
        columns: Vec<TableAlign>,
        rows: Vec<TableRow>,
        /// The `| --- |` line, which never renders.
        delimiter: usize,
    },
    Frontmatter {
        properties: Vec<Property>,
    },
}

/// A run of lines rendered as one unit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveBlock {
    pub lines: Range<usize>,
    pub range: Range<usize>,
    pub kind: LiveBlockKind,
}

/// The parsed document: exactly one [`LiveLine`] per source line (a trailing
/// newline leaves a final empty line, as it does in an editor).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LiveDocument {
    pub lines: Vec<LiveLine>,
    pub blocks: Vec<LiveBlock>,
}

impl LiveDocument {
    /// The index of the line containing `byte`.
    pub fn line_at(&self, byte: usize) -> usize {
        self.lines
            .partition_point(|line| line.range.start <= byte)
            .saturating_sub(1)
    }
}

/// Parse a markdown document into ordered, non-overlapping spans that cover
/// every byte of `text`. Adjacent same-kind spans (including across newlines,
/// e.g. code-block lines) are merged.
pub fn parse(text: &str) -> Vec<MarkdownLiveSpan> {
    let doc = parse_document(text);
    let mut merged: Vec<MarkdownLiveSpan> = Vec::new();
    let mut push_merged = |span: MarkdownLiveSpan| {
        if span.range.is_empty() {
            return;
        }
        if let Some(last) = merged.last_mut()
            && last.kind == span.kind
            && last.emphasis == span.emphasis
            && last.owner == span.owner
            && last.range.end == span.range.start
        {
            last.range.end = span.range.end;
        } else {
            merged.push(span);
        }
    };
    for (ix, line) in doc.lines.iter().enumerate() {
        for span in &line.spans {
            push_merged(span.clone());
        }
        let next_start = doc.lines.get(ix + 1).map_or(text.len(), |n| n.range.start);
        // the newline belongs to a fenced block so consecutive lines merge
        let newline = match line.kind {
            LiveLineKind::Fence => MarkdownLiveKind::CodeFence,
            LiveLineKind::Code => MarkdownLiveKind::CodeBlock,
            _ => MarkdownLiveKind::Plain,
        };
        push_merged(span(line.range.end..next_start, newline));
    }
    merged
}

/// Parse a markdown document into its line and block model.
pub fn parse_document(text: &str) -> LiveDocument {
    let mut parser = Parser::new(text);
    let mut pos = 0usize;
    for raw in text.split_inclusive('\n') {
        let body = raw.trim_end_matches(['\r', '\n']);
        parser.line(pos..pos + body.len());
        pos += raw.len();
    }
    // the empty line after a final newline, or the only line of an empty text
    if text.is_empty() || text.ends_with('\n') {
        parser.line(text.len()..text.len());
    }
    parser.finish()
}

/// Toggle the task checkbox whose `[ ]`/`[x]` span contains `byte`
/// (either span boundary accepted), returning the full new text. The
/// replacement is 3 bytes for 3 bytes, so every other offset is stable.
pub fn toggle_task_at(text: &str, byte: usize) -> Option<String> {
    let span = parse(text).into_iter().find(|s| {
        matches!(
            s.kind,
            MarkdownLiveKind::TaskOpen | MarkdownLiveKind::TaskDone
        ) && byte >= s.range.start
            && byte <= s.range.end
    })?;
    let replacement = match span.kind {
        MarkdownLiveKind::TaskOpen => "[x]",
        _ => "[ ]",
    };
    Some(format!(
        "{}{}{}",
        &text[..span.range.start],
        replacement,
        &text[span.range.end..]
    ))
}

fn span(range: Range<usize>, kind: MarkdownLiveKind) -> MarkdownLiveSpan {
    MarkdownLiveSpan {
        range,
        kind,
        emphasis: kind.emphasis_flags(),
        owner: None,
    }
}

fn push(spans: &mut Vec<MarkdownLiveSpan>, start: usize, end: usize, kind: MarkdownLiveKind) {
    if start < end {
        spans.push(span(start..end, kind));
    }
}

/// The spans and objects of the line being parsed.
#[derive(Default)]
struct LineOut {
    spans: Vec<MarkdownLiveSpan>,
    objects: Vec<InlineObject>,
}

/// The fence that opened the current code block.
#[derive(Clone, Copy)]
struct OpenFence {
    ch: u8,
    len: usize,
    block: usize,
}

struct Parser<'a> {
    text: &'a str,
    lines: Vec<LiveLine>,
    blocks: Vec<LiveBlock>,
    fence: Option<OpenFence>,
    /// Line index where frontmatter ends, when the document opens with it.
    frontmatter_end: Option<usize>,
    /// Indents of the open list levels, outermost first.
    list_stack: Vec<usize>,
}

impl<'a> Parser<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            text,
            lines: Vec::new(),
            blocks: Vec::new(),
            fence: None,
            frontmatter_end: frontmatter_end(text),
            list_stack: Vec::new(),
        }
    }

    fn line(&mut self, range: Range<usize>) {
        let index = self.lines.len();
        let line = self.classify(index, range);
        self.lines.push(line);
    }

    fn finish(mut self) -> LiveDocument {
        // an unterminated fence runs to the end of the document
        if let Some(fence) = self.fence.take() {
            let last = self.lines.len();
            let block = &mut self.blocks[fence.block];
            block.lines.end = last;
            block.range.end = self.text.len();
        }
        self.group_tables();
        self.group_callouts();
        self.group_properties();
        self.blocks.sort_by_key(|block| block.lines.start);
        for (ix, block) in self.blocks.iter().enumerate() {
            for line in &mut self.lines[block.lines.clone()] {
                line.block = Some(ix);
            }
        }
        LiveDocument {
            lines: self.lines,
            blocks: self.blocks,
        }
    }

    fn classify(&mut self, index: usize, range: Range<usize>) -> LiveLine {
        let text = self.text;
        let body = &text[range.clone()];
        let mut out = LineOut::default();

        if let Some(end) = self.frontmatter_end
            && index <= end
        {
            if index == 0 {
                self.blocks.push(LiveBlock {
                    lines: 0..end + 1,
                    range: 0..0,
                    kind: LiveBlockKind::Frontmatter {
                        properties: Vec::new(),
                    },
                });
            }
            if index == end {
                let block = self
                    .blocks
                    .iter_mut()
                    .find(|b| matches!(b.kind, LiveBlockKind::Frontmatter { .. }))
                    .expect("the frontmatter block opens on line 0");
                block.range.end = range.end;
            }
            let fence = index == 0 || index == end;
            let kind = if fence {
                MarkdownLiveKind::CodeFence
            } else {
                MarkdownLiveKind::CodeBlock
            };
            push(&mut out.spans, range.start, range.end, kind);
            return self.finish_line(range, LiveLineKind::Frontmatter, None, out);
        }

        let indent = body.bytes().take_while(|&b| b == b' ').count();
        let t_off = range.start + if indent <= 3 { indent } else { 0 };
        let t = &text[t_off..range.end];

        // inside a fence: body lines, or the closing fence
        if let Some(open) = self.fence {
            let inner = body.trim_start_matches([' ', '\t']);
            let run = byte_run(inner, 0, open.ch);
            if run >= open.len && inner[run..].trim().is_empty() {
                self.fence = None;
                let block = &mut self.blocks[open.block];
                block.lines.end = index + 1;
                block.range.end = range.end;
                push(
                    &mut out.spans,
                    range.start,
                    range.end,
                    MarkdownLiveKind::CodeFence,
                );
                return self.finish_line(range, LiveLineKind::Fence, None, out);
            }
            push(
                &mut out.spans,
                range.start,
                range.end,
                MarkdownLiveKind::CodeBlock,
            );
            return self.finish_line(range, LiveLineKind::Code, None, out);
        }

        // opening fence: ``` or ~~~ (3+) with an info string allowed; inside
        // a list the fence sits at the item's indent
        let fence_text = if self.list_stack.is_empty() {
            t
        } else {
            body.trim_start_matches([' ', '\t'])
        };
        let fence_run = fence_text
            .strip_prefix("```")
            .map(|_| (b'`', byte_run(fence_text, 0, b'`')))
            .or_else(|| {
                fence_text
                    .strip_prefix("~~~")
                    .map(|_| (b'~', byte_run(fence_text, 0, b'~')))
            });
        if let Some((ch, run)) = fence_run
            && run >= 3
            && (ch == b'~' || !fence_text[run..].contains('`'))
        {
            let info = fence_text[run..].trim();
            let lang = info
                .split_whitespace()
                .next()
                .filter(|lang| !lang.is_empty())
                .map(str::to_string);
            self.blocks.push(LiveBlock {
                lines: index..index + 1,
                range: range.clone(),
                kind: LiveBlockKind::Code { lang },
            });
            self.fence = Some(OpenFence {
                ch,
                len: run,
                block: self.blocks.len() - 1,
            });
            push(
                &mut out.spans,
                range.start,
                range.end,
                MarkdownLiveKind::CodeFence,
            );
            return self.finish_line(range, LiveLineKind::Fence, None, out);
        }

        if body.trim().is_empty() {
            push(
                &mut out.spans,
                range.start,
                range.end,
                MarkdownLiveKind::Plain,
            );
            return self.finish_line(range, LiveLineKind::Blank, None, out);
        }

        // setext underline: `===` or `---` straight under a paragraph
        if is_setext_underline(t)
            && let Some(prev) = self.lines.last()
            && prev.kind == LiveLineKind::Paragraph
            && prev.quote.is_none()
        {
            let level = if t.starts_with('=') { 1 } else { 2 };
            let prev_range = prev.range.clone();
            let mut heading = LineOut::default();
            parse_inlines(
                &text[prev_range.clone()],
                prev_range.start,
                MarkdownLiveKind::HeadingText,
                &mut heading,
            );
            let prev = self.lines.last_mut().expect("just matched");
            prev.kind = LiveLineKind::Heading {
                level,
                marker: prev_range.start..prev_range.start,
            };
            prev.spans = heading.spans;
            prev.objects = heading.objects;
            push(
                &mut out.spans,
                range.start,
                range.end,
                MarkdownLiveKind::Marker,
            );
            return self.finish_line(range, LiveLineKind::SetextUnderline, None, out);
        }

        // horizontal rule: 3+ of -, _ or * with only spaces between
        if is_horizontal_rule(t) {
            self.list_stack.clear();
            push(&mut out.spans, range.start, t_off, MarkdownLiveKind::Plain);
            push(
                &mut out.spans,
                t_off,
                range.end,
                MarkdownLiveKind::HorizontalRule,
            );
            return self.finish_line(range, LiveLineKind::Rule, None, out);
        }

        // blockquote: '>' markers, the rest parsed like an unquoted line
        if t.starts_with('>') {
            self.list_stack.clear();
            let (quote, content) = quote_prefix(text, t_off..range.end, &mut out);
            let start = range.start;
            push_before(&mut out.spans, start, t_off);
            let kind = classify_content(
                text,
                content,
                MarkdownLiveKind::QuoteText,
                &mut Vec::new(),
                &mut out,
            );
            return self.finish_line(range, kind, Some(quote), out);
        }

        // lists keep their nesting across any indent once one is open
        let ws = body.len() - body.trim_start_matches([' ', '\t']).len();
        let list_context = !self.list_stack.is_empty();
        if ws <= 3 || list_context {
            let inner = &body[ws..];
            if list_marker_len(inner).is_some() {
                let mut stack = std::mem::take(&mut self.list_stack);
                let kind = classify_content(
                    text,
                    range.start..range.end,
                    MarkdownLiveKind::Plain,
                    &mut stack,
                    &mut out,
                );
                self.list_stack = stack;
                return self.finish_line(range, kind, None, out);
            }
        }

        // an indented line under a list item continues it
        if list_context && ws >= 2 {
            let level = (self.list_stack.len() - 1) as u8;
            push(
                &mut out.spans,
                range.start,
                range.start + ws,
                MarkdownLiveKind::Plain,
            );
            parse_inlines(
                &body[ws..],
                range.start + ws,
                MarkdownLiveKind::Plain,
                &mut out,
            );
            return self.finish_line(range, LiveLineKind::ListContinuation { level }, None, out);
        }
        self.list_stack.clear();

        // indented code needs a blank line (or more code) above it
        let tabbed = body.starts_with('\t');
        if (ws >= 4 || tabbed)
            && self.lines.last().is_none_or(|prev| {
                matches!(prev.kind, LiveLineKind::Blank | LiveLineKind::IndentedCode)
            })
        {
            push(
                &mut out.spans,
                range.start,
                range.end,
                MarkdownLiveKind::CodeBlock,
            );
            return self.finish_line(range, LiveLineKind::IndentedCode, None, out);
        }

        let kind = classify_content(
            text,
            t_off..range.end,
            MarkdownLiveKind::Plain,
            &mut Vec::new(),
            &mut out,
        );
        push_before(&mut out.spans, range.start, t_off);
        self.finish_line(range, kind, None, out)
    }

    fn finish_line(
        &self,
        range: Range<usize>,
        kind: LiveLineKind,
        quote: Option<QuotePrefix>,
        out: LineOut,
    ) -> LiveLine {
        LiveLine {
            range,
            kind,
            quote,
            spans: out.spans,
            objects: out.objects,
            block: None,
        }
    }

    /// GFM tables: a header line with a matching delimiter line under it,
    /// then rows up to the first line that is not paragraph text.
    fn group_tables(&mut self) {
        let text = self.text;
        let mut ix = 0;
        while ix + 1 < self.lines.len() {
            let plain = |line: &LiveLine| {
                line.kind == LiveLineKind::Paragraph && line.quote.is_none() && line.block.is_none()
            };
            let header = &self.lines[ix];
            let delimiter = &self.lines[ix + 1];
            let columns = (plain(header) && plain(delimiter))
                .then(|| parse_table_delimiter(&text[delimiter.range.clone()]))
                .flatten();
            let header_cells = table_cells(text, header.range.clone());
            let Some(columns) = columns.filter(|c| c.len() == header_cells.len()) else {
                ix += 1;
                continue;
            };
            let first = ix;
            let mut rows = Vec::new();
            let mut row = ix;
            while row < self.lines.len() {
                if row == first + 1 {
                    row += 1;
                    continue;
                }
                let line = &self.lines[row];
                if !plain(line) || line.range.is_empty() {
                    break;
                }
                rows.push(TableRow {
                    line: row,
                    cells: table_cells(text, line.range.clone()),
                });
                row += 1;
            }
            for table_row in &rows {
                let line = &mut self.lines[table_row.line];
                let mut out = LineOut::default();
                let mut cursor = line.range.start;
                for cell in &table_row.cells {
                    push(&mut out.spans, cursor, cell.start, MarkdownLiveKind::Plain);
                    parse_inlines(
                        &text[cell.clone()],
                        cell.start,
                        MarkdownLiveKind::Plain,
                        &mut out,
                    );
                    cursor = cell.end;
                }
                push(
                    &mut out.spans,
                    cursor,
                    line.range.end,
                    MarkdownLiveKind::Plain,
                );
                line.spans = out.spans;
                line.objects = out.objects;
            }
            let lines = first..row;
            let range = self.lines[first].range.start..self.lines[row - 1].range.end;
            self.blocks.push(LiveBlock {
                lines,
                range,
                kind: LiveBlockKind::Table {
                    columns,
                    rows,
                    delimiter: first + 1,
                },
            });
            ix = row;
        }
    }

    /// Reads the frontmatter's YAML lines into properties. Only the simple
    /// shapes get structure: `key: value`, `key: [a, b]` and `- item` lines.
    /// Anything else stays visible as a keyless line rather than vanishing.
    fn group_properties(&mut self) {
        let text = self.text;
        let Some(block) = self
            .blocks
            .iter_mut()
            .find(|b| matches!(b.kind, LiveBlockKind::Frontmatter { .. }))
        else {
            return;
        };
        let mut parsed: Vec<Property> = Vec::new();
        let inner = block.lines.start + 1..block.lines.end.saturating_sub(1);
        for index in inner {
            let range = self.lines[index].range.clone();
            let body = &text[range.clone()];
            let ws = body.len() - body.trim_start_matches([' ', '\t']).len();
            let start = range.start + ws;
            // an item under the key above
            if let Some(after) = body[ws..].strip_prefix("- ")
                && let Some(owner) = parsed.last_mut()
                && owner.value.is_empty()
                && !owner.key.is_empty()
            {
                let lead = after.len() - after.trim_start().len();
                let item = unquoted(text, start + 2 + lead..range.end);
                owner.items.push(PropertyItem {
                    line: index,
                    text: item,
                });
                continue;
            }
            parsed.push(parse_property(text, index, range.clone(), ws));
        }
        for property in &mut parsed {
            property.kind = property_type(&text[property.key.clone()], text, property);
        }
        block.kind = LiveBlockKind::Frontmatter { properties: parsed };
    }

    /// `> [!type] title` turns the quote it opens into a callout.
    fn group_callouts(&mut self) {
        let text = self.text;
        let mut ix = 0;
        while ix < self.lines.len() {
            let line = &self.lines[ix];
            let Some(quote) = line.quote.clone().filter(|q| q.depth == 1) else {
                ix += 1;
                continue;
            };
            let inner = quote.range.end..line.range.end;
            let Some((callout, title)) = parse_callout_header(text, inner) else {
                // skip the whole plain quote so its later lines never open one
                while self.lines.get(ix).is_some_and(|l| l.quote.is_some()) {
                    ix += 1;
                }
                continue;
            };
            let first = ix;
            let marker = line.range.start..title.as_ref().map_or(line.range.end, |t| t.start);
            ix += 1;
            while self.lines.get(ix).is_some_and(|l| l.quote.is_some()) {
                ix += 1;
            }
            let range = self.lines[first].range.start..self.lines[ix - 1].range.end;
            self.blocks.push(LiveBlock {
                lines: first..ix,
                range,
                kind: LiveBlockKind::Callout {
                    callout,
                    marker,
                    title,
                },
            });
        }
    }
}

/// One frontmatter line that is not a list item.
fn parse_property(text: &str, line: usize, range: Range<usize>, ws: usize) -> Property {
    let body = &text[range.clone()];
    let colon = (ws == 0).then(|| yaml_colon(body)).flatten();
    let Some(colon) = colon else {
        // not YAML this panel understands: the whole line as a bare value
        return Property {
            line,
            key: range.start..range.start,
            kind: PropertyType::Text,
            value: range.start + ws..range.end,
            items: Vec::new(),
            bare: true,
        };
    };
    let key = range.start..range.start + colon;
    let after = &body[colon + 1..];
    let lead = after.len() - after.trim_start().len();
    let start = range.start + colon + 1 + lead;
    let raw = after.trim();
    let mut items = Vec::new();
    if raw.len() >= 2 && raw.starts_with('[') && raw.ends_with(']') {
        let mut cursor = start + 1;
        for part in raw[1..raw.len() - 1].split(',') {
            let part_lead = part.len() - part.trim_start().len();
            let trimmed = part.trim();
            if !trimmed.is_empty() {
                let at = cursor + part_lead;
                items.push(PropertyItem {
                    line,
                    text: unquoted(text, at..at + trimmed.len()),
                });
            }
            cursor += part.len() + 1;
        }
        return Property {
            line,
            key,
            kind: PropertyType::List,
            value: range.end..range.end,
            items,
            bare: false,
        };
    }
    let value = if raw.is_empty() {
        range.end..range.end
    } else {
        unquoted(text, start..start + raw.len())
    };
    Property {
        line,
        key,
        kind: PropertyType::Text,
        value,
        items,
        bare: false,
    }
}

/// The colon ending a YAML key: the first one followed by a space or the end
/// of the line, so a URL in a bare line is no key.
fn yaml_colon(body: &str) -> Option<usize> {
    body.char_indices()
        .find(|&(ix, c)| {
            c == ':' && matches!(body.as_bytes().get(ix + 1), None | Some(b' ' | b'\t'))
        })
        .map(|(ix, _)| ix)
}

/// `range` without one pair of matching quotes around it.
fn unquoted(text: &str, range: Range<usize>) -> Range<usize> {
    let body = &text[range.clone()];
    let quoted = body.len() >= 2
        && ((body.starts_with('"') && body.ends_with('"'))
            || (body.starts_with('\'') && body.ends_with('\'')));
    if quoted {
        range.start + 1..range.end - 1
    } else {
        range
    }
}

/// What a property's key and value say it is.
fn property_type(key: &str, text: &str, property: &Property) -> PropertyType {
    match key.to_lowercase().as_str() {
        "tags" | "tag" => return PropertyType::Tags,
        "aliases" | "alias" => return PropertyType::Aliases,
        "cssclasses" | "cssclass" => return PropertyType::List,
        _ => {}
    }
    if !property.items.is_empty() {
        return PropertyType::List;
    }
    let value = &text[property.value.clone()];
    if matches!(value, "true" | "false") {
        PropertyType::Checkbox
    } else if is_date(value) {
        PropertyType::Date
    } else if value.len() == 16 && is_date(&value[..10]) && is_time(&value[10..]) {
        PropertyType::DateTime
    } else if !value.is_empty() && value.parse::<f64>().is_ok_and(f64::is_finite) {
        PropertyType::Number
    } else {
        PropertyType::Text
    }
}

/// `YYYY-MM-DD`.
fn is_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 10
        && bytes.iter().enumerate().all(|(ix, b)| match ix {
            4 | 7 => *b == b'-',
            _ => b.is_ascii_digit(),
        })
}

/// `THH:MM`.
fn is_time(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 6
        && bytes[0] == b'T'
        && bytes[3] == b':'
        && [1, 2, 4, 5].iter().all(|&ix| bytes[ix].is_ascii_digit())
}

/// Plain indentation in front of the spans parsed so far.
fn push_before(spans: &mut Vec<MarkdownLiveSpan>, start: usize, end: usize) {
    if start < end {
        spans.insert(0, span(start..end, MarkdownLiveKind::Plain));
    }
}

/// Consumes the `>` run at the start of `range`, emitting quote markers.
fn quote_prefix(text: &str, range: Range<usize>, out: &mut LineOut) -> (QuotePrefix, Range<usize>) {
    let bytes = text.as_bytes();
    let mut pos = range.start;
    let mut depth = 0u8;
    while pos < range.end && bytes[pos] == b'>' {
        depth = depth.saturating_add(1);
        push(&mut out.spans, pos, pos + 1, MarkdownLiveKind::QuoteMarker);
        pos += 1;
        let spaces = text[pos..range.end]
            .bytes()
            .take_while(|&b| b == b' ')
            .count()
            .min(1);
        push(
            &mut out.spans,
            pos,
            pos + spaces,
            MarkdownLiveKind::QuoteText,
        );
        pos += spaces;
    }
    (
        QuotePrefix {
            depth,
            range: range.start..pos,
        },
        pos..range.end,
    )
}

/// Classifies one line's content (quote prefix already consumed): heading,
/// list item, footnote definition or paragraph, emitting its spans.
fn classify_content(
    text: &str,
    range: Range<usize>,
    base: MarkdownLiveKind,
    list_stack: &mut Vec<usize>,
    out: &mut LineOut,
) -> LiveLineKind {
    let body = &text[range.clone()];
    if body.is_empty() {
        return LiveLineKind::Blank;
    }

    // ATX heading: 1-6 # then space or end of line
    let hashes = body.bytes().take_while(|&b| b == b'#').count();
    if (1..=6).contains(&hashes) && matches!(body.as_bytes().get(hashes), None | Some(b' ' | b'\t'))
    {
        let marker_end = range.start + hashes;
        push(
            &mut out.spans,
            range.start,
            marker_end,
            MarkdownLiveKind::HeadingMarker,
        );
        let separator = usize::from(hashes < body.len());
        push(
            &mut out.spans,
            marker_end,
            marker_end + separator,
            MarkdownLiveKind::HeadingText,
        );
        parse_inlines(
            &body[hashes + separator..],
            marker_end + separator,
            MarkdownLiveKind::HeadingText,
            out,
        );
        return LiveLineKind::Heading {
            level: hashes as u8,
            marker: range.start..marker_end + separator,
        };
    }

    // lists: bullet (-, *, +) or ordered (N. / N)) followed by whitespace
    let ws = body.len() - body.trim_start_matches([' ', '\t']).len();
    let t = &body[ws..];
    if let Some(marker_len) = list_marker_len(t) {
        return list_item(text, range, ws, marker_len, base, list_stack, out);
    }

    // footnote definition: [^id]: text
    if let Some((label_len, marker_len)) = footnote_definition(body) {
        push(
            &mut out.spans,
            range.start,
            range.start + marker_len,
            MarkdownLiveKind::FootnoteRef,
        );
        parse_inlines(&body[marker_len..], range.start + marker_len, base, out);
        return LiveLineKind::FootnoteDefinition {
            label: range.start + 2..range.start + 2 + label_len,
            marker: range.start..range.start + marker_len,
        };
    }

    parse_inlines(body, range.start, base, out);
    LiveLineKind::Paragraph
}

fn list_item(
    text: &str,
    range: Range<usize>,
    ws: usize,
    marker_len: usize,
    base: MarkdownLiveKind,
    list_stack: &mut Vec<usize>,
    out: &mut LineOut,
) -> LiveLineKind {
    let body = &text[range.clone()];
    let t = &body[ws..];
    let t_off = range.start + ws;

    // nesting: deeper than the open level pushes, shallower pops back
    let indent_width = indent_columns(&body[..ws]);
    while list_stack.last().is_some_and(|&top| top > indent_width) {
        list_stack.pop();
    }
    if list_stack.last().is_none_or(|&top| top < indent_width) {
        list_stack.push(indent_width);
    }
    let level = list_stack.len().saturating_sub(1) as u8;

    push(&mut out.spans, range.start, t_off, MarkdownLiveKind::Plain);
    push(
        &mut out.spans,
        t_off,
        t_off + marker_len,
        MarkdownLiveKind::ListMarker,
    );
    let after_marker = &t[marker_len..];
    // the one separator space/tab after the marker stays plain
    let sep = usize::from(matches!(
        after_marker.as_bytes().first(),
        Some(b' ') | Some(b'\t')
    ));
    push(
        &mut out.spans,
        t_off + marker_len,
        t_off + marker_len + sep,
        MarkdownLiveKind::Plain,
    );
    let rest_off = t_off + marker_len + sep;
    let rest = &after_marker[sep..];
    let number = t[..marker_len]
        .trim_end_matches(['.', ')'])
        .parse::<u32>()
        .ok();

    // task list item: [ ] or [x] after the marker
    let task = task_box(rest);
    if let Some(mark) = task {
        let kind = if mark == ' ' {
            MarkdownLiveKind::TaskOpen
        } else {
            MarkdownLiveKind::TaskDone
        };
        push(&mut out.spans, rest_off, rest_off + 3, kind);
        let separator = usize::from(rest.len() > 3);
        push(
            &mut out.spans,
            rest_off + 3,
            rest_off + 3 + separator,
            MarkdownLiveKind::Plain,
        );
        let after = &rest[3 + separator..];
        // a done task strikes through its whole text, Obsidian-style
        let base = if matches!(mark, 'x' | 'X') {
            MarkdownLiveKind::emphasis(InlineEmphasis {
                strike: true,
                ..InlineEmphasis::default()
            })
        } else {
            base
        };
        parse_inlines(after, rest_off + 3 + separator, base, out);
    } else {
        parse_inlines(rest, rest_off, base, out);
    }
    LiveLineKind::List(ListItem {
        level,
        marker: t_off..rest_off,
        number,
        task: task.map(|mark| TaskBox {
            range: rest_off..rest_off + 3,
            mark,
        }),
    })
}

/// Visual columns of an indent: a tab advances to the next stop of 4.
fn indent_columns(indent: &str) -> usize {
    indent.bytes().fold(0, |column, byte| match byte {
        b'\t' => (column / 4 + 1) * 4,
        _ => column + 1,
    })
}

/// The checkbox mark at the start of `rest`: `[c]` then whitespace or the end
/// of the line, where `c` is one ASCII character.
fn task_box(rest: &str) -> Option<char> {
    let bytes = rest.as_bytes();
    if bytes.len() < 3 || bytes[0] != b'[' || bytes[2] != b']' {
        return None;
    }
    let mark = bytes[1];
    let closes = matches!(bytes.get(3), None | Some(b' ' | b'\t'));
    (closes && mark.is_ascii() && mark != b']' && mark != b'[').then_some(mark as char)
}

/// `[^id]:` at the start of a line: (id length, marker length incl. space).
fn footnote_definition(body: &str) -> Option<(usize, usize)> {
    let rest = body.strip_prefix("[^")?;
    let close = rest.find(']')?;
    if close == 0 || rest[..close].contains(char::is_whitespace) {
        return None;
    }
    let after = rest[close + 1..].strip_prefix(':')?;
    let space = usize::from(after.starts_with(' '));
    Some((close, 2 + close + 2 + space))
}

/// `[!type]` with an optional fold sign and title, in a quote's first line.
fn parse_callout_header(text: &str, inner: Range<usize>) -> Option<(String, Option<Range<usize>>)> {
    let body = &text[inner.clone()];
    let rest = body.strip_prefix("[!")?;
    let close = rest.find(']')?;
    let name = &rest[..close];
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return None;
    }
    let mut after = 2 + close + 1;
    if matches!(body.as_bytes().get(after), Some(b'+' | b'-')) {
        after += 1;
    }
    let title_text = body[after..].trim();
    let title = (!title_text.is_empty()).then(|| {
        let start = inner.start + after + (body[after..].len() - body[after..].trim_start().len());
        start..start + title_text.len()
    });
    Some((name.to_lowercase(), title))
}

/// The columns of a `| --- | :-: |` delimiter line.
fn parse_table_delimiter(line: &str) -> Option<Vec<TableAlign>> {
    let trimmed = line.trim();
    if !trimmed.contains('-') {
        return None;
    }
    // a delimiter needs a pipe unless it is unmistakably a one-column table
    if !trimmed.contains('|') {
        return None;
    }
    let inner = trimmed.strip_prefix('|').unwrap_or(trimmed);
    let inner = inner.strip_suffix('|').unwrap_or(inner);
    inner
        .split('|')
        .map(|cell| {
            let cell = cell.trim();
            let dashes = cell.trim_start_matches(':').trim_end_matches(':');
            if dashes.is_empty() || !dashes.bytes().all(|b| b == b'-') {
                return None;
            }
            Some(match (cell.starts_with(':'), cell.ends_with(':')) {
                (true, true) => TableAlign::Center,
                (false, true) => TableAlign::Right,
                _ => TableAlign::Left,
            })
        })
        .collect()
}

/// The trimmed content ranges of one table line's cells. A `|` escaped with a
/// backslash or inside a code span does not separate cells.
fn table_cells(text: &str, range: Range<usize>) -> Vec<Range<usize>> {
    let bytes = text.as_bytes();
    let mut separators: Vec<usize> = Vec::new();
    let mut in_code = false;
    let mut i = range.start;
    while i < range.end {
        match bytes[i] {
            b'\\' => i += 1,
            b'`' => in_code = !in_code,
            b'|' if !in_code => separators.push(i),
            _ => {}
        }
        i += 1;
    }
    let body = &text[range.clone()];
    let lead = body.len() - body.trim_start().len();
    let start = range.start + lead;
    let end = range.start + body.trim_end().len();
    let mut cells = Vec::new();
    let mut cell_start = start;
    if separators.first() == Some(&start) {
        separators.remove(0);
        cell_start = start + 1;
    }
    let closes = separators.last().is_some_and(|&last| last + 1 == end);
    let mut bounds: Vec<usize> = separators;
    if !closes {
        bounds.push(end);
    }
    for bound in bounds {
        let raw = &text[cell_start..bound];
        let trimmed = raw.trim();
        // a blank cell keeps its caret after the first space, so typing lands
        // where the text of a formatted cell starts
        let lead = if trimmed.is_empty() {
            raw.len().min(1)
        } else {
            raw.len() - raw.trim_start().len()
        };
        let content = cell_start + lead;
        cells.push(content..content + trimmed.len());
        cell_start = bound + 1;
    }
    cells
}

/// The line index of the frontmatter's closing `---`, when the document opens
/// with one.
fn frontmatter_end(text: &str) -> Option<usize> {
    let mut lines = text.lines();
    if lines.next()?.trim_end() != "---" {
        return None;
    }
    lines
        .position(|line| matches!(line.trim_end(), "---" | "..."))
        .map(|ix| ix + 1)
}

/// `=` (1+) or `-` (2+) alone on the line.
fn is_setext_underline(t: &str) -> bool {
    let trimmed = t.trim_end();
    let Some(first) = trimmed.bytes().next() else {
        return false;
    };
    let run = trimmed.bytes().all(|b| b == first);
    run && match first {
        b'=' => true,
        b'-' => trimmed.len() >= 2,
        _ => false,
    }
}

/// Length of a list marker at the start of `t`, if any (bullet or ordered).
fn list_marker_len(t: &str) -> Option<usize> {
    let bytes = t.as_bytes();
    let after_space = |i: usize| bytes.get(i).is_some_and(|&b| b == b' ' || b == b'\t');
    match bytes.first() {
        Some(b'-' | b'+' | b'*') if after_space(1) => Some(1),
        _ => {
            let digits = bytes.iter().take_while(|b| b.is_ascii_digit()).count();
            if (1..=9).contains(&digits)
                && matches!(bytes.get(digits), Some(b'.') | Some(b')'))
                && after_space(digits + 1)
            {
                Some(digits + 1)
            } else {
                None
            }
        }
    }
}

fn is_horizontal_rule(t: &str) -> bool {
    let Some(first) = t.bytes().next() else {
        return false;
    };
    if !matches!(first, b'-' | b'_' | b'*') {
        return false;
    }
    let mut count = 0;
    for &b in t.as_bytes() {
        match b {
            b if b == first => count += 1,
            b' ' | b'\t' => {}
            _ => return false,
        }
    }
    count >= 3
}

fn byte_run(s: &str, from: usize, ch: u8) -> usize {
    s.as_bytes()[from..]
        .iter()
        .take_while(|&&b| b == ch)
        .count()
}

// ---- inline parsing -----------------------------------------------------------

/// One emphasis pair: marker ranges plus the content between them.
#[derive(PartialEq)]
struct EmphPair {
    open: Range<usize>,
    content: Range<usize>,
    close: Range<usize>,
    flags: InlineEmphasis,
    /// The two delimiter runs and everything between them.
    owner: Range<usize>,
}

/// One relative inline span on its way to the line's span list.
struct Inline {
    range: Range<usize>,
    kind: MarkdownLiveKind,
    emphasis: InlineEmphasis,
    owner: Option<Range<usize>>,
}

impl Inline {
    fn atomic(range: Range<usize>, kind: MarkdownLiveKind) -> Self {
        Self {
            range,
            kind,
            emphasis: InlineEmphasis::default(),
            owner: None,
        }
    }

    fn owned(range: Range<usize>, kind: MarkdownLiveKind, owner: Range<usize>) -> Self {
        Self {
            owner: Some(owner),
            ..Self::atomic(range, kind)
        }
    }
}

/// Parse inline constructs (relative `s` at absolute `offset`) into `out`.
/// `base` is the kind unstyled text takes (Plain, or QuoteText in quotes).
fn parse_inlines(s: &str, offset: usize, base: MarkdownLiveKind, out: &mut LineOut) {
    let bytes = s.as_bytes();
    // Phase A: atomic constructs — code spans, links, escapes, URLs.
    let mut atomic: Vec<Inline> = Vec::new();
    let mut objects: Vec<InlineObject> = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'`' => {
                let run = byte_run(s, i, b'`');
                if let Some(j) = find_backtick_run(s, i + run, run) {
                    let owner = i..j + run;
                    atomic.push(Inline::owned(
                        i..i + run,
                        MarkdownLiveKind::Marker,
                        owner.clone(),
                    ));
                    atomic.push(Inline::atomic(i + run..j, MarkdownLiveKind::InlineCode));
                    atomic.push(Inline::owned(j..j + run, MarkdownLiveKind::Marker, owner));
                    i = j + run;
                } else {
                    i += run;
                }
            }
            b'\\' if bytes.get(i + 1).is_some_and(|b| b.is_ascii_punctuation()) => {
                atomic.push(Inline::owned(i..i + 1, MarkdownLiveKind::Marker, i..i + 2));
                atomic.push(Inline::atomic(i + 1..i + 2, base));
                i += 2;
            }
            b'!' if s[i..].starts_with("![[") => {
                if let Some(end) = s[i + 3..].find("]]").map(|k| i + 3 + k) {
                    let (target, width, height) = split_size(&s[i + 3..end]);
                    let range = i..end + 2;
                    atomic.push(Inline::atomic(range.clone(), MarkdownLiveKind::Embed));
                    objects.push(InlineObject {
                        range: range.start + offset..range.end + offset,
                        kind: InlineObjectKind::Image {
                            src: target.to_string(),
                            width,
                            height,
                        },
                    });
                    i = end + 2;
                } else {
                    i += 1;
                }
            }
            b'[' if s[i..].starts_with("[[") => {
                if let Some(end) = s[i + 2..]
                    .find("]]")
                    .map(|k| i + 2 + k)
                    .filter(|&end| end > i + 2)
                {
                    let inner = i + 2..end;
                    let owner = i..end + 2;
                    let alias = s[inner.clone()].find('|').map(|k| inner.start + k);
                    let (target, shown) = match alias {
                        Some(bar) => (inner.start..bar, bar + 1..end),
                        None => (inner.clone(), inner.clone()),
                    };
                    atomic.push(Inline::owned(
                        i..shown.start,
                        MarkdownLiveKind::Marker,
                        owner.clone(),
                    ));
                    atomic.push(Inline::atomic(shown.clone(), MarkdownLiveKind::WikiLink));
                    atomic.push(Inline::owned(
                        shown.end..end + 2,
                        MarkdownLiveKind::Marker,
                        owner.clone(),
                    ));
                    objects.push(InlineObject {
                        range: owner.start + offset..owner.end + offset,
                        kind: InlineObjectKind::WikiLink {
                            target: s[target].trim().to_string(),
                        },
                    });
                    i = end + 2;
                } else {
                    i += 2;
                }
            }
            b'[' if s[i..].starts_with("[^") => {
                if let Some(end) = s[i..].find(']').map(|k| i + k).filter(|&end| end > i + 2) {
                    atomic.push(Inline::atomic(i..end + 1, MarkdownLiveKind::FootnoteRef));
                    i = end + 1;
                } else {
                    i += 1;
                }
            }
            b'!' | b'[' => {
                if let Some(link) = parse_link(s, i) {
                    let owner = link.marker.start..link.close.end;
                    objects.push(InlineObject {
                        range: owner.start + offset..owner.end + offset,
                        kind: link.object(s),
                    });
                    atomic.push(Inline::owned(
                        link.marker,
                        MarkdownLiveKind::Marker,
                        owner.clone(),
                    ));
                    atomic.push(Inline::atomic(link.label, MarkdownLiveKind::LinkText));
                    atomic.push(Inline::owned(
                        link.mid,
                        MarkdownLiveKind::Marker,
                        owner.clone(),
                    ));
                    atomic.push(Inline::owned(
                        link.url,
                        MarkdownLiveKind::LinkUrl,
                        owner.clone(),
                    ));
                    atomic.push(Inline::owned(
                        link.close.clone(),
                        MarkdownLiveKind::Marker,
                        owner,
                    ));
                    i = link.close.end;
                } else {
                    i += 1;
                }
            }
            b'<' => {
                let end = s[i + 1..].find('>').map(|k| i + 1 + k);
                if let Some(end) = end
                    && s[i..end].contains("://")
                {
                    let owner = i..end + 1;
                    atomic.push(Inline::owned(
                        i..i + 1,
                        MarkdownLiveKind::Marker,
                        owner.clone(),
                    ));
                    atomic.push(Inline::atomic(i + 1..end, MarkdownLiveKind::LinkUrl));
                    atomic.push(Inline::owned(
                        end..end + 1,
                        MarkdownLiveKind::Marker,
                        owner.clone(),
                    ));
                    objects.push(InlineObject {
                        range: owner.start + offset..owner.end + offset,
                        kind: InlineObjectKind::Link {
                            url: s[i + 1..end].to_string(),
                        },
                    });
                    i = end + 1;
                } else {
                    i += 1;
                }
            }
            b':' if s[i..].starts_with("://") && is_bare_url_start(s, i) => {
                let start = bare_url_start(s, i);
                let end = bare_url_end(s, i);
                atomic.push(Inline::atomic(start..end, MarkdownLiveKind::LinkUrl));
                objects.push(InlineObject {
                    range: start + offset..end + offset,
                    kind: InlineObjectKind::Link {
                        url: s[start..end].to_string(),
                    },
                });
                i = end;
            }
            b'w' if s[i..].starts_with("www.")
                && char_before(s, i).is_none_or(|c| !c.is_alphanumeric())
                && s[i + 4..].chars().next().is_some_and(char::is_alphanumeric) =>
            {
                let end = bare_url_end(s, i);
                atomic.push(Inline::atomic(i..end, MarkdownLiveKind::LinkUrl));
                objects.push(InlineObject {
                    range: i + offset..end + offset,
                    kind: InlineObjectKind::Link {
                        url: format!("https://{}", &s[i..end]),
                    },
                });
                i = end;
            }
            b'#' => {
                if let Some(end) = tag_end(s, i) {
                    atomic.push(Inline::atomic(i..end, MarkdownLiveKind::Tag));
                    i = end;
                } else {
                    i += 1;
                }
            }
            b'%' if s[i..].starts_with("%%") => {
                if let Some(end) = s[i + 2..].find("%%").map(|k| i + 2 + k) {
                    atomic.push(Inline::atomic(i..end + 2, MarkdownLiveKind::Comment));
                    i = end + 2;
                } else {
                    i += 2;
                }
            }
            _ => i += 1,
        }
    }

    // Phase B: emphasis over the gaps between the atomic spans. Delimiters
    // pair across the whole line (e.g. bold around an inline-code span), but
    // only ever live in gaps, never inside an atomic span.
    let mut cursor = 0usize;
    let mut gaps: Vec<Range<usize>> = Vec::new();
    for item in &atomic {
        if item.range.start > cursor {
            gaps.push(cursor..item.range.start);
        }
        cursor = item.range.end;
    }
    if cursor < s.len() {
        gaps.push(cursor..s.len());
    }
    let pairs = resolve_emphasis(s, &gaps);
    let mut runs: Vec<Inline> = Vec::new();
    flatten_pairs(
        &pairs,
        0..s.len(),
        base,
        base.emphasis_flags(),
        &gaps,
        &mut runs,
    );

    // atomic spans take the emphasis of every pair around them
    for item in &mut atomic {
        let around = pairs
            .iter()
            .filter(|p| p.content.start <= item.range.start && item.range.end <= p.content.end)
            .fold(base.emphasis_flags(), |flags, p| flags.merge(p.flags));
        item.emphasis = around;
        if item.kind == base && around != InlineEmphasis::default() {
            item.kind = MarkdownLiveKind::emphasis(around);
        }
    }

    let mut covered: Vec<Inline> = atomic;
    covered.extend(runs);
    covered.sort_by_key(|item| item.range.start);
    covered.dedup_by_key(|item| item.range.clone());
    cursor = 0;
    for item in covered {
        if item.range.start > cursor {
            push(
                &mut out.spans,
                offset + cursor,
                offset + item.range.start,
                base,
            );
        }
        if !item.range.is_empty() {
            let end = item.range.end;
            out.spans.push(MarkdownLiveSpan {
                range: offset + item.range.start..offset + end,
                kind: item.kind,
                emphasis: item.emphasis,
                owner: item
                    .owner
                    .map(|owner| offset + owner.start..offset + owner.end),
            });
            cursor = end;
        }
    }
    if cursor < s.len() {
        push(&mut out.spans, offset + cursor, offset + s.len(), base);
    }
    out.objects.extend(objects);
}

/// The end of a `#tag` at `i`: a word boundary before, tag characters after
/// and at least one of them not a digit.
fn tag_end(s: &str, i: usize) -> Option<usize> {
    if char_before(s, i).is_some_and(|c| !c.is_whitespace()) {
        return None;
    }
    let mut end = i + 1;
    let mut any_letter = false;
    for c in s[i + 1..].chars() {
        if c.is_alphanumeric() || matches!(c, '_' | '-' | '/') {
            any_letter |= !c.is_ascii_digit();
            end += c.len_utf8();
        } else {
            break;
        }
    }
    (end > i + 1 && any_letter).then_some(end)
}

/// `target|100` or `target|100x200`: the target plus the size suffix.
fn split_size(inner: &str) -> (&str, Option<u32>, Option<u32>) {
    let Some((target, size)) = inner.rsplit_once('|') else {
        return (inner, None, None);
    };
    match size.split_once('x') {
        Some((w, h)) => match (w.parse().ok(), h.parse().ok()) {
            (Some(w), Some(h)) => (target, Some(w), Some(h)),
            _ => (inner, None, None),
        },
        None => match size.parse().ok() {
            Some(w) => (target, Some(w), None),
            None => (inner, None, None),
        },
    }
}

/// A `[label](url)` / `![alt](url)` starting at `i`, as its five relative
/// ranges: (marker, label, "](", url, ")").
struct LinkParts {
    marker: Range<usize>,
    label: Range<usize>,
    mid: Range<usize>,
    url: Range<usize>,
    close: Range<usize>,
}

impl LinkParts {
    fn object(&self, s: &str) -> InlineObjectKind {
        // a title after the destination: [a](b "title")
        let url = s[self.url.clone()]
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .trim_start_matches('<')
            .trim_end_matches('>')
            .to_string();
        if s[self.marker.clone()].starts_with('!') {
            let (_, width, height) = split_size(&s[self.label.clone()]);
            InlineObjectKind::Image {
                src: url,
                width,
                height,
            }
        } else {
            InlineObjectKind::Link { url }
        }
    }
}

fn parse_link(s: &str, i: usize) -> Option<LinkParts> {
    let bytes = s.as_bytes();
    let (start, lb) = if bytes[i] == b'!' { (i, i + 1) } else { (i, i) };
    if bytes.get(lb) != Some(&b'[') {
        return None;
    }
    // label: up to the next ']' without another '[' in between
    let mut j = lb + 1;
    while j < bytes.len() && bytes[j] != b']' && bytes[j] != b'[' {
        j += 1;
    }
    if j >= bytes.len() || bytes[j] != b']' || bytes.get(j + 1) != Some(&b'(') {
        return None;
    }
    let url_start = j + 2;
    let k = s[url_start..].find(')').map(|rel| url_start + rel)?;
    Some(LinkParts {
        marker: start..lb + 1,
        label: lb + 1..j,
        mid: j..j + 2,
        url: url_start..k,
        close: k..k + 1,
    })
}

/// The position of a backtick run of exactly `len` ticks, starting at `from`.
fn find_backtick_run(s: &str, from: usize, len: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut i = from;
    while i < bytes.len() {
        if bytes[i] == b'`' {
            let run = byte_run(s, i, b'`');
            if run == len {
                return Some(i);
            }
            i += run;
        } else {
            i += 1;
        }
    }
    None
}

/// `://` at `i` opens a bare URL only when preceded by a scheme
/// (at least "http." style letters and a valid scheme char before us).
fn is_bare_url_start(s: &str, i: usize) -> bool {
    let bytes = s.as_bytes();
    let mut k = i;
    while k > 0 && bytes[k - 1].is_ascii_alphanumeric() {
        k -= 1;
    }
    // require at least one letter and a scheme-ish separator boundary
    k < i && bytes[k].is_ascii_alphabetic() && (k == 0 || !bytes[k - 1].is_ascii_alphanumeric())
}

fn bare_url_start(s: &str, colon: usize) -> usize {
    let bytes = s.as_bytes();
    let mut k = colon;
    while k > 0 && bytes[k - 1].is_ascii_alphanumeric() {
        k -= 1;
    }
    k
}

/// A bare URL runs to whitespace, then gives back trailing punctuation that
/// belongs to the sentence around it (the GFM autolink rule).
fn bare_url_end(s: &str, from: usize) -> usize {
    let bytes = s.as_bytes();
    let mut k = from;
    while k < bytes.len() && !matches!(bytes[k], b' ' | b'\t' | b'<' | b'>') {
        k += 1;
    }
    while k > from
        && matches!(
            bytes[k - 1],
            b'.' | b',' | b':' | b';' | b'!' | b'?' | b'*' | b'_' | b'~'
        )
    {
        k -= 1;
    }
    k
}

/// Resolve emphasis delimiter runs in `gaps` into properly-nested pairs.
fn resolve_emphasis(s: &str, gaps: &[Range<usize>]) -> Vec<EmphPair> {
    let bytes = s.as_bytes();
    // collect delimiter runs within the gaps
    struct Delim {
        pos: usize,
        len: usize,
        ch: u8,
        can_open: bool,
        can_close: bool,
    }
    let mut delims: Vec<Delim> = Vec::new();
    for gap in gaps {
        let mut i = gap.start;
        while i < gap.end {
            let b = bytes[i];
            if matches!(b, b'*' | b'_' | b'~' | b'=') {
                let end = (i + byte_run(s, i, b)).min(gap.end);
                let len = end - i;
                let prev = char_before(s, i);
                let next = s[end..].chars().next();
                let left = left_flanking(prev, next);
                let right = right_flanking(prev, next);
                let (can_open, can_close) = match b {
                    b'~' | b'=' if len == 2 => (left, right),
                    b'~' | b'=' => (false, false),
                    b'_' => {
                        let intraword = prev.is_some_and(|c| c.is_alphanumeric())
                            && next.is_some_and(|c| c.is_alphanumeric());
                        (!intraword && left && !right, !intraword && right && !left)
                    }
                    _ => (left, right),
                };
                delims.push(Delim {
                    pos: i,
                    len,
                    ch: b,
                    can_open,
                    can_close,
                });
                i = end;
            } else {
                i += 1;
            }
        }
    }

    // the CommonMark delimiter algorithm: a closer eats the nearest compatible
    // opener from the inside out, splitting runs (`***a** b*`), so one run
    // can close several pairs. Strikes and highlights only pair whole `~~`/`==`.
    let mut used: Vec<(usize, usize)> = vec![(0, 0); delims.len()]; // eaten (from end, from start)
    let mut open: Vec<usize> = Vec::new(); // stack of delims indices
    let mut pairs: Vec<EmphPair> = Vec::new();
    for idx in 0..delims.len() {
        let closer = &delims[idx];
        if closer.can_close {
            while used[idx].0 + used[idx].1 < closer.len {
                let closer_left = closer.len - used[idx].0 - used[idx].1;
                let found = (0..open.len()).rev().find(|&stack_idx| {
                    let cand = &delims[open[stack_idx]];
                    let cand_left = cand.len - used[open[stack_idx]].0;
                    let whole_only = matches!(closer.ch, b'~' | b'=');
                    // the "rule of 3": a run that can do both must not pair a
                    // total multiple of 3 unless both runs are
                    let both = cand.can_close || closer.can_open;
                    let thirds = both
                        && (cand.len + closer.len).is_multiple_of(3)
                        && !(cand.len.is_multiple_of(3) && closer.len.is_multiple_of(3));
                    cand.ch == closer.ch
                        && cand.can_open
                        && cand_left > 0
                        && !thirds
                        && (!whole_only || (cand_left == 2 && closer_left == 2))
                });
                let Some(stack_idx) = found else {
                    break;
                };
                let opener_idx = open[stack_idx];
                let opener = &delims[opener_idx];
                let opener_left = opener.len - used[opener_idx].0;
                let eat = if opener_left >= 2 && closer_left >= 2 {
                    2
                } else {
                    1
                };
                let open_start = opener.pos + opener_left - eat;
                let close_start = closer.pos + used[idx].1;
                let flags = match (closer.ch, eat) {
                    (b'~', _) => InlineEmphasis {
                        strike: true,
                        ..InlineEmphasis::default()
                    },
                    (b'=', _) => InlineEmphasis {
                        highlight: true,
                        ..InlineEmphasis::default()
                    },
                    (_, 2) => InlineEmphasis {
                        bold: true,
                        ..InlineEmphasis::default()
                    },
                    _ => InlineEmphasis {
                        italic: true,
                        ..InlineEmphasis::default()
                    },
                };
                pairs.push(EmphPair {
                    open: open_start..open_start + eat,
                    content: open_start + eat..close_start,
                    close: close_start..close_start + eat,
                    flags,
                    // the whole runs reveal together: every `*` of `***a***`
                    // shows as soon as the caret touches the construct
                    owner: opener.pos..closer.pos + closer.len,
                });
                used[opener_idx].0 += eat;
                used[idx].1 += eat;
                // delimiters between the two can no longer pair
                open.truncate(stack_idx + 1);
                if used[opener_idx].0 == opener.len {
                    open.pop();
                }
            }
        }
        if closer.can_open && used[idx].0 + used[idx].1 < closer.len {
            open.push(idx);
        }
    }

    // keep only properly nested pairs: sort by content start (outer first),
    // drop any pair that partially overlaps an accepted one
    pairs.sort_by(|a, b| {
        a.content
            .start
            .cmp(&b.content.start)
            .then(b.content.end.cmp(&a.content.end))
    });
    let mut accepted: Vec<EmphPair> = Vec::new();
    let mut stack: Vec<Range<usize>> = Vec::new();
    for pair in pairs {
        while let Some(top) = stack.last() {
            if pair.content.start >= top.end {
                stack.pop();
            } else {
                break;
            }
        }
        if let Some(top) = stack.last()
            && pair.content.end > top.end
        {
            continue; // partial overlap
        }
        stack.push(pair.content.clone());
        accepted.push(pair);
    }
    accepted
}

/// The character ending at byte `i`.
fn char_before(s: &str, i: usize) -> Option<char> {
    s[..i].chars().next_back()
}

/// CommonMark-ish left-flanking: may open emphasis.
fn left_flanking(prev: Option<char>, next: Option<char>) -> bool {
    next.is_some_and(|n| !n.is_whitespace())
        && (next.is_none_or(|n| !n.is_ascii_punctuation())
            || prev.is_none_or(|p| p.is_whitespace() || p.is_ascii_punctuation()))
}

/// CommonMark-ish right-flanking: may close emphasis.
fn right_flanking(prev: Option<char>, next: Option<char>) -> bool {
    prev.is_some_and(|p| !p.is_whitespace())
        && (prev.is_none_or(|p| !p.is_ascii_punctuation())
            || next.is_none_or(|n| n.is_whitespace() || n.is_ascii_punctuation()))
}

/// Emit the runs of an emphasis forest: segments between nested pairs take
/// their enclosing pairs' emphasis; markers take Marker, owned by their pair.
/// Segments only ever cover the non-atomic gaps, so atomic spans (code/links)
/// stay intact.
fn flatten_pairs(
    pairs: &[EmphPair],
    region: Range<usize>,
    base: MarkdownLiveKind,
    flags: InlineEmphasis,
    gaps: &[Range<usize>],
    runs: &mut Vec<Inline>,
) {
    // children of this region: pairs inside it that are not contained in
    // another pair inside it (their own recursion handles the nested ones)
    let in_region = |p: &EmphPair| p.open.start >= region.start && p.close.end <= region.end;
    let mut children: Vec<&EmphPair> = pairs
        .iter()
        .filter(|p| {
            in_region(p)
                && !pairs.iter().any(|q| {
                    q != *p
                        && in_region(q)
                        && q.content.start <= p.content.start
                        && p.content.end <= q.content.end
                })
        })
        .collect();
    children.sort_by_key(|p| p.open.start);

    let mut cursor = region.start;
    let segment = |start: usize, end: usize, runs: &mut Vec<Inline>| {
        for gap in gaps {
            let from = gap.start.max(start);
            let to = gap.end.min(end);
            if from < to {
                runs.push(Inline {
                    range: from..to,
                    kind: base,
                    emphasis: flags,
                    owner: None,
                });
            }
        }
    };
    for pair in children {
        segment(cursor, pair.open.start, runs);
        let owner = pair.owner.clone();
        runs.push(Inline {
            range: pair.open.clone(),
            kind: MarkdownLiveKind::Marker,
            emphasis: flags.merge(pair.flags),
            owner: Some(owner.clone()),
        });
        let nested = flags.merge(pair.flags);
        flatten_pairs(
            pairs,
            pair.content.clone(),
            MarkdownLiveKind::emphasis(nested),
            nested,
            gaps,
            runs,
        );
        runs.push(Inline {
            range: pair.close.clone(),
            kind: MarkdownLiveKind::Marker,
            emphasis: flags.merge(pair.flags),
            owner: Some(owner),
        });
        cursor = pair.close.end;
    }
    segment(cursor, region.end, runs);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind_at(text: &str, byte: usize) -> MarkdownLiveKind {
        parse(text)
            .into_iter()
            .find(|s| s.range.contains(&byte))
            .map(|s| s.kind)
            .unwrap_or_else(|| panic!("no span covers byte {byte} of {text:?}"))
    }

    /// spans must be ordered, non-overlapping, and cover every byte
    fn assert_covered(text: &str) {
        let spans = parse(text);
        let mut cursor = 0;
        for span in &spans {
            assert_eq!(
                span.range.start, cursor,
                "gap/overlap at {} in {text:?}",
                span.range.start
            );
            cursor = span.range.end;
        }
        assert_eq!(cursor, text.len(), "coverage must reach end in {text:?}");
    }

    const BOLD: MarkdownLiveKind = MarkdownLiveKind::Emphasis {
        bold: true,
        italic: false,
        strike: false,
        highlight: false,
    };
    const ITALIC: MarkdownLiveKind = MarkdownLiveKind::Emphasis {
        bold: false,
        italic: true,
        strike: false,
        highlight: false,
    };
    const STRIKE: MarkdownLiveKind = MarkdownLiveKind::Emphasis {
        bold: false,
        italic: false,
        strike: true,
        highlight: false,
    };
    const BOLD_ITALIC: MarkdownLiveKind = MarkdownLiveKind::Emphasis {
        bold: true,
        italic: true,
        strike: false,
        highlight: false,
    };

    #[test]
    fn headings() {
        let text = "## Title\nbody";
        assert_eq!(kind_at(text, 0), MarkdownLiveKind::HeadingMarker);
        assert_eq!(kind_at(text, 1), MarkdownLiveKind::HeadingMarker);
        assert_eq!(kind_at(text, 3), MarkdownLiveKind::HeadingText);
        assert_eq!(kind_at(text, 9), MarkdownLiveKind::Plain);
        assert_covered(text);
        // hashes without a space are no heading: that is a tag
        assert_eq!(kind_at("#hashtag", 0), MarkdownLiveKind::Tag);
        // seven hashes is not a heading
        assert_eq!(kind_at("####### x", 0), MarkdownLiveKind::Plain);
    }

    #[test]
    fn emphasis_pairs() {
        assert_eq!(kind_at("**bold**", 2), BOLD);
        assert_eq!(kind_at("__bold__", 2), BOLD);
        assert_eq!(kind_at("*ital*", 1), ITALIC);
        assert_eq!(kind_at("***both***", 3), BOLD_ITALIC);
        assert_eq!(kind_at("~~gone~~", 2), STRIKE);
        // markers themselves
        assert_eq!(kind_at("**bold**", 0), MarkdownLiveKind::Marker);
        assert_eq!(kind_at("**bold**", 6), MarkdownLiveKind::Marker);
        // nesting: bold containing italic
        let text = "**a *b* c**";
        assert_eq!(kind_at(text, 2), BOLD);
        assert_eq!(kind_at(text, 3), BOLD);
        assert_eq!(kind_at(text, 5), BOLD_ITALIC);
        assert_eq!(kind_at(text, 4), MarkdownLiveKind::Marker);
        assert_eq!(kind_at(text, 8), BOLD);
        // unclosed stays plain
        assert_eq!(kind_at("**bold", 0), MarkdownLiveKind::Plain);
        assert_eq!(kind_at("a * b", 2), MarkdownLiveKind::Plain);
        // mismatched lengths stay plain
        assert_eq!(kind_at("**bold*", 0), MarkdownLiveKind::Plain);
        // intraword underscores (snake_case) stay plain
        assert_eq!(kind_at("snake_case_name", 5), MarkdownLiveKind::Plain);
        // intraword asterisks are emphasis
        assert_eq!(kind_at("a*b*c", 2), ITALIC);
        assert_covered("**a *b* c**");
        assert_covered("***both*** and ~~gone~~");
    }

    #[test]
    fn emphasis_does_not_span_lines() {
        let text = "*one\ntwo*";
        assert_eq!(kind_at(text, 1), MarkdownLiveKind::Plain);
        assert_eq!(kind_at(text, 5), MarkdownLiveKind::Plain);
    }

    #[test]
    fn inline_code() {
        assert_eq!(kind_at("`code`", 1), MarkdownLiveKind::InlineCode);
        assert_eq!(kind_at("`code`", 0), MarkdownLiveKind::Marker);
        assert_eq!(kind_at("``a ` b``", 2), MarkdownLiveKind::InlineCode);
        // unmatched backticks are plain
        assert_eq!(kind_at("`oops", 0), MarkdownLiveKind::Plain);
        assert_covered("x `code` y");
    }

    #[test]
    fn links_and_urls() {
        let text = "see [docs](https://example.com) now";
        assert_eq!(kind_at(text, 4), MarkdownLiveKind::Marker);
        assert_eq!(kind_at(text, 5), MarkdownLiveKind::LinkText);
        assert_eq!(kind_at(text, 10), MarkdownLiveKind::Marker);
        assert_eq!(kind_at(text, 12), MarkdownLiveKind::LinkUrl);
        assert_eq!(kind_at(text, 30), MarkdownLiveKind::Marker);
        assert_eq!(kind_at(text, 32), MarkdownLiveKind::Plain);
        assert_covered(text);

        let img = "![alt](pic.png)";
        assert_eq!(kind_at(img, 0), MarkdownLiveKind::Marker);
        assert_eq!(kind_at(img, 2), MarkdownLiveKind::LinkText);
        assert_eq!(kind_at(img, 7), MarkdownLiveKind::LinkUrl);

        let auto = "<https://example.com>";
        assert_eq!(kind_at(auto, 1), MarkdownLiveKind::LinkUrl);

        let bare = "go https://example.com/x end";
        assert_eq!(kind_at(bare, 3), MarkdownLiveKind::LinkUrl);

        // no url part: plain text
        assert_eq!(kind_at("[note]", 0), MarkdownLiveKind::Plain);
    }

    #[test]
    fn quotes_and_lists() {
        assert_eq!(kind_at("> quoted", 0), MarkdownLiveKind::QuoteMarker);
        assert_eq!(kind_at("> quoted", 2), MarkdownLiveKind::QuoteText);
        // markdown inside quotes still styles
        assert_eq!(kind_at("> **b**", 4), BOLD);

        assert_eq!(kind_at("- item", 0), MarkdownLiveKind::ListMarker);
        assert_eq!(kind_at("- item", 2), MarkdownLiveKind::Plain);
        assert_eq!(kind_at("3. item", 0), MarkdownLiveKind::ListMarker);
        assert_eq!(kind_at("1) item", 0), MarkdownLiveKind::ListMarker);

        assert_eq!(kind_at("- [ ] todo", 2), MarkdownLiveKind::TaskOpen);
        assert_eq!(kind_at("- [ ] todo", 6), MarkdownLiveKind::Plain);
        assert_eq!(kind_at("- [x] done", 2), MarkdownLiveKind::TaskDone);
        assert_eq!(kind_at("- [x] done", 6), STRIKE);
        assert_covered("- [x] done thing");
    }

    #[test]
    fn fenced_code() {
        let text = "```rust\nfn x() {}\n```\nafter";
        assert_eq!(kind_at(text, 0), MarkdownLiveKind::CodeFence);
        assert_eq!(kind_at(text, 8), MarkdownLiveKind::CodeBlock);
        assert_eq!(kind_at(text, 20), MarkdownLiveKind::CodeFence);
        assert_eq!(kind_at(text, 24), MarkdownLiveKind::Plain);
        assert_covered(text);

        // unterminated fence: body until EOF
        let open = "```\nbody";
        assert_eq!(kind_at(open, 5), MarkdownLiveKind::CodeBlock);

        // no emphasis inside code
        let code = "```\n**not bold**\n```";
        assert_eq!(kind_at(code, 5), MarkdownLiveKind::CodeBlock);
    }

    #[test]
    fn rules_and_escapes() {
        assert_eq!(kind_at("---", 0), MarkdownLiveKind::HorizontalRule);
        assert_eq!(kind_at("*  *  *", 0), MarkdownLiveKind::HorizontalRule);
        assert_eq!(kind_at("____", 0), MarkdownLiveKind::HorizontalRule);
        // escaped marker fades as source
        assert_eq!(kind_at(r"\*not\*", 0), MarkdownLiveKind::Marker);
        assert_eq!(kind_at(r"\*not\*", 2), MarkdownLiveKind::Plain);
        assert_covered(r"\*escaped\* rest");
    }

    #[test]
    fn adjacent_same_kind_merge() {
        let text = "```\na\nb\n```";
        let spans = parse(text);
        // fence, one merged code-block run, fence — nothing between a and b
        assert_eq!(spans.len(), 3, "{spans:?}");
        assert_covered(text);
    }

    #[test]
    fn empty_and_plain() {
        assert!(parse("").is_empty());
        assert_covered("just text, nothing markdown");
        assert_covered("");
        assert_covered("multi\nline\nplain");
    }

    #[test]
    fn toggle_task_flips_both_ways_and_only_on_tasks() {
        let text = "- [ ] first\n- [x] second\n- plain item";
        // open → done (byte inside and on both boundaries of `[ ]`)
        for byte in [2, 4, 5] {
            assert_eq!(
                toggle_task_at(text, byte).as_deref(),
                Some("- [x] first\n- [x] second\n- plain item")
            );
        }
        // done → open
        for byte in [14, 16, 17] {
            assert_eq!(
                toggle_task_at(text, byte).as_deref(),
                Some("- [ ] first\n- [ ] second\n- plain item")
            );
        }
        // one byte past the span belongs to the item text, not the box
        assert_eq!(toggle_task_at(text, 6), None);
        // list marker / plain item / out of range
        assert_eq!(toggle_task_at(text, 0), None);
        assert_eq!(toggle_task_at(text, 22), None);
        assert_eq!(toggle_task_at(text, 99), None);
    }

    #[test]
    fn toggle_task_keeps_length_and_later_offsets_stable() {
        let text = "- [X] capital done\nplain";
        // `[X]` normalizes to `[ ]` on untoggle and back to lowercase `[x]`
        let toggled = toggle_task_at(text, 3).unwrap();
        assert_eq!(toggled.len(), text.len());
        assert_eq!(toggled, "- [ ] capital done\nplain");
        assert_eq!(
            toggle_task_at(&toggled, 3).as_deref(),
            Some("- [x] capital done\nplain")
        );
    }

    #[test]
    fn unicode_never_splits_a_char() {
        let text = "**ğünaydın** dünya";
        assert_eq!(kind_at(text, 2), BOLD);
        assert_covered(text);
        // combining chars and CJK content
        let emoji = "- [x] 🎉 ship it";
        assert_eq!(kind_at(emoji, 2), MarkdownLiveKind::TaskDone);
        assert_covered(emoji);
    }

    fn line_kind(text: &str, line: usize) -> LiveLineKind {
        parse_document(text).lines[line].kind.clone()
    }

    /// The markers of `text` as (source, owner source) pairs.
    fn owned_markers(text: &str) -> Vec<(&str, &str)> {
        parse(text)
            .into_iter()
            .filter_map(|s| {
                let owner = s.owner?;
                Some((&text[s.range], &text[owner]))
            })
            .collect()
    }

    #[test]
    fn every_line_is_a_document_line_and_a_final_newline_adds_one() {
        assert_eq!(parse_document("").lines.len(), 1);
        assert_eq!(parse_document("a").lines.len(), 1);
        assert_eq!(parse_document("a\n").lines.len(), 2);
        assert_eq!(parse_document("a\n\nb\n").lines.len(), 4);
        let doc = parse_document("a\r\nb");
        assert_eq!(doc.lines[0].range, 0..1);
        assert_eq!(doc.lines[1].range, 3..4);
        assert_eq!(doc.line_at(0), 0);
        assert_eq!(doc.line_at(3), 1);
        assert_eq!(doc.line_at(99), 1);
    }

    #[test]
    fn nested_emphasis_combines_and_carries_onto_atomic_spans() {
        let text = "**bold `code` [l](u)**";
        let spans = parse(text);
        let code = spans
            .iter()
            .find(|s| s.kind == MarkdownLiveKind::InlineCode)
            .unwrap();
        assert!(code.emphasis.bold, "code inside bold stays bold");
        let link = spans
            .iter()
            .find(|s| s.kind == MarkdownLiveKind::LinkText)
            .unwrap();
        assert!(link.emphasis.bold, "link inside bold stays bold");
    }

    #[test]
    fn markers_know_the_construct_they_reveal_with() {
        assert_eq!(
            owned_markers("a **b** c"),
            [("**", "**b**"), ("**", "**b**")]
        );
        assert_eq!(owned_markers("`c`"), [("`", "`c`"), ("`", "`c`")]);
        assert_eq!(
            owned_markers("[t](u)"),
            [
                ("[", "[t](u)"),
                ("](", "[t](u)"),
                ("u", "[t](u)"),
                (")", "[t](u)")
            ]
        );
        assert_eq!(
            owned_markers("[[a|b]]"),
            [("[[a|", "[[a|b]]"), ("]]", "[[a|b]]")]
        );
        assert_eq!(owned_markers(r"\*"), [(r"\", r"\*")]);
        // nesting: the inner marker reveals with its own pair
        assert_eq!(
            owned_markers("**a *b***"),
            [
                ("**", "**a *b***"),
                ("*", "*b***"),
                ("*", "*b***"),
                ("**", "**a *b***")
            ]
        );
        // both sides of a triple run reveal with the whole construct
        assert_eq!(
            owned_markers("***a***"),
            [
                ("*", "***a***"),
                ("**", "***a***"),
                ("**", "***a***"),
                ("*", "***a***")
            ]
        );
    }

    #[test]
    fn delimiter_runs_split_like_commonmark() {
        let text = "***a** b*";
        assert_eq!(kind_at(text, 3), BOLD_ITALIC);
        assert_eq!(kind_at(text, 7), ITALIC);
        let text = "**a *b***";
        assert_eq!(kind_at(text, 2), BOLD);
        assert_eq!(kind_at(text, 5), BOLD_ITALIC);
    }

    #[test]
    fn highlights_pair_like_strikethrough() {
        let text = "a ==b== c";
        let spans = parse(text);
        let body = spans
            .iter()
            .find(|s| &text[s.range.clone()] == "b")
            .unwrap();
        assert!(body.emphasis.highlight);
        assert_eq!(kind_at("a == b", 2), MarkdownLiveKind::Plain);
        assert_eq!(kind_at("a=b", 1), MarkdownLiveKind::Plain);
        assert_covered(text);
    }

    #[test]
    fn wikilinks_embeds_tags_footnotes_comments() {
        assert_eq!(kind_at("[[note]]", 2), MarkdownLiveKind::WikiLink);
        let alias = "[[note|shown]]";
        assert_eq!(kind_at(alias, 3), MarkdownLiveKind::Marker);
        assert_eq!(kind_at(alias, 8), MarkdownLiveKind::WikiLink);
        let doc = parse_document(alias);
        assert_eq!(
            doc.lines[0].objects[0].kind,
            InlineObjectKind::WikiLink {
                target: "note".into()
            }
        );

        assert_eq!(kind_at("![[a.png]]", 3), MarkdownLiveKind::Embed);
        assert_eq!(
            parse_document("![[a.png|100x50]]").lines[0].objects[0].kind,
            InlineObjectKind::Image {
                src: "a.png".into(),
                width: Some(100),
                height: Some(50)
            }
        );

        assert_eq!(kind_at("a #tag b", 3), MarkdownLiveKind::Tag);
        assert_eq!(kind_at("#tag/sub-1", 9), MarkdownLiveKind::Tag);
        // not tags: inside a word, digits only, a bare hash
        assert_eq!(kind_at("a#tag", 2), MarkdownLiveKind::Plain);
        assert_eq!(kind_at("#123", 1), MarkdownLiveKind::Plain);
        assert_eq!(kind_at("# ", 1), MarkdownLiveKind::HeadingText);

        assert_eq!(kind_at("a[^1] b", 2), MarkdownLiveKind::FootnoteRef);
        assert_eq!(kind_at("a %%x%% b", 3), MarkdownLiveKind::Comment);
        assert_covered("a [[b]] ![[c]] #d [^e] %%f%% g");
    }

    #[test]
    fn urls_give_back_sentence_punctuation() {
        let text = "see https://example.org, then www.example.com.";
        assert_eq!(kind_at(text, 5), MarkdownLiveKind::LinkUrl);
        assert_eq!(kind_at(text, 23), MarkdownLiveKind::Plain);
        assert_eq!(kind_at(text, 32), MarkdownLiveKind::LinkUrl);
        assert_eq!(kind_at(text, 45), MarkdownLiveKind::Plain);
        let objects = &parse_document(text).lines[0].objects;
        assert_eq!(
            objects[1].kind,
            InlineObjectKind::Link {
                url: "https://www.example.com".into()
            }
        );
    }

    #[test]
    fn headings_parse_their_inlines_and_keep_the_marker_with_its_space() {
        let text = "## a **b**";
        assert_eq!(kind_at(text, 3), MarkdownLiveKind::HeadingText);
        assert_eq!(kind_at(text, 7), BOLD);
        let LiveLineKind::Heading { level, marker } = line_kind(text, 0) else {
            panic!("a heading");
        };
        assert_eq!((level, marker), (2, 0..3));
    }

    #[test]
    fn setext_underlines_turn_the_paragraph_above_into_a_heading() {
        let text = "Title\n===\n\nSub\n---\n\nrule\n\n---\n";
        let doc = parse_document(text);
        assert!(matches!(
            doc.lines[0].kind,
            LiveLineKind::Heading { level: 1, .. }
        ));
        assert_eq!(doc.lines[1].kind, LiveLineKind::SetextUnderline);
        assert!(matches!(
            doc.lines[3].kind,
            LiveLineKind::Heading { level: 2, .. }
        ));
        assert_eq!(doc.lines[4].kind, LiveLineKind::SetextUnderline);
        assert_eq!(doc.lines[8].kind, LiveLineKind::Rule);
        // a single dash is a list in the making, never an underline
        assert_eq!(line_kind("a\n-", 0), LiveLineKind::Paragraph);
    }

    #[test]
    fn list_levels_follow_the_indent_stack() {
        let text = "- a\n\t- b\n\t\t- c\n\t- d\n- e\n\n1. f\n  - g\n";
        let levels: Vec<u8> = parse_document(text)
            .lines
            .iter()
            .filter_map(|l| match &l.kind {
                LiveLineKind::List(item) => Some(item.level),
                _ => None,
            })
            .collect();
        assert_eq!(levels, [0, 1, 2, 1, 0, 0, 1]);
    }

    #[test]
    fn list_items_carry_marker_number_and_task() {
        let LiveLineKind::List(item) = line_kind("  12. [x] a", 0) else {
            panic!("a list item");
        };
        assert_eq!(item.marker, 2..6);
        assert_eq!(item.number, Some(12));
        let task = item.task.as_ref().unwrap();
        assert_eq!(task.range, 6..9);
        assert!(task.is_done());
        assert_eq!(item.content_start(), 10);

        // custom marks check the box but leave the text alone
        let LiveLineKind::List(item) = line_kind("- [/] a", 0) else {
            panic!("a list item");
        };
        let task = item.task.unwrap();
        assert!(task.is_checked() && !task.is_done());
        assert_eq!(kind_at("- [/] a", 2), MarkdownLiveKind::TaskDone);
        assert_eq!(kind_at("- [/] a", 6), MarkdownLiveKind::Plain);
    }

    #[test]
    fn continuation_lines_stay_in_the_list() {
        let text = "- a\n  more\nout";
        assert_eq!(
            line_kind(text, 1),
            LiveLineKind::ListContinuation { level: 0 }
        );
        assert_eq!(line_kind(text, 2), LiveLineKind::Paragraph);
    }

    #[test]
    fn indented_code_needs_a_blank_line_above() {
        assert_eq!(
            line_kind("a\n\n    code\n    more", 2),
            LiveLineKind::IndentedCode
        );
        assert_eq!(
            line_kind("a\n\n    code\n    more", 3),
            LiveLineKind::IndentedCode
        );
        assert_eq!(line_kind("a\n    b", 1), LiveLineKind::Paragraph);
        // inside a list it is a continuation, not code
        assert_eq!(
            line_kind("- a\n\n    b", 2),
            LiveLineKind::ListContinuation { level: 0 }
        );
    }

    #[test]
    fn quotes_keep_their_depth_and_inner_structure() {
        let doc = parse_document("> a\n> > b\n>> c\n> - d\n");
        let depths: Vec<u8> = doc
            .lines
            .iter()
            .filter_map(|l| l.quote.as_ref().map(|q| q.depth))
            .collect();
        assert_eq!(depths, [1, 2, 2, 1]);
        assert_eq!(doc.lines[1].quote.as_ref().unwrap().range, 4..8);
        assert!(matches!(doc.lines[3].kind, LiveLineKind::List(_)));
    }

    #[test]
    fn callouts_group_their_quote_lines() {
        let text = "> [!NOTE]- Title here\n> body\n> more\n\n> plain\n> [!x] not a head\n";
        let doc = parse_document(text);
        assert_eq!(doc.blocks.len(), 1, "{:?}", doc.blocks);
        let block = &doc.blocks[0];
        assert_eq!(block.lines, 0..3);
        let LiveBlockKind::Callout {
            callout,
            marker,
            title,
        } = &block.kind
        else {
            panic!("a callout");
        };
        assert_eq!(callout, "note");
        assert_eq!(&text[marker.clone()], "> [!NOTE]- ");
        assert_eq!(&text[title.clone().unwrap()], "Title here");
        assert_eq!(doc.lines[1].block, Some(0));
        assert_eq!(doc.lines[4].block, None);
    }

    #[test]
    fn code_blocks_carry_lang_and_span_their_fences() {
        let text = "a\n```rust title\nb\n```\nc\n~~~\nopen";
        let doc = parse_document(text);
        assert_eq!(doc.blocks.len(), 2);
        assert_eq!(doc.blocks[0].lines, 1..4);
        assert_eq!(&text[doc.blocks[0].range.clone()], "```rust title\nb\n```");
        assert_eq!(
            doc.blocks[0].kind,
            LiveBlockKind::Code {
                lang: Some("rust".into())
            }
        );
        // unterminated: runs to the end
        assert_eq!(doc.blocks[1].lines, 5..7);
        assert_eq!(doc.blocks[1].range.end, text.len());
    }

    #[test]
    fn tables_need_a_matching_delimiter() {
        let text = "| a | b |\n| - | :-: |\n| `x|y` | **z** |\nafter\n\nno | table\n";
        let doc = parse_document(text);
        assert_eq!(doc.blocks.len(), 1);
        let LiveBlockKind::Table {
            columns,
            rows,
            delimiter,
        } = &doc.blocks[0].kind
        else {
            panic!("a table");
        };
        assert_eq!(*columns, [TableAlign::Left, TableAlign::Center]);
        assert_eq!(*delimiter, 1);
        assert_eq!(doc.blocks[0].lines, 0..4);
        let cell = |line: usize, cell: usize| &text[rows[line].cells[cell].clone()];
        assert_eq!((cell(0, 0), cell(0, 1)), ("a", "b"));
        assert_eq!((cell(1, 0), cell(1, 1)), ("`x|y`", "**z**"));
        // cells parse their inlines
        assert!(
            doc.lines[2]
                .spans
                .iter()
                .any(|s| s.kind == MarkdownLiveKind::InlineCode)
        );
        // a delimiter with the wrong column count leaves plain text
        assert!(parse_document("a | b\n- |\n").blocks.is_empty());
        assert_covered(text);
    }

    #[test]
    fn a_blank_cell_holds_the_caret_after_its_first_space() {
        let text = "| a |   |\n| - | - |\n";
        let doc = parse_document(text);
        let LiveBlockKind::Table { rows, .. } = &doc.blocks[0].kind else {
            panic!("a table");
        };
        assert_eq!(rows[0].cells[1], 6..6);
    }

    #[test]
    fn frontmatter_only_opens_a_document() {
        let text = "---\ntitle: a\n---\nbody\n";
        let doc = parse_document(text);
        assert!(matches!(
            doc.blocks[0].kind,
            LiveBlockKind::Frontmatter { .. }
        ));
        assert_eq!(doc.blocks[0].lines, 0..3);
        assert_eq!(doc.lines[1].kind, LiveLineKind::Frontmatter);
        assert_eq!(doc.lines[3].kind, LiveLineKind::Paragraph);
        // unterminated or later: a plain rule
        assert_eq!(line_kind("---\nbody", 0), LiveLineKind::Rule);
        assert_eq!(line_kind("a\n\n---\nb\n---", 2), LiveLineKind::Rule);
    }

    fn properties(text: &str) -> Vec<Property> {
        match parse_document(text)
            .blocks
            .into_iter()
            .next()
            .map(|b| b.kind)
        {
            Some(LiveBlockKind::Frontmatter { properties }) => properties,
            other => panic!("no frontmatter: {other:?}"),
        }
    }

    #[test]
    fn properties_read_keys_values_and_types() {
        let text = "---\ntitle: \"Quoted\"\ncount: 12\ndone: true\nday: 2026-11-01\nat: 2026-11-01T10:30\nsec: 2026-11-01T10:30:00\nurl: https://a.b\nempty:\n---\n";
        let got = properties(text);
        let shown: Vec<(&str, &str, PropertyType)> = got
            .iter()
            .map(|p| (&text[p.key.clone()], &text[p.value.clone()], p.kind))
            .collect();
        assert_eq!(
            shown,
            [
                ("title", "Quoted", PropertyType::Text),
                ("count", "12", PropertyType::Number),
                ("done", "true", PropertyType::Checkbox),
                ("day", "2026-11-01", PropertyType::Date),
                ("at", "2026-11-01T10:30", PropertyType::DateTime),
                ("sec", "2026-11-01T10:30:00", PropertyType::Text),
                ("url", "https://a.b", PropertyType::Text),
                ("empty", "", PropertyType::Text),
            ]
        );
        assert_eq!(
            got[7].value,
            text.find("empty:").unwrap() + 6..text.find("empty:").unwrap() + 6
        );
    }

    #[test]
    fn properties_collect_list_items_in_both_shapes() {
        let text =
            "---\ntags:\n  - alpha\n  - \"beta\"\naliases: [one, 'two']\nrelated:\n- x\n---\n";
        let got = properties(text);
        let items = |p: &Property| -> Vec<String> {
            p.items
                .iter()
                .map(|i| text[i.text.clone()].to_string())
                .collect()
        };
        assert_eq!(got.len(), 3);
        assert_eq!(got[0].kind, PropertyType::Tags);
        assert_eq!(items(&got[0]), ["alpha", "beta"]);
        assert_eq!(got[1].kind, PropertyType::Aliases);
        assert_eq!(items(&got[1]), ["one", "two"]);
        assert_eq!(got[2].kind, PropertyType::List);
        assert_eq!(items(&got[2]), ["x"]);
    }

    #[test]
    fn unusual_yaml_stays_as_bare_lines() {
        let text = "---\nmap:\n  nested: 1\n# a comment\n---\n";
        let got = properties(text);
        assert_eq!(&text[got[1].value.clone()], "nested: 1");
        assert!(got[1].key.is_empty() && got[1].bare);
        assert_eq!(&text[got[2].value.clone()], "# a comment");
    }

    #[test]
    fn a_property_being_added_has_an_empty_key() {
        let text = "---\n: \n---\n";
        let got = properties(text);
        assert_eq!(got.len(), 1);
        assert!(got[0].key.is_empty() && got[0].value.is_empty());
        assert!(!got[0].bare);
    }

    #[test]
    fn a_property_lists_where_the_caret_may_sit() {
        let text = "---\na: 1\nb: true\ntags:\n  - x\n  - y\nc:\n# bare\n---\n";
        let got = properties(text);
        let fields: Vec<Vec<&str>> = got
            .iter()
            .map(|p| p.fields().into_iter().map(|f| &text[f]).collect())
            .collect();
        assert_eq!(
            fields,
            [
                vec!["a", "1"],
                vec!["b"],
                vec!["tags", "x", "y"],
                vec!["c", ""],
                vec!["# bare"],
            ]
        );
    }

    #[test]
    fn footnote_definitions_split_label_and_marker() {
        let text = "[^note]: body **b**";
        let LiveLineKind::FootnoteDefinition { label, marker } = line_kind(text, 0) else {
            panic!("a definition");
        };
        assert_eq!(&text[label], "note");
        assert_eq!(&text[marker], "[^note]: ");
        assert_eq!(kind_at(text, 16), BOLD);
    }

    #[test]
    fn multibyte_neighbours_never_read_as_whitespace() {
        // à ends in the byte 0xA0, which is NBSP when misread as Latin-1
        assert_eq!(kind_at("à*b*", 2), MarkdownLiveKind::Marker);
        assert_covered("à*b* ğ**ü**ş");
    }

    #[test]
    fn every_line_kind_stays_covered() {
        let text = "---\na: b\n---\n# h\n\n> [!note] t\n> b\n\n| a |\n| - |\n| b |\n\n```\nc\n```\n\n- [ ] t\n1. n\n[^1]: f\n---\nend";
        assert_covered(text);
        for line in parse_document(text).lines {
            let mut cursor = line.range.start;
            for span in &line.spans {
                assert_eq!(span.range.start, cursor, "gap in {line:?}");
                cursor = span.range.end;
            }
            assert_eq!(cursor, line.range.end, "coverage ends short in {line:?}");
        }
    }
}
