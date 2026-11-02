//! The live editor's pure editing commands: each takes the text and selection
//! and returns the text and selection after the command, so its behavior is
//! testable without a window. The expected results in the tests are what
//! Obsidian's live preview does for the same keystroke.

use crate::live::{
    self, LiveBlockKind, LiveDocument, LiveLine, LiveLineKind, Property, TableAlign,
};
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation as _;

/// One tab stop; a Tab key indents with a real tab, like Obsidian's default.
const TAB_COLUMNS: usize = 4;

/// Bracket-like pairs closed automatically: (open, close).
const PAIRS: [(char, char); 8] = [
    ('(', ')'),
    ('[', ']'),
    ('{', '}'),
    ('"', '"'),
    ('\'', '\''),
    ('`', '`'),
    ('*', '*'),
    ('_', '_'),
];

/// Characters that wrap a selection without pairing at a bare caret.
const WRAP_ONLY: [char; 3] = ['~', '=', '$'];

/// Closing characters a new pair may be typed in front of.
const CLOSE_BEFORE: &str = ")]}:;>";

/// The narrowest a formatted table column gets, Obsidian's `---`.
const MIN_TABLE_COLUMN: usize = 3;

/// How a list property's items are indented when this editor writes them.
const ITEM_PREFIX: &str = "  - ";

/// A selection as byte offsets: `head` is the moving end, the caret.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    pub anchor: usize,
    pub head: usize,
}

impl Selection {
    pub const fn caret(offset: usize) -> Self {
        Self {
            anchor: offset,
            head: offset,
        }
    }

    pub const fn is_caret(self) -> bool {
        self.anchor == self.head
    }

    pub fn range(self) -> Range<usize> {
        self.anchor.min(self.head)..self.anchor.max(self.head)
    }

    fn map(self, f: impl Fn(usize) -> usize) -> Self {
        Self {
            anchor: f(self.anchor),
            head: f(self.head),
        }
    }
}

/// What an editing command leaves behind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub text: String,
    pub selection: Selection,
    /// Offset of a closer the command typed itself, which the same character
    /// typed again steps over instead of doubling.
    pub closed: Option<usize>,
}

impl Change {
    fn new(text: String, selection: Selection) -> Self {
        Self {
            text,
            selection,
            closed: None,
        }
    }

    fn replace(text: &str, range: Range<usize>, with: &str, caret: usize) -> Self {
        Self::new(
            format!("{}{with}{}", &text[..range.start], &text[range.end..]),
            Selection::caret(caret),
        )
    }
}

fn splice(text: &str, range: Range<usize>, with: &str) -> String {
    format!("{}{with}{}", &text[..range.start], &text[range.end..])
}

/// The line prefix a new line under `line` repeats, plus what it means for
/// the item the caret is on.
struct Continuation {
    /// `> ` in front of everything.
    quote: String,
    indent: String,
    marker: String,
    /// Where the line's own text starts.
    content: usize,
}

fn list_continuation(text: &str, line: &LiveLine) -> Option<Continuation> {
    let LiveLineKind::List(item) = &line.kind else {
        return None;
    };
    let quote_end = line
        .quote
        .as_ref()
        .map_or(line.range.start, |q| q.range.end);
    let marker = text[item.marker.clone()].trim_end();
    let separator = &text[marker.len() + item.marker.start..item.marker.end];
    let next = match item.number {
        Some(number) => {
            let delimiter = &marker[marker.len() - 1..];
            format!("{}{delimiter}", number + 1)
        }
        None => marker.to_string(),
    };
    let task = if item.task.is_some() { "[ ] " } else { "" };
    Some(Continuation {
        quote: text[line.range.start..quote_end].to_string(),
        indent: text[quote_end..item.marker.start].to_string(),
        marker: format!("{next}{separator}{task}"),
        content: item.content_start().min(line.range.end),
    })
}

/// The Enter key. Lists and quotes continue, an empty item ends them,
/// everything else is a newline that keeps the line's indent.
pub fn enter(text: &str, selection: Selection) -> Change {
    let range = selection.range();
    let doc = live::parse_document(text);
    let line = &doc.lines[doc.line_at(range.start)];

    if let Some(list) = list_continuation(text, line)
        && range.start >= list.content
    {
        let empty = list.content >= line.range.end && selection.is_caret();
        if empty {
            let quote_end = line.range.start + list.quote.len();
            if list.indent.is_empty() {
                // an empty top-level item clears its marker
                return Change::replace(text, quote_end..line.range.end, "", quote_end);
            }
            // an empty nested item moves up a level
            let outdented = outdent_prefix(&list.indent);
            let marker_start = quote_end + list.indent.len();
            let caret = line.range.end - (list.indent.len() - outdented.len());
            let change = Change::replace(text, quote_end..marker_start, &outdented, caret);
            return renumber(change, doc.line_at(range.start));
        }
        let insert = format!("\n{}{}{}", list.quote, list.indent, list.marker);
        let caret = range.start + insert.len();
        let change = Change::replace(text, range, &insert, caret);
        return renumber(change, doc.line_at(selection.range().start) + 1);
    }

    if let Some(quote) = &line.quote
        && !matches!(line.kind, LiveLineKind::List(_))
    {
        if line.range.end == quote.range.end && selection.is_caret() {
            // an empty quote line ends the quote, leaving a blank line
            return Change::replace(text, line.range.clone(), "\n", line.range.start + 1);
        }
        if range.start >= quote.range.end {
            let insert = format!("\n{}", &text[quote.range.clone()]);
            let caret = range.start + insert.len();
            return Change::replace(text, range, &insert, caret);
        }
    }

    newline(text, range, line)
}

/// A plain newline keeping the indent, dropping spaces right after the caret.
fn newline(text: &str, range: Range<usize>, line: &LiveLine) -> Change {
    let indent = leading_whitespace(&text[line.range.clone()]);
    let keep = indent.len().min(range.start - line.range.start);
    let skip = text[range.end..line.range.end]
        .bytes()
        .take_while(|&b| b == b' ')
        .count();
    let insert = format!("\n{}", &indent[..keep]);
    let caret = range.start + insert.len();
    Change::replace(text, range.start..range.end + skip, &insert, caret)
}

/// Shift+Enter: a new line that stays inside the list item or quote.
pub fn soft_break(text: &str, selection: Selection) -> Change {
    let range = selection.range();
    let doc = live::parse_document(text);
    let line = &doc.lines[doc.line_at(range.start)];
    if let Some(list) = list_continuation(text, line)
        && range.start >= list.content
    {
        let content_col = list.content - (line.range.start + list.quote.len() + list.indent.len());
        let insert = format!("\n{}{}{}", list.quote, list.indent, " ".repeat(content_col));
        let caret = range.start + insert.len();
        return Change::replace(text, range, &insert, caret);
    }
    if line.quote.is_some() {
        return enter(text, selection);
    }
    let insert = "\n";
    Change::replace(text, range.clone(), insert, range.start + 1)
}

fn leading_whitespace(line: &str) -> &str {
    &line[..line.len() - line.trim_start_matches([' ', '\t']).len()]
}

fn columns(indent: &str) -> usize {
    indent.bytes().fold(0, |column, byte| match byte {
        b'\t' => (column / TAB_COLUMNS + 1) * TAB_COLUMNS,
        _ => column + 1,
    })
}

/// The indent one level shallower, tabs first like `indentString`.
fn outdent_prefix(indent: &str) -> String {
    let target = columns(indent).saturating_sub(TAB_COLUMNS);
    let tabs = "\t".repeat(target / TAB_COLUMNS);
    format!("{tabs}{}", " ".repeat(target % TAB_COLUMNS))
}

/// The lines a line-wise command covers; a selection ending at the very
/// start of a line leaves that line out.
fn selected_lines(doc: &LiveDocument, range: &Range<usize>) -> Range<usize> {
    let first = doc.line_at(range.start);
    let mut last = doc.line_at(range.end);
    if last > first && doc.lines[last].range.start == range.end {
        last -= 1;
    }
    first..last + 1
}

/// Applies `edits` (ascending, non-overlapping) and maps the selection, with
/// an insertion at the selection's own offset pushing it right.
fn apply_edits(text: &str, selection: Selection, edits: &[(Range<usize>, String)]) -> Change {
    let mut out = String::with_capacity(text.len() + edits.len());
    let mut cursor = 0;
    for (range, with) in edits {
        out.push_str(&text[cursor..range.start]);
        out.push_str(with);
        cursor = range.end;
    }
    out.push_str(&text[cursor..]);
    let map = |pos: usize| {
        let mut shifted = pos as isize;
        for (range, with) in edits {
            if range.end <= pos {
                shifted += with.len() as isize - range.len() as isize;
            } else if range.start < pos {
                shifted = (range.start + with.len()).min(pos) as isize;
            }
        }
        shifted as usize
    };
    Change::new(out, selection.map(map))
}

/// The Tab key indents every selected line, wherever the caret is on it.
pub fn indent(text: &str, selection: Selection) -> Change {
    let range = selection.range();
    let doc = live::parse_document(text);
    let lines = selected_lines(&doc, &range);
    let edits: Vec<_> = doc.lines[lines.clone()]
        .iter()
        .map(|line| (line.range.start..line.range.start, "\t".to_string()))
        .collect();
    let mut change = apply_edits(text, selection, &edits);
    // the new level's numbering starts over under its parent
    if let Some(line) = doc.lines.get(lines.start)
        && let LiveLineKind::List(item) = &line.kind
        && item.number.is_some()
    {
        change = restart_numbering(change, lines.start);
        change = renumber(change, lines.start);
    }
    change
}

/// Shift+Tab: one level off every selected line.
pub fn outdent(text: &str, selection: Selection) -> Change {
    let range = selection.range();
    let doc = live::parse_document(text);
    let lines = selected_lines(&doc, &range);
    let edits: Vec<_> = doc.lines[lines.clone()]
        .iter()
        .filter_map(|line| {
            let indent = leading_whitespace(&text[line.range.clone()]);
            (!indent.is_empty()).then(|| {
                (
                    line.range.start..line.range.start + indent.len(),
                    outdent_prefix(indent),
                )
            })
        })
        .collect();
    let change = apply_edits(text, selection, &edits);
    match doc.lines.get(lines.start).map(|line| &line.kind) {
        Some(LiveLineKind::List(item)) if item.number.is_some() => renumber(change, lines.start),
        _ => change,
    }
}

/// Sets line `line`'s ordered marker to 1 when nothing precedes it on its
/// level.
fn restart_numbering(change: Change, line: usize) -> Change {
    let doc = live::parse_document(&change.text);
    let Some(LiveLineKind::List(item)) = doc.lines.get(line).map(|l| &l.kind) else {
        return change;
    };
    let level = item.level;
    let has_sibling = doc.lines[..line]
        .iter()
        .rev()
        .find_map(|prev| match &prev.kind {
            LiveLineKind::List(prev) if prev.level == level => Some(true),
            LiveLineKind::List(prev) if prev.level < level => Some(false),
            LiveLineKind::List(_) | LiveLineKind::ListContinuation { .. } => None,
            _ => Some(false),
        });
    if has_sibling == Some(true) {
        return change;
    }
    let digits = item.marker.start
        ..item.marker.start
            + change.text[item.marker.clone()]
                .bytes()
                .take_while(u8::is_ascii_digit)
                .count();
    apply_edits(&change.text, change.selection, &[(digits, "1".to_string())])
}

/// Renumbers the ordered list around `line`: every level counts up from its
/// first item, the way Obsidian keeps a list sequential as items move.
fn renumber(change: Change, line: usize) -> Change {
    let doc = live::parse_document(&change.text);
    let item_line = |ix: usize| -> bool {
        doc.lines.get(ix).is_some_and(|l| {
            matches!(
                l.kind,
                LiveLineKind::List(_) | LiveLineKind::ListContinuation { .. }
            )
        })
    };
    if !item_line(line) {
        return change;
    }
    let mut first = line;
    while first > 0 && item_line(first - 1) {
        first -= 1;
    }
    let mut counters: Vec<Option<u32>> = Vec::new();
    let mut edits = Vec::new();
    let mut ix = first;
    while item_line(ix) {
        if let LiveLineKind::List(item) = &doc.lines[ix].kind {
            let level = usize::from(item.level);
            counters.truncate(level + 1);
            counters.resize(level + 1, None);
            let number = item.number.map(|own| match counters[level] {
                Some(previous) => previous + 1,
                None => own,
            });
            counters[level] = number;
            if let (Some(want), Some(own)) = (number, item.number)
                && want != own
            {
                let digits = change.text[item.marker.clone()]
                    .bytes()
                    .take_while(u8::is_ascii_digit)
                    .count();
                edits.push((
                    item.marker.start..item.marker.start + digits,
                    want.to_string(),
                ));
            }
        }
        ix += 1;
    }
    if edits.is_empty() {
        return change;
    }
    apply_edits(&change.text, change.selection, &edits)
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// One typed character, when auto-pairing has something to say about it:
/// a selection gets wrapped, an opener gets its closer, a closer the editor
/// typed itself gets stepped over. `None` is a plain insertion.
pub fn type_char(text: &str, selection: Selection, ch: char, closed: &[usize]) -> Option<Change> {
    let range = selection.range();
    let pair = PAIRS.iter().find(|(open, _)| *open == ch).copied();

    if !selection.is_caret() {
        let close = pair
            .map(|(_, close)| close)
            .or_else(|| WRAP_ONLY.contains(&ch).then_some(ch))?;
        let inner = &text[range.clone()];
        let wrapped = format!("{ch}{inner}{close}");
        let shift = ch.len_utf8();
        return Some(Change {
            text: splice(text, range, &wrapped),
            selection: selection.map(|pos| pos + shift),
            closed: Some(selection.range().end + shift),
        });
    }

    let caret = selection.head;
    let next = text[caret..].chars().next();
    // step over a closer the editor typed itself
    if next == Some(ch) && closed.contains(&caret) {
        return Some(Change::new(
            text.to_string(),
            Selection::caret(caret + ch.len_utf8()),
        ));
    }

    // the third backtick of a line opens a fence: its closer comes along
    if ch == '`' {
        let line_start = text[..caret].rfind('\n').map_or(0, |ix| ix + 1);
        if &text[line_start..caret] == "``" && next.is_none_or(|n| n == '\n') {
            return Some(Change::replace(text, caret..caret, "`\n```", caret + 1));
        }
    }

    let (open, close) = pair?;
    if open == close {
        // quote-likes: never right after a word or in front of one, and a
        // second one in a row is closing something, not opening
        let prev = text[..caret].chars().next_back();
        if prev == Some(ch) || prev.is_some_and(is_word_char) || next.is_some_and(is_word_char) {
            return None;
        }
    } else if next.is_some_and(|n| !n.is_whitespace() && !CLOSE_BEFORE.contains(n)) {
        return None;
    }
    let shift = ch.len_utf8();
    Some(Change {
        text: splice(text, range, &format!("{open}{close}")),
        selection: Selection::caret(caret + shift),
        closed: Some(caret + shift),
    })
}

/// The smallest single edit turning `old` into `new`: the replaced range of
/// `old` and the inserted length. Offsets land on char boundaries of both.
pub fn diff(old: &str, new: &str) -> (Range<usize>, usize) {
    let mut prefix = old
        .bytes()
        .zip(new.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    while !old.is_char_boundary(prefix) || !new.is_char_boundary(prefix) {
        prefix -= 1;
    }
    let mut suffix = old[prefix..]
        .bytes()
        .rev()
        .zip(new[prefix..].bytes().rev())
        .take_while(|(a, b)| a == b)
        .count();
    while !old.is_char_boundary(old.len() - suffix) || !new.is_char_boundary(new.len() - suffix) {
        suffix -= 1;
    }
    (prefix..old.len() - suffix, new.len() - suffix - prefix)
}

/// Moves tracked closers through one edit, dropping the ones it removed.
pub fn map_closed(closed: &mut Vec<usize>, edit: &Range<usize>, inserted: usize) {
    closed.retain_mut(|at| {
        if *at >= edit.end {
            *at = *at - edit.len() + inserted;
            true
        } else {
            *at < edit.start
        }
    });
}

/// Backspace between an empty pair deletes both sides.
pub fn delete_pair(text: &str, selection: Selection) -> Option<Change> {
    if !selection.is_caret() {
        return None;
    }
    let caret = selection.head;
    let before = text[..caret].chars().next_back()?;
    let after = text[caret..].chars().next()?;
    PAIRS
        .iter()
        .any(|&(open, close)| open == before && close == after)
        .then(|| {
            let start = caret - before.len_utf8();
            Change::replace(text, start..caret + after.len_utf8(), "", start)
        })
}

/// The word around `caret`, for commands that act on the word under a bare
/// caret.
fn word_at(text: &str, caret: usize) -> Range<usize> {
    let start = text[..caret]
        .char_indices()
        .rev()
        .take_while(|(_, c)| is_word_char(*c))
        .last()
        .map_or(caret, |(ix, _)| ix);
    let end = text[caret..]
        .char_indices()
        .take_while(|(_, c)| is_word_char(*c))
        .last()
        .map_or(caret, |(ix, c)| caret + ix + c.len_utf8());
    start..end
}

/// Ctrl+B/Ctrl+I: wraps the selection (or the word under the caret) in
/// `marker`, or unwraps it when `marker` already surrounds it.
pub fn toggle_wrap(text: &str, selection: Selection, marker: &str) -> Change {
    let range = selection.range();

    // an exact selection between the markers, or a caret inside them
    let target = if selection.is_caret() {
        enclosing_wrap(text, selection.head, marker)
    } else {
        let wrapped = text[..range.start].ends_with(marker)
            && text[range.end..].starts_with(marker)
            && !doubled(text, range.start, marker);
        wrapped.then(|| range.clone())
    };
    if let Some(inner) = target {
        let start = inner.start - marker.len();
        let end = inner.end + marker.len();
        let unwrapped = format!("{}{}{}", &text[..start], &text[inner.clone()], &text[end..]);
        return Change::new(
            unwrapped,
            selection.map(|pos| pos.clamp(inner.start, inner.end) - marker.len()),
        );
    }

    let span = if selection.is_caret() {
        word_at(text, selection.head)
    } else {
        range
    };
    let wrapped = format!("{marker}{}{marker}", &text[span.clone()]);
    Change::new(
        splice(text, span, &wrapped),
        selection.map(|pos| pos + marker.len()),
    )
}

/// `*` must not read a `**` as its own marker when toggling italics.
fn doubled(text: &str, start: usize, marker: &str) -> bool {
    marker.len() == 1 && text[..start - marker.len()].ends_with(marker)
}

/// The content between the `marker` pair around `caret` on its line.
fn enclosing_wrap(text: &str, caret: usize, marker: &str) -> Option<Range<usize>> {
    let line_start = text[..caret].rfind('\n').map_or(0, |ix| ix + 1);
    let line_end = text[caret..].find('\n').map_or(text.len(), |ix| caret + ix);
    let before = &text[line_start..caret];
    let after = &text[caret..line_end];
    let open = before.rfind(marker)?;
    let close = after.find(marker)?;
    let inner = line_start + open + marker.len()..caret + close;
    // `**wo|rd**` is no `*…*`: the marker run must be exactly this long
    let run_before = before[..open].ends_with(marker.chars().next()?);
    let run_after = after[close + marker.len()..].starts_with(marker.chars().next()?);
    (!run_before && !run_after).then_some(inner)
}

/// Ctrl+K: `[selection]()` with the caret on the URL, or `[]()` with the
/// caret on the label.
pub fn insert_link(text: &str, selection: Selection) -> Change {
    let range = selection.range();
    if selection.is_caret() {
        return Change::replace(text, range.clone(), "[]()", range.start + 1);
    }
    let link = format!("[{}]()", &text[range.clone()]);
    let caret = range.start + link.len() - 1;
    Change::replace(text, range, &link, caret)
}

/// Ctrl+L: flips the checkbox, adds one to a list item, or turns a plain
/// line into a task.
pub fn toggle_task_line(text: &str, selection: Selection) -> Change {
    let doc = live::parse_document(text);
    let line = &doc.lines[doc.line_at(selection.head)];
    let edit = match &line.kind {
        LiveLineKind::List(item) => match &item.task {
            Some(task) => {
                let mark = if task.is_checked() { "[ ]" } else { "[x]" };
                (task.range.clone(), mark.to_string())
            }
            None => (item.marker.end..item.marker.end, "[ ] ".to_string()),
        },
        _ => {
            let indent = leading_whitespace(&text[line.range.clone()]).len();
            let at = line.range.start + indent;
            (at..at, "- [ ] ".to_string())
        }
    };
    apply_edits(text, selection, &[edit])
}

/// Removes the caret's line, leaving the caret at the same offset.
pub fn delete_line(text: &str, selection: Selection) -> Change {
    let doc = live::parse_document(text);
    let index = doc.line_at(selection.head);
    let line = &doc.lines[index];
    let end = doc
        .lines
        .get(index + 1)
        .map_or(line.range.end, |next| next.range.start);
    let start = if index + 1 == doc.lines.len() && index > 0 {
        doc.lines[index - 1].range.end
    } else {
        line.range.start
    };
    let out = splice(text, start..end, "");
    let caret = floor_char_boundary(&out, selection.head.min(out.len()));
    Change::new(out, Selection::caret(caret))
}

/// The closest char boundary at or before `offset`.
fn floor_char_boundary(text: &str, mut offset: usize) -> usize {
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

/// The grapheme boundary before `offset`.
pub fn prev_grapheme(text: &str, offset: usize) -> usize {
    text[..offset]
        .grapheme_indices(true)
        .next_back()
        .map_or(0, |(ix, _)| ix)
}

/// The grapheme boundary after `offset`.
pub fn next_grapheme(text: &str, offset: usize) -> usize {
    text[offset..]
        .graphemes(true)
        .next()
        .map_or(text.len(), |grapheme| offset + grapheme.len())
}

#[derive(PartialEq, Clone, Copy)]
enum CharClass {
    Space,
    Word,
    Other,
}

fn char_class(c: char) -> CharClass {
    if c.is_whitespace() {
        CharClass::Space
    } else if is_word_char(c) {
        CharClass::Word
    } else {
        CharClass::Other
    }
}

/// Ctrl+Left: over any spaces, then one group of like characters.
pub fn word_left(text: &str, offset: usize) -> usize {
    let mut chars = text[..offset].char_indices().rev().peekable();
    let mut pos = offset;
    while let Some(&(ix, c)) = chars.peek() {
        if c == '\n' || !c.is_whitespace() {
            break;
        }
        pos = ix;
        chars.next();
    }
    let Some(&(ix, first)) = chars.peek() else {
        return pos;
    };
    if first == '\n' {
        return if pos == offset { ix } else { pos };
    }
    let class = char_class(first);
    for (ix, c) in chars {
        if char_class(c) != class {
            break;
        }
        pos = ix;
    }
    pos.min(ix)
}

/// Ctrl+Right: over any spaces, then one group of like characters.
pub fn word_right(text: &str, offset: usize) -> usize {
    let mut chars = text[offset..].char_indices().peekable();
    let mut pos = offset;
    while let Some(&(ix, c)) = chars.peek() {
        if c == '\n' || !c.is_whitespace() {
            break;
        }
        pos = offset + ix + c.len_utf8();
        chars.next();
    }
    let Some(&(ix, first)) = chars.peek() else {
        return pos;
    };
    if first == '\n' {
        return if pos == offset { offset + ix + 1 } else { pos };
    }
    let class = char_class(first);
    for (ix, c) in chars {
        if char_class(c) != class {
            break;
        }
        pos = offset + ix + c.len_utf8();
    }
    pos
}

/// Every case-insensitive occurrence of `query`, non-overlapping. Characters
/// fold one by one, so byte offsets stay true to `text` even where lowercasing
/// changes a character's length (Turkish `İ`).
pub fn find_matches(text: &str, query: &str) -> Vec<Range<usize>> {
    let needle: Vec<char> = query.chars().flat_map(char::to_lowercase).collect();
    if needle.is_empty() {
        return Vec::new();
    }
    let mut matches = Vec::new();
    let mut skip_until = 0;
    for (start, _) in text.char_indices() {
        if start < skip_until {
            continue;
        }
        let mut wanted = needle.iter();
        let mut next = wanted.next();
        let mut end = start;
        for (ix, c) in text[start..].char_indices() {
            if next.is_none() {
                break;
            }
            let mut folded = c.to_lowercase();
            let fits = folded.all(|lower| {
                let hit = next == Some(&lower);
                if hit {
                    next = wanted.next();
                }
                hit
            });
            if !fits {
                break;
            }
            end = start + ix + c.len_utf8();
        }
        if next.is_none() {
            matches.push(start..end);
            skip_until = end;
        }
    }
    matches
}

/// The word (or whitespace or symbol run) around `offset`, for double-click.
pub fn group_at(text: &str, offset: usize) -> Range<usize> {
    let line_start = text[..offset].rfind('\n').map_or(0, |ix| ix + 1);
    let line_end = text[offset..]
        .find('\n')
        .map_or(text.len(), |ix| offset + ix);
    let class = match text[offset..line_end].chars().next() {
        Some(c) => char_class(c),
        None => match text[line_start..offset].chars().next_back() {
            Some(c) => char_class(c),
            None => return offset..offset,
        },
    };
    let start = text[line_start..offset]
        .char_indices()
        .rev()
        .take_while(|(_, c)| char_class(*c) == class)
        .last()
        .map_or(offset, |(ix, _)| line_start + ix);
    let end = text[offset..line_end]
        .char_indices()
        .take_while(|(_, c)| char_class(*c) == class)
        .last()
        .map_or(offset, |(ix, c)| offset + ix + c.len_utf8());
    start..end
}

/// A table read back into cells.
struct Grid<'a> {
    aligns: Vec<TableAlign>,
    /// Which columns spell their left alignment out, as `:---`.
    colons: Vec<bool>,
    /// Rows of trimmed cells, the header first.
    rows: Vec<Vec<&'a str>>,
}

impl<'a> Grid<'a> {
    fn read(text: &'a str, doc: &LiveDocument, block: usize) -> Option<Self> {
        let block = doc.blocks.get(block)?;
        let LiveBlockKind::Table {
            columns,
            rows,
            delimiter,
        } = &block.kind
        else {
            return None;
        };
        let delimiter = &text[doc.lines.get(*delimiter)?.range.clone()];
        let marks = delimiter.split('|').map(str::trim);
        Some(Self {
            aligns: columns.clone(),
            colons: marks
                .filter(|mark| !mark.is_empty())
                .map(|mark| mark.starts_with(':'))
                .collect(),
            rows: rows
                .iter()
                .map(|row| {
                    row.cells
                        .iter()
                        .map(|cell| text[cell.clone()].trim())
                        .collect()
                })
                .collect(),
        })
    }

    /// The table as Obsidian writes it, every column padded to its widest
    /// cell. Returns the text and where the caret sits in cell `target`
    /// (row, column), which must be a blank one.
    fn format(&self, target: (usize, usize)) -> (String, usize) {
        let widths: Vec<usize> = (0..self.aligns.len())
            .map(|column| {
                let cells = self.rows.iter().filter_map(|row| row.get(column));
                cells
                    .map(|cell| cell.chars().count())
                    .max()
                    .unwrap_or(0)
                    .max(MIN_TABLE_COLUMN)
            })
            .collect();
        let mut out = String::new();
        let mut caret = 0;
        let pad = |out: &mut String, count: usize| out.extend(std::iter::repeat_n(' ', count));
        for (row_ix, row) in self.rows.iter().enumerate() {
            if row_ix > 0 {
                out.push('\n');
            }
            out.push('|');
            for (column, width) in widths.iter().enumerate() {
                out.push(' ');
                if (row_ix, column) == target {
                    caret = out.len();
                }
                let cell = row.get(column).copied().unwrap_or("");
                let free = width - cell.chars().count();
                let left = match self.aligns[column] {
                    TableAlign::Left => 0,
                    TableAlign::Center => free / 2,
                    TableAlign::Right => free,
                };
                pad(&mut out, left);
                out.push_str(cell);
                pad(&mut out, free - left);
                out.push_str(" |");
            }
            if row_ix == 0 {
                out.push_str("\n|");
                for (column, width) in widths.iter().enumerate() {
                    let spelled = self.colons.get(column).copied().unwrap_or(false);
                    let (open, close) = match self.aligns[column] {
                        TableAlign::Left if spelled => (":", ""),
                        TableAlign::Left => ("", ""),
                        TableAlign::Center => (":", ":"),
                        TableAlign::Right => ("", ":"),
                    };
                    out.push(' ');
                    out.push_str(open);
                    let dashes = width - open.len() - close.len();
                    out.extend(std::iter::repeat_n('-', dashes));
                    out.push_str(close);
                    out.push_str(" |");
                }
            }
        }
        (out, caret)
    }
}

/// The whole table with the formatted `grid` in its place, the caret at
/// `caret` inside it.
fn replace_table(
    text: &str,
    doc: &LiveDocument,
    block: usize,
    formatted: &str,
    caret: usize,
) -> Option<Change> {
    let range = doc.blocks.get(block)?.range.clone();
    Some(Change {
        selection: Selection::caret(range.start + caret),
        text: splice(text, range, formatted),
        closed: None,
    })
}

/// Adds a blank row under the table; the caret lands in its first cell.
pub fn table_add_row(text: &str, doc: &LiveDocument, block: usize) -> Option<Change> {
    let mut grid = Grid::read(text, doc, block)?;
    grid.rows.push(Vec::new());
    let (formatted, caret) = grid.format((grid.rows.len() - 1, 0));
    replace_table(text, doc, block, &formatted, caret)
}

/// Adds a blank column on the table's right; the caret lands in its header.
pub fn table_add_column(text: &str, doc: &LiveDocument, block: usize) -> Option<Change> {
    let mut grid = Grid::read(text, doc, block)?;
    grid.aligns.push(TableAlign::Left);
    let (formatted, caret) = grid.format((0, grid.aligns.len() - 1));
    replace_table(text, doc, block, &formatted, caret)
}

/// A new property's line before the closing fence at `fence`, with the caret
/// on its empty key.
pub fn add_property(text: &str, fence: usize) -> Change {
    Change {
        text: splice(text, fence..fence, ": \n"),
        selection: Selection::caret(fence),
        closed: None,
    }
}

/// An empty item in a list property, with the caret in it: after item `after`,
/// or last. `key: value` and `key: [a, b]` hold one item at best, so they turn
/// into `- item` lines first.
pub fn property_add_item(
    text: &str,
    doc: &LiveDocument,
    property: &Property,
    after: Option<usize>,
) -> Change {
    let listed = property
        .items
        .first()
        .is_some_and(|item| item.line != property.line);
    if listed {
        let anchor = after
            .and_then(|ix| property.items.get(ix))
            .or(property.items.last())
            .expect("a listed property has items");
        let line = &doc.lines[anchor.line];
        let indent = leading_whitespace(&text[line.range.clone()]);
        let inserted = format!("\n{indent}- ");
        let at = line.range.end;
        return Change {
            text: splice(text, at..at, &inserted),
            selection: Selection::caret(at + inserted.len()),
            closed: None,
        };
    }
    let line = &doc.lines[property.line];
    let after_colon = property.key.end + 1;
    let mut inserted = String::new();
    if property.items.is_empty() {
        let scalar = text[after_colon..line.range.end].trim();
        if !scalar.is_empty() {
            inserted = format!("\n{ITEM_PREFIX}{scalar}");
        }
    }
    for item in &property.items {
        inserted.push('\n');
        inserted.push_str(ITEM_PREFIX);
        inserted.push_str(&text[item.text.clone()]);
    }
    inserted.push('\n');
    inserted.push_str(ITEM_PREFIX);
    Change {
        text: splice(text, after_colon..line.range.end, &inserted),
        selection: Selection::caret(after_colon + inserted.len()),
        closed: None,
    }
}

/// `range` gone from `text`, with `selection` following.
fn cut_range(text: &str, range: Range<usize>, selection: Selection) -> Change {
    let shift = |at: usize| {
        if at >= range.end {
            at - range.len()
        } else {
            at.min(range.start)
        }
    };
    Change {
        selection: selection.map(shift),
        text: splice(text, range, ""),
        closed: None,
    }
}

/// Takes item `item` out of a list property; `None` clears a lone scalar.
pub fn property_remove_item(
    text: &str,
    doc: &LiveDocument,
    property: &Property,
    item: Option<usize>,
    selection: Selection,
) -> Change {
    let line = &doc.lines[property.line];
    let after_colon = property.key.end + 1;
    let Some(item) = item.and_then(|ix| property.items.get(ix).map(|found| (ix, found))) else {
        return cut_range(text, after_colon..line.range.end, selection);
    };
    let (ix, found) = item;
    if found.line != property.line {
        // the line and the break before it
        let range = doc.lines[found.line - 1].range.end..doc.lines[found.line].range.end;
        return cut_range(text, range, selection);
    }
    let kept: Vec<&str> = property
        .items
        .iter()
        .enumerate()
        .filter(|(at, _)| *at != ix)
        .map(|(_, kept)| &text[kept.text.clone()])
        .collect();
    let rewritten = format!(" [{}]", kept.join(", "));
    let range = after_colon..line.range.end;
    let end = after_colon + rewritten.len();
    Change {
        selection: selection.map(|at| at.min(end)),
        text: splice(text, range, &rewritten),
        closed: None,
    }
}

/// Takes a whole property out, its item lines with it.
pub fn remove_property(
    text: &str,
    doc: &LiveDocument,
    property: &Property,
    selection: Selection,
) -> Change {
    let last = property
        .items
        .last()
        .map_or(property.line, |item| item.line);
    let end = doc
        .lines
        .get(last + 1)
        .map_or(doc.lines[last].range.end, |next| next.range.start);
    cut_range(text, doc.lines[property.line].range.start..end, selection)
}

/// Flips a checkbox property's `true` or `false` at `range`.
pub fn toggle_property(text: &str, range: Range<usize>, selection: Selection) -> Change {
    let flipped = if text[range.clone()] == *"true" {
        "false"
    } else {
        "true"
    };
    let delta = flipped.len().cast_signed() - range.len().cast_signed();
    let shift = |at: usize| {
        if at >= range.end {
            at.saturating_add_signed(delta)
        } else {
            at
        }
    };
    Change {
        selection: selection.map(shift),
        text: splice(text, range.clone(), flipped),
        closed: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `a|b` is a caret, `|a|` a forward selection.
    fn parse_case(case: &str) -> (String, Selection) {
        let first = case.find('|').expect("a caret marker");
        let second = case[first + 1..].find('|').map(|ix| first + ix);
        let text = case.replace('|', "");
        let selection = Selection {
            anchor: first,
            head: second.unwrap_or(first),
        };
        (text, selection)
    }

    fn show(change: &Change) -> String {
        let range = change.selection.range();
        if range.is_empty() {
            format!(
                "{}|{}",
                &change.text[..range.start],
                &change.text[range.end..]
            )
        } else {
            format!(
                "{}|{}|{}",
                &change.text[..range.start],
                &change.text[range.clone()],
                &change.text[range.end..]
            )
        }
    }

    fn check(op: impl Fn(&str, Selection) -> Change, cases: &[(&str, &str)]) {
        for (input, want) in cases {
            let (text, selection) = parse_case(input);
            assert_eq!(&show(&op(&text, selection)), want, "input {input:?}");
        }
    }

    #[test]
    fn enter_matches_obsidian() {
        check(
            enter,
            &[
                ("- item|", "- item\n- |"),
                ("- item\n- |", "- item\n|"),
                ("\t- item|", "\t- item\n\t- |"),
                ("\t- |", "- |"),
                ("- it|em", "- it\n- |em"),
                ("- [ ] task|", "- [ ] task\n- [ ] |"),
                ("- [x] task|", "- [x] task\n- [ ] |"),
                ("- [ ] |", "|"),
                ("1. a|\n2. b\n3. c", "1. a\n2. |\n3. b\n4. c"),
                ("1. |", "|"),
                ("1) one|", "1) one\n2) |"),
                ("* star|", "* star\n* |"),
                ("+ plus|", "+ plus\n+ |"),
                ("> quote|", "> quote\n> |"),
                ("> |", "\n|"),
                ("> a\n> |\n> b", "> a\n\n|\n> b"),
                ("> > deep|", "> > deep\n> > |"),
                ("> - in quote|", "> - in quote\n> - |"),
                ("# Title|", "# Title\n|"),
                ("# Ti|tle", "# Ti\n|tle"),
                ("```\n  code|\n```", "```\n  code\n  |\n```"),
                ("  para|", "  para\n  |"),
                ("|- item", "\n|- item"),
                ("-| item", "-\n|item"),
            ],
        );
    }

    #[test]
    fn soft_break_matches_obsidian() {
        check(
            soft_break,
            &[
                ("para|", "para\n|"),
                ("- item|", "- item\n  |"),
                ("1. a|", "1. a\n   |"),
                ("- [ ] a|", "- [ ] a\n      |"),
                ("> a|", "> a\n> |"),
                ("\t- a|", "\t- a\n\t  |"),
            ],
        );
    }

    #[test]
    fn tab_matches_obsidian() {
        check(
            indent,
            &[
                ("- item|", "\t- item|"),
                ("- a\n- item|", "- a\n\t- item|"),
                ("- a\n- it|em", "- a\n\t- it|em"),
                ("1. a\n2. b|", "1. a\n\t1. b|"),
                ("1. a\n2. b|\n3. c", "1. a\n\t1. b|\n2. c"),
                ("para|", "\tpara|"),
                ("pa|ra", "\tpa|ra"),
                ("|a\nb|", "\t|a\n\tb|"),
                ("- |a\n- b|", "\t- |a\n\t- b|"),
            ],
        );
    }

    #[test]
    fn shift_tab_matches_obsidian() {
        check(
            outdent,
            &[
                ("- a\n\t- item|", "- a\n- item|"),
                ("- it|em\n- next", "- it|em\n- next"),
                ("pa|ra", "pa|ra"),
                ("\tpa|ra", "pa|ra"),
                ("1. a\n\t1. b|\n2. c", "1. a\n2. b|\n3. c"),
            ],
        );
    }

    fn typed(input: &str, keys: &str) -> String {
        let (mut text, mut selection) = parse_case(input);
        let mut closed: Vec<usize> = Vec::new();
        for ch in keys.chars() {
            let change = type_char(&text, selection, ch, &closed).unwrap_or_else(|| {
                let range = selection.range();
                let caret = range.start + ch.len_utf8();
                Change::replace(&text, range, ch.encode_utf8(&mut [0; 4]), caret)
            });
            let (edit, inserted) = diff(&text, &change.text);
            map_closed(&mut closed, &edit, inserted);
            if let Some(at) = change.closed {
                closed.push(at);
            }
            text = change.text;
            selection = change.selection;
        }
        show(&Change::new(text, selection))
    }

    #[test]
    fn auto_pairs_match_obsidian() {
        for (input, keys, want) in [
            ("|", "(", "(|)"),
            ("|", "(a)", "(a)|"),
            ("|", "[", "[|]"),
            ("|", "[[", "[[|]]"),
            ("|", "{", "{|}"),
            ("|", "\"", "\"|\""),
            ("|", "'", "'|'"),
            ("|", "`", "`|`"),
            ("|", "*", "*|*"),
            ("|", "**", "**|"),
            ("|", "***", "***|"),
            ("|", "___", "___|"),
            ("|", "```", "```|\n```"),
            ("a *x*|", "*", "a *x**|"),
            ("\"a\"|", "\"", "\"a\"\"|"),
            ("''|", "'", "'''|"),
            ("|", "_", "_|_"),
            ("|", "~", "~|"),
            ("|", "=", "=|"),
            ("|", "$", "$|"),
            ("it|", "'", "it'|"),
            ("a |b", "*", "a *|b"),
            ("a|b", "*", "a*|b"),
            ("a |", "**", "a **|"),
            ("a |", "**x", "a **x|"),
            ("a |", "**x**", "a **x**|"),
            ("a |", "`x`", "a `x`|"),
            ("a |", "[[x]]", "a [[x]]|"),
            ("foo(|", ")", "foo()|"),
            ("- [ ]|", "x", "- [ ]x|"),
            // a closer nobody typed for us is doubled
            ("(|)", ")", "()|)"),
            ("```\n|\n```", "*", "```\n*|*\n```"),
            ("```\n|\n```", "(", "```\n(|)\n```"),
        ] {
            assert_eq!(typed(input, keys), want, "typing {keys:?} into {input:?}");
        }
    }

    #[test]
    fn selections_wrap_like_obsidian() {
        for (input, key, want) in [
            ("x|y|", "*", "x*|y|*"),
            ("|word|", "*", "*|word|*"),
            ("|word|", "_", "_|word|_"),
            ("|word|", "`", "`|word|`"),
            ("|word|", "~", "~|word|~"),
            ("|word|", "=", "=|word|="),
            ("|word|", "(", "(|word|)"),
            ("|word|", "[", "[|word|]"),
            ("|word|", "\"", "\"|word|\""),
            ("|word|", "$", "$|word|$"),
        ] {
            assert_eq!(typed(input, key), want, "typing {key:?} into {input:?}");
        }
    }

    #[test]
    fn backspace_deletes_an_empty_pair() {
        for (input, want) in [
            ("(|)", Some("|")),
            ("[|]", Some("|")),
            ("`|`", Some("|")),
            ("*|*", Some("|")),
            ("**|**", Some("*|*")),
            ("[[|]]", Some("[|]")),
            ("a|b", None),
            ("(|", None),
        ] {
            let (text, selection) = parse_case(input);
            let got = delete_pair(&text, selection).map(|c| show(&c));
            assert_eq!(got.as_deref(), want, "input {input:?}");
        }
    }

    #[test]
    fn format_toggles_match_obsidian() {
        let bold = |text: &str, sel| toggle_wrap(text, sel, "**");
        check(
            bold,
            &[
                ("|word|", "**|word|**"),
                ("|", "**|**"),
                ("**|word|**", "|word|"),
                ("a wo|rd b", "a **wo|rd** b"),
                ("**wo|rd**", "wo|rd"),
            ],
        );
        let italic = |text: &str, sel| toggle_wrap(text, sel, "*");
        check(
            italic,
            &[
                ("|word|", "*|word|*"),
                ("|", "*|*"),
                ("*wo|rd*", "wo|rd"),
                ("**wo|rd**", "***wo|rd***"),
            ],
        );
    }

    #[test]
    fn links_tasks_and_line_commands_match_obsidian() {
        check(insert_link, &[("|word|", "[word](|)"), ("|", "[|]()")]);
        check(
            toggle_task_line,
            &[
                ("- [ ] ta|sk", "- [x] ta|sk"),
                ("- [x] a|", "- [ ] a|"),
                ("pa|ra", "- [ ] pa|ra"),
                ("- a|", "- [ ] a|"),
            ],
        );
        // Obsidian keeps the caret's pixel column here; offsets stand in for it
        check(delete_line, &[("line|\nnext", "next|")]);
    }

    #[test]
    fn word_motion_matches_obsidian() {
        for (input, want) in [
            ("hello wor|ld", "hello |world"),
            ("foo-ba|r baz", "foo-|bar baz"),
        ] {
            let (text, selection) = parse_case(input);
            let change = Change::new(
                text.clone(),
                Selection::caret(word_left(&text, selection.head)),
            );
            assert_eq!(show(&change), want, "ctrl+left in {input:?}");
        }
        for (input, want) in [
            ("hello wor|ld foo", "hello world| foo"),
            ("**bo|ld** x", "**bold|** x"),
        ] {
            let (text, selection) = parse_case(input);
            let change = Change::new(
                text.clone(),
                Selection::caret(word_right(&text, selection.head)),
            );
            assert_eq!(show(&change), want, "ctrl+right in {input:?}");
        }
        // the word delete commands walk the same groups
        assert_eq!(word_left("hello wor", 9), 6);
        assert_eq!(word_right("hello world", 5), 11);
    }

    #[test]
    fn word_motion_stops_at_line_ends() {
        assert_eq!(word_right("a\nb", 1), 2);
        assert_eq!(word_left("a\nb", 2), 1);
        assert_eq!(word_left("a  \nb", 3), 0);
    }

    #[test]
    fn grapheme_steps_keep_clusters_whole() {
        let text = "ae\u{301}🎉";
        assert_eq!(next_grapheme(text, 0), 1);
        assert_eq!(next_grapheme(text, 1), 4);
        assert_eq!(next_grapheme(text, 4), 8);
        assert_eq!(prev_grapheme(text, 8), 4);
        assert_eq!(prev_grapheme(text, 4), 1);
        assert_eq!(prev_grapheme(text, 0), 0);
    }

    #[test]
    fn find_folds_case_and_keeps_true_offsets() {
        assert_eq!(find_matches("foo Foo FOO", "foo"), [0..3, 4..7, 8..11]);
        assert_eq!(find_matches("aaaa", "aa"), [0..2, 2..4]);
        assert_eq!(find_matches("x", ""), Vec::<Range<usize>>::new());
        assert_eq!(find_matches("x", "xy"), Vec::<Range<usize>>::new());
        // Turkish: dotted capital İ folds to two chars and is 2 bytes
        assert_eq!(
            find_matches("İstanbul", "i\u{307}stanbul").first(),
            Some(&(0..9))
        );
        assert_eq!(find_matches("Ğüneş ğüneş", "ĞÜNEŞ"), [0..8, 9..17]);
    }

    #[test]
    fn diff_finds_the_smallest_edit_on_char_boundaries() {
        assert_eq!(diff("abc", "abc"), (3..3, 0));
        assert_eq!(diff("abc", "axc"), (1..2, 1));
        assert_eq!(diff("abc", "abXYc"), (2..2, 2));
        assert_eq!(diff("abc", "c"), (0..2, 0));
        // é and è share their first byte
        assert_eq!(diff("é", "è"), (0..2, 2));
    }

    #[test]
    fn double_click_groups() {
        let text = "foo bar.baz  x";
        assert_eq!(group_at(text, 5), 4..7);
        assert_eq!(group_at(text, 7), 7..8);
        assert_eq!(group_at(text, 12), 11..13);
        assert_eq!(group_at("", 0), 0..0);
    }

    /// `blocks` of a note, by its only block of each kind.
    fn block_of(doc: &LiveDocument, want: impl Fn(&LiveBlockKind) -> bool) -> usize {
        doc.blocks.iter().position(|b| want(&b.kind)).unwrap()
    }

    fn table_case(op: impl Fn(&str, &LiveDocument, usize) -> Option<Change>, text: &str) -> String {
        let doc = crate::live::parse_document(text);
        let block = block_of(&doc, |kind| matches!(kind, LiveBlockKind::Table { .. }));
        show(&op(text, &doc, block).unwrap())
    }

    #[test]
    fn adding_a_row_formats_the_table_like_obsidian() {
        assert_eq!(
            table_case(table_add_row, "|a|b|\n|-|-|\n|1|2|\n\nz\n"),
            "| a   | b   |\n| --- | --- |\n| 1   | 2   |\n| |    |     |\n\nz\n"
        );
    }

    #[test]
    fn adding_a_column_keeps_alignments_and_their_padding() {
        let text = "x\n\n| aa | bbbbbb | c |\n| :-- | --: | :-: |\n| 1 | 2 | 3 |\n| longer | y | zzzzz |\n\nz\n";
        // Obsidian's result, byte for byte
        assert_eq!(
            table_case(table_add_column, text),
            "x\n\n| aa     | bbbbbb |   c   | |    |\n| :----- | -----: | :---: | --- |\n| 1      |      2 |   3   |     |\n| longer |      y | zzzzz |     |\n\nz\n"
        );
    }

    #[test]
    fn a_new_cell_holds_the_caret_where_the_parser_puts_it() {
        let text = "| a | b |\n| - | - |\n| 1 | 2 |";
        let doc = crate::live::parse_document(text);
        let block = block_of(&doc, |kind| matches!(kind, LiveBlockKind::Table { .. }));
        let change = table_add_row(text, &doc, block).unwrap();
        let after = crate::live::parse_document(&change.text);
        let LiveBlockKind::Table { rows, .. } = &after.blocks[block].kind else {
            panic!("still a table");
        };
        assert_eq!(rows.last().unwrap().cells[0].start, change.selection.head);
    }

    #[test]
    fn a_short_row_grows_to_the_table() {
        assert_eq!(
            table_case(table_add_column, "| a | b |\n| - | - |\n| 1 |"),
            "| a   | b   | |    |\n| --- | --- | --- |\n| 1   |     |     |"
        );
    }

    fn property_case(text: &str, line: usize) -> (LiveDocument, Property) {
        let doc = crate::live::parse_document(text);
        let block = block_of(&doc, |kind| {
            matches!(kind, LiveBlockKind::Frontmatter { .. })
        });
        let LiveBlockKind::Frontmatter { properties } = &doc.blocks[block].kind else {
            panic!("a frontmatter block");
        };
        let found = properties.iter().find(|p| p.line == line).unwrap().clone();
        (doc, found)
    }

    #[test]
    fn a_new_property_goes_last_with_the_caret_on_its_key() {
        let text = "---\na: 1\n---\n";
        assert_eq!(show(&add_property(text, 9)), "---\na: 1\n|: \n---\n");
    }

    #[test]
    fn list_items_join_the_lines_they_follow() {
        let text = "---\ntags:\n    - a\n    - b\n---\n";
        let (doc, property) = property_case(text, 1);
        assert_eq!(
            show(&property_add_item(text, &doc, &property, None)),
            "---\ntags:\n    - a\n    - b\n    - |\n---\n"
        );
        assert_eq!(
            show(&property_add_item(text, &doc, &property, Some(0))),
            "---\ntags:\n    - a\n    - |\n    - b\n---\n"
        );
    }

    #[test]
    fn other_list_shapes_become_lines_for_a_second_item() {
        for (text, want) in [
            ("---\ntags:\n---\n", "---\ntags:\n  - |\n---\n"),
            ("---\ntags: \n---\n", "---\ntags:\n  - |\n---\n"),
            ("---\ntags: one\n---\n", "---\ntags:\n  - one\n  - |\n---\n"),
            (
                "---\ntags: [a, b]\n---\n",
                "---\ntags:\n  - a\n  - b\n  - |\n---\n",
            ),
        ] {
            let (doc, property) = property_case(text, 1);
            assert_eq!(show(&property_add_item(text, &doc, &property, None)), want);
        }
    }

    #[test]
    fn items_leave_with_their_line() {
        let text = "---\ntags:\n  - a\n  - b\n---\nbody";
        let (doc, property) = property_case(text, 1);
        let caret = Selection::caret(text.len());
        let change = property_remove_item(text, &doc, &property, Some(0), caret);
        assert_eq!(show(&change), "---\ntags:\n  - b\n---\nbody|");
    }

    #[test]
    fn inline_items_stay_inline_when_one_leaves() {
        let text = "---\ntags: [a, b, c]\n---\n";
        let (doc, property) = property_case(text, 1);
        let change = property_remove_item(text, &doc, &property, Some(1), Selection::caret(0));
        assert_eq!(show(&change), "|---\ntags: [a, c]\n---\n");
    }

    #[test]
    fn a_lone_scalar_tag_clears_its_value() {
        let text = "---\ntags: one\n---\n";
        let (doc, property) = property_case(text, 1);
        let change = property_remove_item(text, &doc, &property, None, Selection::caret(0));
        assert_eq!(show(&change), "|---\ntags:\n---\n");
    }

    #[test]
    fn a_property_leaves_with_its_items() {
        let text = "---\na: 1\ntags:\n  - x\nb: 2\n---\n";
        let (doc, property) = property_case(text, 2);
        let change = remove_property(text, &doc, &property, Selection::caret(text.len()));
        assert_eq!(show(&change), "---\na: 1\nb: 2\n---\n|");
    }

    #[test]
    fn checkboxes_flip_and_the_selection_follows() {
        let text = "---\na: true\nb: 1\n---\n";
        let change = toggle_property(text, 7..11, Selection::caret(text.len()));
        assert_eq!(show(&change), "---\na: false\nb: 1\n---\n|");
        let text = "---\na: false\n---\n";
        let change = toggle_property(text, 7..12, Selection::caret(4));
        assert_eq!(show(&change), "---\n|a: true\n---\n");
    }
}
