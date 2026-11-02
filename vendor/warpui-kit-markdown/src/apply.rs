//! The toolbar's pure markdown string ops: apply one format over a selection
//! (char indices), returning the new text and selection.

/// The result of one edit: the new text and where the selection lands.
#[derive(Clone, Debug, PartialEq)]
pub struct MarkdownEditResult {
    pub text: String,
    pub start: usize,
    pub length: usize,
}

const PLACEHOLDER: &str = "text";

/// Applies one toolbar format to `text` over `start..start+length` (char
/// indices), returning the new text and selection.
pub fn markdown_apply(format: &str, text: &str, start: usize, length: usize) -> MarkdownEditResult {
    let (start, length) = clamp(text, start, length);
    match format {
        "bold" => wrap_selection(text, start, length, "**"),
        "italic" => wrap_selection(text, start, length, "*"),
        "strike" => wrap_selection(text, start, length, "~~"),
        "code" => wrap_selection(text, start, length, "`"),
        "codeblock" => wrap_code_block(text, start, length),
        "link" => insert_link(text, start, length),
        "h1" => toggle_line_prefix(text, start, length, "# "),
        "h2" => toggle_line_prefix(text, start, length, "## "),
        "h3" => toggle_line_prefix(text, start, length, "### "),
        "bullet" => toggle_line_prefix(text, start, length, "- "),
        "task" => toggle_line_prefix(text, start, length, "- [ ] "),
        "numbered" => toggle_numbered(text, start, length),
        "quote" => toggle_line_prefix(text, start, length, "> "),
        "hr" => insert_rule(text, start),
        _ => MarkdownEditResult {
            text: text.to_string(),
            start,
            length,
        },
    }
}

fn clamp(text: &str, start: usize, length: usize) -> (usize, usize) {
    let count = text.chars().count();
    let start = start.min(count);
    let length = length.min(count - start);
    (start, length)
}

fn char_index(text: &str, char_ix: usize) -> usize {
    text.char_indices()
        .nth(char_ix)
        .map(|(i, _)| i)
        .unwrap_or(text.len())
}

fn wrap_selection(text: &str, start: usize, length: usize, marker: &str) -> MarkdownEditResult {
    let marker_chars = marker.chars().count();
    let b_start = char_index(text, start);
    let b_inner_end = char_index(text, start + length);
    // already wrapped exactly once → unwrap
    let before = text[..b_start].ends_with(marker);
    let after = text[b_inner_end..].starts_with(marker);
    if length >= marker_chars * 2 && before && after {
        let unwrapped = format!(
            "{}{}{}",
            &text[..b_start - marker.len()],
            &text[b_start..b_inner_end],
            &text[b_inner_end + marker.len()..]
        );
        return MarkdownEditResult {
            text: unwrapped,
            start: start - marker_chars,
            length,
        };
    }
    let inner = if length > 0 {
        &text[b_start..b_inner_end]
    } else {
        PLACEHOLDER
    };
    let new_text = format!(
        "{}{marker}{inner}{marker}{}",
        &text[..b_start],
        &text[b_inner_end..]
    );
    MarkdownEditResult {
        text: new_text,
        start: start + marker_chars,
        length: inner.chars().count(),
    }
}

fn wrap_code_block(text: &str, start: usize, length: usize) -> MarkdownEditResult {
    let b_start = char_index(text, start);
    let b_inner_end = char_index(text, start + length);
    if length == 0 {
        let block = format!("```\n{PLACEHOLDER}\n```");
        return MarkdownEditResult {
            text: format!("{}{block}{}", &text[..b_start], &text[b_start..]),
            start: start + 4,
            length: PLACEHOLDER.len(),
        };
    }
    let inner = &text[b_start..b_inner_end];
    MarkdownEditResult {
        text: format!(
            "{}```\n{inner}\n```{}",
            &text[..b_start],
            &text[b_inner_end..]
        ),
        start: start + 4,
        length,
    }
}

fn insert_link(text: &str, start: usize, length: usize) -> MarkdownEditResult {
    let b_start = char_index(text, start);
    let b_inner_end = char_index(text, start + length);
    let label = if length > 0 {
        &text[b_start..b_inner_end]
    } else {
        PLACEHOLDER
    };
    let new_text = format!("{}[{label}](url){}", &text[..b_start], &text[b_inner_end..]);
    MarkdownEditResult {
        text: new_text,
        start: start + label.chars().count() + 3,
        length: 3,
    }
}

fn insert_rule(text: &str, start: usize) -> MarkdownEditResult {
    let b_start = char_index(text, start);
    let before = &text[..b_start];
    let prefix = if before.is_empty() || before.ends_with("\n\n") {
        ""
    } else if before.ends_with('\n') {
        "\n"
    } else {
        "\n\n"
    };
    MarkdownEditResult {
        text: format!("{before}{prefix}---\n\n{}", &text[b_start..]),
        start: start + prefix.chars().count() + 5,
        length: 0,
    }
}

/// Toggles a line prefix over every line touching the selection: when every
/// touched line already carries it (after up to 3 leading spaces) the prefix
/// is removed, otherwise applied.
fn toggle_line_prefix(text: &str, start: usize, length: usize, prefix: &str) -> MarkdownEditResult {
    let chars: Vec<char> = text.chars().collect();
    let selection_end = (start + length).min(chars.len());
    let line_start = chars[..start]
        .iter()
        .rposition(|&c| c == '\n')
        .map(|i| i + 1)
        .unwrap_or(0);
    let line_end = chars[selection_end..]
        .iter()
        .position(|&c| c == '\n')
        .map(|i| selection_end + i)
        .unwrap_or(chars.len());

    let mut lines: Vec<(usize, String)> = Vec::new(); // (line start, content)
    let mut current = line_start;
    for end in chars[line_start..line_end]
        .iter()
        .enumerate()
        .filter(|&(_, c)| *c == '\n')
        .map(|(i, _)| line_start + i + 1)
        .chain(std::iter::once(line_end))
    {
        lines.push((
            current,
            chars[current..end.min(chars.len())].iter().collect(),
        ));
        current = end;
    }

    let strip_prefix = |line: &str| -> Option<String> {
        let trimmed = line.trim_start_matches(' ');
        if trimmed.len() == line.len() - trimmed.len() && trimmed.len() > 3 {
            return None;
        }
        trimmed
            .strip_prefix(prefix)
            .map(|rest| format!("{}{}", &line[..line.len() - trimmed.len()], rest))
    };
    let all_carry = lines.iter().all(|(_, line)| {
        line.trim().is_empty() || line.trim_start_matches(' ').starts_with(prefix.trim_end())
    });

    let mut out = String::new();
    out.extend(chars[..line_start].iter());
    let mut new_start = None;
    let mut consumed = 0usize;
    for (line_ix, (line_begin, line)) in lines.iter().enumerate() {
        let _ = line_begin;
        let replaced = if all_carry && !line.trim().is_empty() {
            strip_prefix(line).unwrap_or_else(|| line.clone())
        } else if line.trim().is_empty() {
            line.clone()
        } else {
            // remove any competing list/heading prefix first
            let stripped = strip_competing(line);
            format!("{prefix}{stripped}")
        };
        if line_ix == 0 {
            new_start =
                Some(line_start + consumed + prefix.chars().count().min(replaced.chars().count()));
        }
        consumed += replaced.chars().count();
        out.push_str(&replaced);
    }
    let new_len = consumed.saturating_sub(new_start.unwrap_or(line_start));
    out.extend(chars[line_end..].iter());
    MarkdownEditResult {
        text: out,
        start: new_start.unwrap_or(line_start),
        length: new_len,
    }
}

fn strip_competing(line: &str) -> String {
    let trimmed = line.trim_start_matches(' ');
    let indent = &line[..line.len() - trimmed.len()];
    let rest = trimmed
        .trim_start_matches('#')
        .trim_start_matches(' ')
        .trim_start_matches("- [ ] ")
        .trim_start_matches("- [x] ")
        .trim_start_matches("- ")
        .trim_start_matches("* ")
        .trim_start_matches("> ");
    // also strip an existing "N. " marker so renumbering never stacks
    let rest = match rest.split_once(". ") {
        Some((head, tail)) if !head.is_empty() && head.chars().all(|c| c.is_ascii_digit()) => tail,
        _ => rest,
    };
    format!("{indent}{rest}")
}

/// The numbered-list toggle: strips "N. " markers when every affected line
/// already carries one; otherwise renumbers the selection 1., 2., 3., …
fn toggle_numbered(text: &str, start: usize, length: usize) -> MarkdownEditResult {
    let chars: Vec<char> = text.chars().collect();
    let selection_end = (start + length).min(chars.len());
    let line_start = chars[..start]
        .iter()
        .rposition(|&c| c == '\n')
        .map(|i| i + 1)
        .unwrap_or(0);
    let line_end = chars[selection_end..]
        .iter()
        .position(|&c| c == '\n')
        .map(|i| selection_end + i)
        .unwrap_or(chars.len());

    let numbered_prefix = |line: &str| -> Option<(String, String)> {
        let trimmed = line.trim_start_matches(' ');
        let (head, tail) = trimmed.split_once(". ")?;
        head.chars()
            .all(|c| c.is_ascii_digit())
            .then(|| (head.to_string(), tail.to_string()))
    };

    let mut affected: Vec<String> = Vec::new();
    let mut current = line_start;
    while current <= line_end {
        let end = chars[current..]
            .iter()
            .position(|&c| c == '\n')
            .map(|i| current + i)
            .unwrap_or(line_end)
            .min(line_end);
        affected.push(chars[current..end].iter().collect());
        if end >= line_end {
            break;
        }
        current = end + 1;
    }

    let numbers: Vec<Option<u32>> = affected
        .iter()
        .map(|line| numbered_prefix(line).and_then(|(head, _)| head.parse::<u32>().ok()))
        .collect();
    let all_numbered = affected
        .iter()
        .zip(&numbers)
        .all(|(line, n)| line.trim().is_empty() || n.is_some());
    let numbered: Vec<u32> = numbers.iter().filter_map(|n| *n).collect();
    // already a clean 1..n list → the click toggles it off; stale or duplicate
    // markers → renumber instead
    let sequential = numbered == (1..=numbered.len() as u32).collect::<Vec<u32>>();

    let mut out = String::new();
    out.extend(chars[..line_start].iter());
    let mut new_start = None;
    let mut consumed = 0usize;
    let mut number = 0usize;
    for (ix, line) in affected.iter().enumerate() {
        let replaced = if line.trim().is_empty() {
            line.clone()
        } else if all_numbered && sequential {
            match numbered_prefix(line) {
                Some((_, tail)) => {
                    let indent_len = line.len() - line.trim_start_matches(' ').len();
                    format!("{}{}", &line[..indent_len], tail)
                }
                None => line.clone(),
            }
        } else {
            number += 1;
            format!("{number}. {}", strip_competing(line))
        };
        if ix == 0 {
            new_start = Some(line_start + consumed + replaced.chars().count());
        }
        consumed += replaced.chars().count();
        out.push_str(&replaced);
        if ix + 1 < affected.len() {
            out.push('\n');
            consumed += 1;
        }
    }
    let new_len =
        consumed.saturating_sub(new_start.unwrap_or(line_start).saturating_sub(line_start));
    out.extend(chars[line_end..].iter());
    MarkdownEditResult {
        text: out,
        start: new_start.unwrap_or(line_start),
        length: new_len,
    }
}

/// Enter inside a list item continues the list (empty item ends it).
pub fn continue_list(text: &str, caret: usize) -> Option<(String, usize)> {
    let chars: Vec<char> = text.chars().collect();
    let caret = caret.min(chars.len());
    let line_start = chars[..caret]
        .iter()
        .rposition(|&c| c == '\n')
        .map(|i| i + 1)
        .unwrap_or(0);
    let line: String = chars[line_start..caret].iter().collect();
    let trimmed = line.trim_start_matches(' ');
    let indent = &line[..line.len() - trimmed.len()];

    let markers: &[(&str, bool)] = &[
        ("- ", false),
        ("* ", false),
        ("+ ", false),
        ("> ", false),
        ("- [ ] ", true),
        ("- [x] ", true),
    ];
    for (marker, _is_task) in markers {
        if let Some(rest) = trimmed.strip_prefix(marker) {
            if rest.trim().is_empty() {
                // empty item: remove it (and the newline before it) and end
                // the list
                let cut = if line_start > 0 {
                    line_start - 1
                } else {
                    line_start
                };
                let without = format!(
                    "{}{}",
                    &text[..char_index(text, cut)],
                    &text[char_index(text, caret)..]
                );
                let new_caret = cut;
                return Some((without, new_caret));
            }
            let insertion = format!("\n{indent}{marker}");
            let new_caret = caret + insertion.chars().count();
            let at = char_index(text, caret);
            return Some((
                format!("{}{insertion}{}", &text[..at], &text[at..]),
                new_caret,
            ));
        }
    }
    // ordered list: "1. " with any number
    if let Some(numbered) = trimmed
        .split_once(". ")
        .or_else(|| trimmed.split_once(") "))
        .filter(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
    {
        let n: u64 = numbered.0.parse().ok()?;
        let rest = numbered.1;
        if rest.trim().is_empty() {
            let cut = if line_start > 0 {
                line_start - 1
            } else {
                line_start
            };
            let without = format!(
                "{}{}",
                &text[..char_index(text, cut)],
                &text[char_index(text, caret)..]
            );
            return Some((without, cut));
        }
        let insertion = format!("\n{indent}{}. ", n + 1);
        let new_caret = caret + insertion.chars().count();
        let at = char_index(text, caret);
        return Some((
            format!("{}{insertion}{}", &text[..at], &text[at..]),
            new_caret,
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_and_unwrap_bold() {
        let result = markdown_apply("bold", "hello world", 6, 5);
        assert_eq!(result.text, "hello **world**");
        assert_eq!((result.start, result.length), (8, 5));

        let unwrapped = markdown_apply("bold", &result.text, result.start, result.length);
        assert_eq!(unwrapped.text, "hello world");
        assert_eq!((unwrapped.start, unwrapped.length), (6, 5));
    }

    #[test]
    fn wrap_with_no_selection_inserts_placeholder() {
        let result = markdown_apply("code", "ab", 1, 0);
        assert_eq!(result.text, "a`text`b");
        assert_eq!((result.start, result.length), (2, 4));
    }

    #[test]
    fn heading_toggles_on_then_off() {
        let on = markdown_apply("h2", "line one\nline two", 0, 15);
        assert_eq!(on.text, "## line one\n## line two");
        let off = markdown_apply("h2", &on.text, on.start, on.length);
        assert_eq!(off.text, "line one\nline two");
    }

    #[test]
    fn bullet_replaces_heading_prefix() {
        let result = markdown_apply("bullet", "## title", 0, 8);
        assert_eq!(result.text, "- title");
    }

    #[test]
    fn numbered_renumbers_then_toggles_off() {
        let on = markdown_apply("numbered", "alpha\nbeta\ngamma", 0, 16);
        assert_eq!(on.text, "1. alpha\n2. beta\n3. gamma");

        // re-applying over the numbered block strips the markers
        let off = markdown_apply("numbered", &on.text, on.start, on.length);
        assert_eq!(off.text, "alpha\nbeta\ngamma");
    }

    #[test]
    fn numbered_skips_blank_lines_when_renumbering() {
        let on = markdown_apply("numbered", "alpha\n\nbeta", 0, 11);
        assert_eq!(on.text, "1. alpha\n\n2. beta");
    }

    #[test]
    fn numbered_replaces_stale_markers_not_stacks() {
        let on = markdown_apply("numbered", "1. alpha\n1. beta", 0, 15);
        assert_eq!(on.text, "1. alpha\n2. beta");
    }

    #[test]
    fn list_continuation_and_termination() {
        let (text, caret) = continue_list("- first item", 12).unwrap();
        assert_eq!(text, "- first item\n- ");
        assert_eq!(caret, 15);

        let (text, _) = continue_list("- first item\n- ", 15).unwrap();
        assert_eq!(text, "- first item");

        let (text, _) = continue_list("1. one", 6).unwrap();
        assert_eq!(text, "1. one\n2. ");
    }
}
