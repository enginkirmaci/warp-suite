//! The notes store's read side for the global search: per-repo note folders
//! under the store root (default ~/.warp/notes, shared with the warp-notes
//! app), capped content search. The full store — tree loading, CRUD, the
//! markdown editing ops — lives in the warp-notes repo.

use crate::paths::AppPaths;
use std::path::{Path, PathBuf};

const SEARCH_CONTENT_CAP_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct NotesSearchHit {
    pub path: PathBuf,
    pub title: String,
    pub relative: String,
    pub excerpt: String,
}

pub fn resolve_store_root(configured: &str) -> PathBuf {
    let trimmed = configured.trim();
    if trimmed.is_empty() {
        AppPaths::notes_dir()
    } else {
        PathBuf::from(trimmed)
    }
}

pub fn repo_notes_root(store_root: &Path, repo_name: &str) -> PathBuf {
    store_root.join(sanitize_entry_name(repo_name, false).unwrap_or_else(|| "notes".into()))
}

/// The global search scans a small per-repo file budget (40 files per repo);
/// `file_cap` is that budget.
pub fn search_capped(repo_notes_root: &Path, term: &str, file_cap: usize) -> Vec<NotesSearchHit> {
    if !repo_notes_root.is_dir() || term.trim().is_empty() {
        return Vec::new();
    }
    let term = term.trim();
    let mut hits = Vec::new();
    for path in enumerate_note_files(repo_notes_root) {
        if hits.len() >= file_cap {
            break;
        }
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let name_match = file_name.to_lowercase().contains(&term.to_lowercase());
        let excerpt = if name_match {
            String::new()
        } else {
            match read_excerpt(&path, term) {
                Some(e) => e,
                None => continue,
            }
        };
        let relative = path
            .strip_prefix(repo_notes_root)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        hits.push(NotesSearchHit {
            title: file_name
                .trim_end_matches(".md")
                .trim_end_matches(".MD")
                .to_string(),
            excerpt: if excerpt.is_empty() {
                first_content_line(&path).unwrap_or_default()
            } else {
                excerpt
            },
            path,
            relative,
        });
    }
    hits
}

/// Path-name sanitizer: invalid filename chars and separators become '-',
/// edges trimmed; `None` when nothing survives.
fn sanitize_entry_name(name: &str, enforce_markdown_extension: bool) -> Option<String> {
    let mut cleaned: String = name
        .trim()
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0'..='\u{1f}' => '-',
            other => other,
        })
        .collect();
    cleaned = cleaned.trim_matches([' ', '.']).to_string();
    if cleaned.is_empty() {
        return None;
    }
    if enforce_markdown_extension && !cleaned.to_lowercase().ends_with(".md") {
        cleaned.push_str(".md");
    }
    Some(cleaned)
}

fn enumerate_note_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut queue = std::collections::VecDeque::new();
    queue.push_back(root.to_path_buf());
    while let Some(current) = queue.pop_front() {
        let Ok(entries) = std::fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            match entry.file_type() {
                Ok(t) if t.is_dir() => {
                    if !name.starts_with('.') {
                        queue.push_back(entry.path());
                    }
                }
                Ok(_) if name.to_lowercase().ends_with(".md") => files.push(entry.path()),
                _ => {}
            }
        }
    }
    files
}

fn read_excerpt(path: &Path, term: &str) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    if meta.len() > SEARCH_CONTENT_CAP_BYTES {
        return None;
    }
    let content = std::fs::read_to_string(path).ok()?;
    for raw_line in content.lines() {
        let line: String = raw_line.split_whitespace().collect::<Vec<_>>().join(" ");
        let Some(index) = line.to_lowercase().find(&term.to_lowercase()) else {
            continue;
        };
        // the offset was found in the lowercased copy, whose byte length can
        // differ (İ U+0130 lowercases to 3 bytes) — walk back onto the
        // original line's char boundaries before slicing (see
        // code_search::excerpt_around for the same pattern)
        let mut start = index.saturating_sub(40);
        while start > 0 && !line.is_char_boundary(start) {
            start += 1;
        }
        let mut end = (start + 160).min(line.len());
        while end < line.len() && !line.is_char_boundary(end) {
            end += 1;
        }
        let mut excerpt = String::new();
        if start > 0 {
            excerpt.push('…');
        }
        excerpt.push_str(&line[start..end]);
        if end < line.len() {
            excerpt.push('…');
        }
        return Some(excerpt);
    }
    None
}

fn first_content_line(path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    content
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_replaces_invalid_and_enforces_md() {
        assert_eq!(
            sanitize_entry_name("a/b\\c:d", false).as_deref(),
            Some("a-b-c-d")
        );
        assert_eq!(
            sanitize_entry_name("  ..note.. ", false).as_deref(),
            Some("note")
        );
        assert_eq!(sanitize_entry_name("   ", false), None);
        assert_eq!(
            sanitize_entry_name("todo", true).as_deref(),
            Some("todo.md")
        );
        assert_eq!(
            sanitize_entry_name("TODO.MD", true).as_deref(),
            Some("TODO.MD")
        );
    }

    #[test]
    fn search_scans_the_repo_folder() {
        let tmp = std::env::temp_dir().join(format!("dt-notes-search-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let root = tmp.join("My Repo");
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::write(root.join("alpha.md"), "# Title\n\nhello needle here\n").unwrap();
        std::fs::write(root.join("sub/beta.md"), "nothing relevant\n").unwrap();
        std::fs::write(root.join(".hidden.md"), "needle").unwrap();

        // hidden FILES match by name (only dot-DIRECTORIES are skipped)
        let hits = search_capped(&root, "needle", 40);
        assert_eq!(hits.len(), 2);
        assert!(hits.iter().any(|h| h.excerpt.contains("needle")));
        assert!(hits.iter().all(|h| h.path.starts_with(&root)));

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
