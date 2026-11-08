//! Unified patch parsing: one pass from patch text to files, hunks and rows.
//!
//! The renderer draws from this document instead of re-classifying lines. The
//! rules the parser enforces are the ones the old row walk got wrong:
//!
//! - A unified diff's first column is a marker column. Context lines begin
//!   with a space, additions with `+`, deletions with `-`; every row's `text`
//!   is the line *without* that marker, so all three kinds share one code
//!   column.
//! - Old and new line numbers are tracked separately; a context row carries
//!   both, a deletion only the old one, an addition only the new one.
//! - Deletions and additions inside a hunk are paired by position and get a
//!   word-level emphasis, so a changed identifier can stand out from its row's
//!   wash.
//!
//! Parsing is pure: no `Window`, no `App`, no theme. The renderer adds token
//! colors and washes on top.

use std::ops::Range;

use similar::{ChangeTag, TextDiff};

/// The kind of a diff row. Metadata and hunk headers are structure, not rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowKind {
    Context,
    Add,
    Del,
}

/// One code row in a hunk.
#[derive(Clone, Debug, PartialEq)]
pub struct DiffRow {
    pub kind: RowKind,
    /// Old-file line number: `Some` for context and deletions.
    pub old: Option<u32>,
    /// New-file line number: `Some` for context and additions.
    pub new: Option<u32>,
    /// The line with the marker column removed.
    pub text: String,
    /// Byte ranges in `text` that changed inside a paired line.
    pub emphasis: Vec<Range<usize>>,
}

impl DiffRow {
    /// The new-file number the gutter shows. A deletion has no line in the
    /// new file, so it carries no number — its old-file number would point at
    /// a line the file no longer has.
    pub fn number(&self) -> Option<u32> {
        match self.kind {
            RowKind::Context | RowKind::Add => self.new,
            RowKind::Del => None,
        }
    }
}

/// One row of the split view: indices into [`DiffHunk::rows`] for each side.
/// A context row appears on both sides; a deletion only on the left and an
/// addition only on the right. An unpaired change leaves one side `None`; the
/// renderer draws a blank filler there so the two columns stay aligned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SplitRow {
    pub left: Option<usize>,
    pub right: Option<usize>,
}

/// One `@@ ... @@` hunk.
#[derive(Clone, Debug, PartialEq)]
pub struct DiffHunk {
    /// The header line as it appears in the patch.
    pub header: String,
    pub old_start: u32,
    pub new_start: u32,
    /// New-file line count the header declares.
    ///
    /// The renderer never draws the header; this is parsed for callers that
    /// navigate hunks and for the parser's own tests.
    pub new_count: u32,
    pub rows: Vec<DiffRow>,
}

impl DiffHunk {
    /// Pair a hunk for the split view: context on both sides, a change block's
    /// deletions and additions by position, leftovers on one side.
    pub fn split_rows(&self) -> Vec<SplitRow> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < self.rows.len() {
            if self.rows[i].kind == RowKind::Context {
                out.push(SplitRow {
                    left: Some(i),
                    right: Some(i),
                });
                i += 1;
                continue;
            }
            let (dels, adds, next) = change_block(&self.rows, i);
            for k in 0..dels.len().max(adds.len()) {
                out.push(SplitRow {
                    left: dels.get(k).copied(),
                    right: adds.get(k).copied(),
                });
            }
            i = next;
        }
        out
    }
}

/// One file section of a patch.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DiffFile {
    pub old_path: Option<String>,
    pub new_path: Option<String>,
    pub added: u32,
    pub removed: u32,
    pub binary: bool,
    pub truncated: bool,
    pub hunks: Vec<DiffHunk>,
}

impl DiffFile {
    /// The path the header shows: the new side, or the old one for a deletion.
    pub fn path(&self) -> Option<&str> {
        self.new_path.as_deref().or(self.old_path.as_deref())
    }

    /// A rename or mode-only change the renderer states in one line.
    pub fn rename(&self) -> Option<(&str, &str)> {
        match (&self.old_path, &self.new_path) {
            (Some(old), Some(new)) if old != new => Some((old, new)),
            _ => None,
        }
    }
}

/// A parsed patch.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DiffDocument {
    pub files: Vec<DiffFile>,
    pub truncated: bool,
}

impl DiffDocument {
    /// Parse a unified patch. Unknown metadata is ignored; the rows a hunk
    /// carries are kept even when the header is malformed.
    pub fn parse(text: &str) -> Self {
        let normalized = text.replace('\r', "");
        let mut doc = Self::default();
        let lines: Vec<&str> = normalized.split('\n').collect();
        let mut current: Option<DiffFile> = None;
        let mut in_hunk = false;
        let mut old = 1u32;
        let mut new = 1u32;
        let mut i = 0;

        while i < lines.len() {
            let line = lines[i];
            if line.is_empty() {
                // a trailing newline terminates the last line, it is not a row
                i += 1;
                continue;
            }
            if let Some(rest) = line.strip_prefix("diff --git ") {
                flush(&mut doc, &mut current);
                let mut file = DiffFile::default();
                if let Some((old_path, new_path)) = split_git_paths(rest) {
                    file.old_path = Some(old_path.to_string());
                    file.new_path = Some(new_path.to_string());
                }
                current = Some(file);
                in_hunk = false;
                i += 1;
                continue;
            }
            // A file header is the `---`/`+++` pair, not any line that starts
            // with `---`: inside a hunk that prefix is a deleted line whose
            // content begins with `--`.
            if line.starts_with("--- ")
                && lines
                    .get(i + 1)
                    .is_some_and(|next| next.starts_with("+++ "))
            {
                if current.as_ref().is_none_or(|file| !file.hunks.is_empty()) {
                    flush(&mut doc, &mut current);
                }
                let file = current.get_or_insert_with(DiffFile::default);
                file.old_path = header_path(&line[4..]).map(str::to_string);
                let plus = lines[i + 1];
                file.new_path = header_path(&plus[4..]).map(str::to_string);
                in_hunk = false;
                i += 2;
                continue;
            }
            if line.starts_with("@@") {
                let file = current.get_or_insert_with(DiffFile::default);
                let (old_start, new_start, new_count) =
                    parse_hunk_header(line).unwrap_or((1, 1, 1));
                file.hunks.push(DiffHunk {
                    header: line.trim_end().to_string(),
                    old_start,
                    new_start,
                    new_count,
                    rows: Vec::new(),
                });
                old = old_start;
                new = new_start;
                in_hunk = true;
                i += 1;
                continue;
            }
            if line.starts_with('\\') {
                // "\ No newline at end of file" is a marker for the row above
                i += 1;
                continue;
            }
            if line.contains("(patch truncated)") {
                doc.truncated = true;
                if let Some(file) = current.as_mut() {
                    file.truncated = true;
                }
                in_hunk = false;
                i += 1;
                continue;
            }
            if in_hunk {
                let file = current.get_or_insert_with(DiffFile::default);
                let Some(hunk) = file.hunks.last_mut() else {
                    i += 1;
                    continue;
                };
                let (kind, marker) = match line.as_bytes()[0] {
                    b'+' => (RowKind::Add, 1),
                    b'-' => (RowKind::Del, 1),
                    b' ' => (RowKind::Context, 1),
                    // a malformed row: keep its text, but do not invent a marker
                    _ => (RowKind::Context, 0),
                };
                let text = line[marker..].to_string();
                match kind {
                    RowKind::Context => {
                        hunk.rows.push(DiffRow {
                            kind,
                            old: Some(old),
                            new: Some(new),
                            text,
                            emphasis: Vec::new(),
                        });
                        old += 1;
                        new += 1;
                    }
                    RowKind::Add => {
                        hunk.rows.push(DiffRow {
                            kind,
                            old: None,
                            new: Some(new),
                            text,
                            emphasis: Vec::new(),
                        });
                        file.added += 1;
                        new += 1;
                    }
                    RowKind::Del => {
                        hunk.rows.push(DiffRow {
                            kind,
                            old: Some(old),
                            new: None,
                            text,
                            emphasis: Vec::new(),
                        });
                        file.removed += 1;
                        old += 1;
                    }
                }
                i += 1;
                continue;
            }
            if line.starts_with("Binary files ") || line.starts_with("GIT binary patch") {
                let file = current.get_or_insert_with(DiffFile::default);
                file.binary = true;
                i += 1;
                continue;
            }
            if let Some(rest) = line.strip_prefix("rename from ") {
                let file = current.get_or_insert_with(DiffFile::default);
                file.old_path = Some(rest.to_string());
            } else if let Some(rest) = line.strip_prefix("rename to ") {
                let file = current.get_or_insert_with(DiffFile::default);
                file.new_path = Some(rest.to_string());
            }
            i += 1;
        }
        flush(&mut doc, &mut current);

        for file in &mut doc.files {
            for hunk in &mut file.hunks {
                emphasize_hunk(hunk);
            }
        }
        doc
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
            || self
                .files
                .iter()
                .all(|file| file.hunks.is_empty() && !file.binary)
    }

    pub fn added(&self) -> u32 {
        self.files.iter().map(|file| file.added).sum()
    }

    pub fn removed(&self) -> u32 {
        self.files.iter().map(|file| file.removed).sum()
    }

    /// The largest line number any row carries, for the gutter width.
    pub fn max_number(&self) -> u32 {
        self.files
            .iter()
            .flat_map(|file| &file.hunks)
            .flat_map(|hunk| &hunk.rows)
            .filter_map(DiffRow::number)
            .max()
            .unwrap_or(0)
    }

    /// The largest old-file number any row carries, for split's left gutter.
    pub fn max_old_number(&self) -> u32 {
        self.files
            .iter()
            .flat_map(|file| &file.hunks)
            .flat_map(|hunk| &hunk.rows)
            .filter_map(|row| row.old)
            .max()
            .unwrap_or(0)
    }

    /// The language of the first file that names one.
    pub fn language(&self, path_hint: Option<&str>) -> Option<&'static str> {
        self.files
            .iter()
            .find_map(|file| file.path().and_then(lang_from_path))
            .or_else(|| path_hint.and_then(lang_from_path))
    }
}

fn flush(doc: &mut DiffDocument, current: &mut Option<DiffFile>) {
    if let Some(file) = current.take()
        && (file.old_path.is_some()
            || file.new_path.is_some()
            || !file.hunks.is_empty()
            || file.binary)
    {
        doc.files.push(file);
    }
}

/// `diff --git a/<old> b/<new>`. Uses the last ` b/` so paths with spaces
/// survive; git quotes unusual names, which is out of scope here.
fn split_git_paths(rest: &str) -> Option<(&str, &str)> {
    let rest = rest.strip_prefix("a/")?;
    let split = rest.rfind(" b/")?;
    Some((&rest[..split], &rest[split + 3..]))
}

/// The path on a `---`/`+++` line: the a/ or b/ prefix and any timestamp tail
/// are removed; `/dev/null` is the absent side.
fn header_path(rest: &str) -> Option<&str> {
    let path = rest.split('\t').next()?.trim();
    if path == "/dev/null" {
        return None;
    }
    Some(
        path.strip_prefix("a/")
            .or_else(|| path.strip_prefix("b/"))
            .unwrap_or(path),
    )
}

/// `@@ -old[,n] +new[,n] @@ ...`.
fn parse_hunk_header(line: &str) -> Option<(u32, u32, u32)> {
    let rest = line.strip_prefix("@@ -")?;
    let plus = rest.find(" +")?;
    let old = rest[..plus].split(',').next()?.trim().parse().ok()?;
    let tail = &rest[plus + 2..];
    let end = tail.find(" @@")?;
    let mut numbers = tail[..end].split(',');
    let new = numbers.next()?.trim().parse().ok()?;
    let count = numbers
        .next()
        .and_then(|count| count.trim().parse().ok())
        .unwrap_or(1);
    Some((old, new, count))
}

/// The deletion and addition indices of the change block starting at `start`
/// (a non-context row), plus the index just past the block. Git's usual shape
/// is all deletions then all additions, but collecting each kind separately
/// keeps a malformed block from pairing the wrong rows.
fn change_block(rows: &[DiffRow], start: usize) -> (Vec<usize>, Vec<usize>, usize) {
    let mut dels = Vec::new();
    let mut adds = Vec::new();
    let mut i = start;
    while i < rows.len() && rows[i].kind != RowKind::Context {
        match rows[i].kind {
            RowKind::Del => dels.push(i),
            RowKind::Add => adds.push(i),
            RowKind::Context => unreachable!("the loop stops at context"),
        }
        i += 1;
    }
    (dels, adds, i)
}

/// Pairs deletion and addition runs inside a hunk and marks the changed words
/// on both sides. Pairing follows [`DiffHunk::split_rows`], so the emphasis and
/// the split columns always name the same rows.
fn emphasize_hunk(hunk: &mut DiffHunk) {
    let mut pairs = Vec::new();
    let mut i = 0;
    while i < hunk.rows.len() {
        if hunk.rows[i].kind == RowKind::Context {
            i += 1;
            continue;
        }
        let (dels, adds, next) = change_block(&hunk.rows, i);
        for offset in 0..dels.len().min(adds.len()) {
            let (old_ranges, new_ranges) =
                word_emphasis(&hunk.rows[dels[offset]].text, &hunk.rows[adds[offset]].text);
            pairs.push((dels[offset], adds[offset], old_ranges, new_ranges));
        }
        i = next;
    }
    for (del, add, old_ranges, new_ranges) in pairs {
        hunk.rows[del].emphasis = old_ranges;
        hunk.rows[add].emphasis = new_ranges;
    }
}

/// Word-level changed spans for a deletion/addition pair. Adjacent spans are
/// merged; a span that covers most of the line is dropped, because the row's
/// own wash already carries it and the emphasis would just brighten the line.
fn word_emphasis(old: &str, new: &str) -> (Vec<Range<usize>>, Vec<Range<usize>>) {
    let diff = TextDiff::from_words(old, new);
    let mut old_ranges = Vec::new();
    let mut new_ranges = Vec::new();
    let mut old_offset = 0;
    let mut new_offset = 0;
    for change in diff.iter_all_changes() {
        let len = change.value().len();
        match change.tag() {
            ChangeTag::Equal => {
                old_offset += len;
                new_offset += len;
            }
            ChangeTag::Delete => {
                old_ranges.push(old_offset..old_offset + len);
                old_offset += len;
            }
            ChangeTag::Insert => {
                new_ranges.push(new_offset..new_offset + len);
                new_offset += len;
            }
        }
    }
    (trim_ranges(old, old_ranges), trim_ranges(new, new_ranges))
}

fn trim_ranges(text: &str, ranges: Vec<Range<usize>>) -> Vec<Range<usize>> {
    let mut merged: Vec<Range<usize>> = Vec::new();
    for range in ranges {
        let Some(range) = trim_span(text, range) else {
            continue;
        };
        if let Some(last) = merged.last_mut()
            && last.end == range.start
        {
            last.end = range.end;
        } else {
            merged.push(range);
        }
    }
    let covered: usize = merged.iter().map(|range| range.len()).sum();
    if text.is_empty() || covered * 5 >= text.len() * 4 {
        return Vec::new();
    }
    merged
}

fn trim_span(text: &str, mut range: Range<usize>) -> Option<Range<usize>> {
    let is_space = |ix: usize| text.as_bytes().get(ix).is_some_and(u8::is_ascii_whitespace);
    while range.start < range.end && is_space(range.start) {
        range.start += 1;
    }
    while range.end > range.start && is_space(range.end - 1) {
        range.end -= 1;
    }
    (range.start < range.end).then_some(range)
}

/// Language names the kit's SyntaxHighlighter accepts, restricted to the
/// grammars the family enables (see Cargo.toml).
pub fn lang_from_path(path: &str) -> Option<&'static str> {
    let name = path.rsplit(['/', '\\']).next()?;
    let ext = name.rsplit_once('.')?.1.to_ascii_lowercase();
    Some(match ext.as_str() {
        "rs" => "rust",
        "js" | "mjs" | "cjs" | "jsx" => "javascript",
        "ts" | "mts" | "cts" => "typescript",
        "tsx" => "tsx",
        "py" | "pyi" => "python",
        "go" => "go",
        "c" | "h" => "c",
        "cpp" | "cc" | "cxx" | "hpp" | "hh" | "hxx" => "cpp",
        "css" | "scss" | "less" => "css",
        "html" | "htm" => "html",
        "json" | "jsonc" => "json",
        "toml" => "toml",
        "yml" | "yaml" => "yaml",
        "sh" | "bash" | "zsh" => "bash",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const PATCH: &str = "\
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -3,4 +3,5 @@ fn a() {
     one
-    two
+    2 + 2
+    three
     four
";

    #[test]
    fn context_rows_lose_the_marker_too() {
        let doc = DiffDocument::parse(PATCH);
        let rows = &doc.files[0].hunks[0].rows;
        assert_eq!(rows[0].text, "    one");
        assert_eq!(rows[1].text, "    two");
        assert_eq!(rows[2].text, "    2 + 2");
        assert_eq!(rows[0].kind, RowKind::Context);
        assert_eq!(rows[0].old, Some(3));
        assert_eq!(rows[0].new, Some(3));
        assert_eq!(rows[1].kind, RowKind::Del);
        assert_eq!(rows[1].old, Some(4));
        assert_eq!(rows[1].new, None);
        assert_eq!(rows[2].kind, RowKind::Add);
        assert_eq!(rows[2].new, Some(4));
        assert_eq!(rows[2].number(), Some(4));
    }

    #[test]
    fn file_counts_and_paths() {
        let doc = DiffDocument::parse(PATCH);
        let file = &doc.files[0];
        assert_eq!(file.path(), Some("src/lib.rs"));
        assert_eq!(file.added, 2);
        assert_eq!(file.removed, 1);
        assert_eq!(doc.added(), 2);
        assert_eq!(doc.removed(), 1);
    }

    #[test]
    fn only_new_file_numbers_are_shown() {
        let doc = DiffDocument::parse(PATCH);
        let hunk = &doc.files[0].hunks[0];
        // the deletion's old number points at a line the file no longer has
        assert_eq!(hunk.rows[1].number(), None);
        assert_eq!(hunk.rows[0].number(), Some(3));
        assert_eq!(hunk.rows[2].number(), Some(4));
        assert_eq!(doc.max_old_number(), 5);
        assert_eq!(doc.max_number(), 6);
    }

    #[test]
    fn split_rows_pair_change_blocks() {
        let doc = DiffDocument::parse("--- a/x\n+++ b/x\n@@ -1,4 +1,3 @@\n a\n-b\n-c\n+x\n d\n");
        let rows = doc.files[0].hunks[0].split_rows();
        assert_eq!(rows.len(), 4);
        assert_eq!(
            rows[0],
            SplitRow {
                left: Some(0),
                right: Some(0)
            }
        );
        assert_eq!(
            rows[1],
            SplitRow {
                left: Some(1),
                right: Some(3)
            }
        );
        assert_eq!(
            rows[2],
            SplitRow {
                left: Some(2),
                right: None
            }
        );
        assert_eq!(
            rows[3],
            SplitRow {
                left: Some(4),
                right: Some(4)
            }
        );
    }

    #[test]
    fn split_rows_keep_a_leading_filler() {
        let doc = DiffDocument::parse("--- a/x\n+++ b/x\n@@ -1,2 +1,4 @@\n a\n+x\n+y\n b\n");
        let rows = doc.files[0].hunks[0].split_rows();
        assert_eq!(rows.len(), 4);
        assert_eq!(
            rows[1],
            SplitRow {
                left: None,
                right: Some(1)
            }
        );
        assert_eq!(
            rows[2],
            SplitRow {
                left: None,
                right: Some(2)
            }
        );
        assert_eq!(
            rows[3],
            SplitRow {
                left: Some(3),
                right: Some(3)
            }
        );
    }

    #[test]
    fn split_rows_keep_an_unpaired_deletion_tail() {
        let doc = DiffDocument::parse("--- a/x\n+++ b/x\n@@ -1,4 +1,2 @@\n a\n-b\n-c\n+x\n d\n");
        let rows = doc.files[0].hunks[0].split_rows();
        assert_eq!(rows.len(), 4);
        assert_eq!(
            rows[2],
            SplitRow {
                left: Some(2),
                right: None
            }
        );
    }

    #[test]
    fn hunk_header_resets_the_counters() {
        let doc = DiffDocument::parse("@@ -41,2 +7,3 @@\n x\n+y\n-z\n@@ -90 +20 @@\n q\n");
        let rows = &doc.files[0].hunks[0].rows;
        assert_eq!(rows[0].old, Some(41));
        assert_eq!(rows[0].new, Some(7));
        assert_eq!(rows[1].new, Some(8));
        assert_eq!(rows[2].old, Some(42));
        let second = &doc.files[0].hunks[1];
        assert_eq!(second.rows[0].old, Some(90));
        assert_eq!(second.rows[0].new, Some(20));
    }

    #[test]
    fn split_paths_and_renames() {
        let doc = DiffDocument::parse(
            "diff --git a/old name.rs b/new name.rs\nsimilarity index 92%\nrename from old name.rs\nrename to new name.rs\n",
        );
        let file = &doc.files[0];
        assert_eq!(file.old_path.as_deref(), Some("old name.rs"));
        assert_eq!(file.new_path.as_deref(), Some("new name.rs"));
        assert_eq!(file.rename(), Some(("old name.rs", "new name.rs")));
    }

    #[test]
    fn binary_and_truncated_are_states() {
        let binary = DiffDocument::parse(
            "diff --git a/x.png b/x.png\nindex 1..2 100644\nBinary files a/x.png and b/x.png differ\n",
        );
        assert!(binary.files[0].binary);
        assert!(!binary.is_empty());

        let truncated =
            DiffDocument::parse("--- a/x\n+++ b/x\n@@ -1 +1 @@\n-a\n+b\n… (patch truncated)\n");
        assert!(truncated.truncated);
        assert!(truncated.files[0].truncated);
        assert_eq!(truncated.files[0].hunks[0].rows.len(), 2);
    }

    #[test]
    fn crlf_and_missing_trailing_newline() {
        let doc = DiffDocument::parse("--- a/x\r\n+++ b/x\r\n@@ -1 +1 @@\r\n-a\r\n+b");
        let rows = &doc.files[0].hunks[0].rows;
        assert_eq!(rows[0].text, "a");
        assert_eq!(rows[1].text, "b");
    }

    #[test]
    fn paired_lines_get_word_emphasis() {
        let doc = DiffDocument::parse(
            "--- a/x\n+++ b/x\n@@ -1 +1 @@\n-let foo_bar = 1;\n+let foo_baz = 1;\n",
        );
        let rows = &doc.files[0].hunks[0].rows;
        assert!(!rows[0].emphasis.is_empty());
        assert!(!rows[1].emphasis.is_empty());
        let old = &rows[0].text[rows[0].emphasis[0].clone()];
        let new = &rows[1].text[rows[1].emphasis[0].clone()];
        assert!(old.contains("bar"), "old emphasis was {old:?}");
        assert!(new.contains("baz"), "new emphasis was {new:?}");
    }

    #[test]
    fn a_fully_rewritten_line_skips_emphasis() {
        let doc = DiffDocument::parse("--- a/x\n+++ b/x\n@@ -1 +1 @@\n-alpha\n+beta\n");
        let rows = &doc.files[0].hunks[0].rows;
        assert!(rows[0].emphasis.is_empty());
        assert!(rows[1].emphasis.is_empty());
    }

    #[test]
    fn language_comes_from_the_patch_or_the_hint() {
        let doc = DiffDocument::parse(PATCH);
        assert_eq!(doc.language(None), Some("rust"));
        assert_eq!(
            DiffDocument::parse("@@ -1 +1 @@\n-a\n+b\n").language(Some("x.py")),
            Some("python")
        );
        assert_eq!(lang_from_path("note.md"), None);
    }
}
