//! Cross-repo code search: one ripgrep process over every tracked repo's
//! folder feeding the titlebar search popup. rg's `--json` event stream is
//! parsed defensively line by line — an unparseable line is skipped, never a
//! panic, and any process-level failure (binary missing, timeout, error exit)
//! settles as an empty hit list so the popup keeps its repo/note results.

use crate::process::{self, ProcessRunOptions};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Upper bound for one ripgrep sweep; a huge tree must not hold the popup
/// hostage.
const RG_TIMEOUT: Duration = Duration::from_secs(8);

/// The hit cap the popup dispatches with (repo/note rows share the popup's
/// 12-row budget; this bounds the code section itself).
pub const CODE_SEARCH_CAP: usize = 12;

/// Excerpt budget in bytes; longer lines are windowed around the match so
/// the highlighted slice stays inside the row.
const MAX_EXCERPT: usize = 96;

/// One file+line match from the ripgrep sweep. `match_start`/`match_end` are
/// byte offsets INTO `excerpt` (they slice the term back out for highlight
/// rendering, case-insensitively since the search runs `-i`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodeHit {
    pub repo: String,
    pub rel_path: String,
    pub line: u32,
    pub excerpt: String,
    pub match_start: usize,
    pub match_end: usize,
    pub abs_path: PathBuf,
}

/// One ripgrep sweep across ALL folders in a single process: fixed-string
/// (`-F`, the term is user text, never a regex), case-insensitive, two
/// matching lines per file, big binaries skipped. `-e` keeps a leading-dash
/// term from reading as a flag.
pub async fn run_code_search(
    rg_exe: &Path,
    folders: &[PathBuf],
    term: &str,
    cap: usize,
) -> Vec<CodeHit> {
    let term = term.trim();
    if folders.is_empty() || term.is_empty() {
        return Vec::new();
    }
    let mut args: Vec<String> = [
        "--json",
        "-F",
        "-i",
        "--no-messages",
        "--max-count",
        "2",
        "--max-filesize",
        "2M",
        "-e",
        term,
        "--",
    ]
    .iter()
    .map(|a| a.to_string())
    .collect();
    args.extend(folders.iter().map(|f| f.to_string_lossy().to_string()));

    let result = process::run(ProcessRunOptions {
        file_name: &rg_exe.to_string_lossy(),
        args: &args,
        timeout: Some(RG_TIMEOUT),
        close_stdin: true,
        // the head is enough: the 12-hit cap sits far below any realistic
        // full output, and a chatty sweep dies instead of materializing MBs
        max_output_bytes: Some(1 << 20),
        environment: vec![("GIT_TERMINAL_PROMPT".into(), "0".into())],
        ..Default::default()
    })
    .await;
    let output = match result {
        Ok(out) => out.stdout,
        Err(err) => {
            tracing::warn!("code search spawn failed: {err:#}");
            return Vec::new();
        }
    };

    let mut hits = Vec::new();
    for line in output.lines() {
        if hits.len() >= cap {
            break;
        }
        if let Some(hit) = parse_match_event(line, folders) {
            hits.push(hit);
        }
    }
    hits
}

/// Parses one rg `--json` line; only `type == "match"` events produce hits.
/// Any shape drift, bad offset, or path outside the searched folders skips
/// the line.
fn parse_match_event(line: &str, folders: &[PathBuf]) -> Option<CodeHit> {
    let event: Value = serde_json::from_str(line).ok()?;
    if event.get("type").and_then(Value::as_str) != Some("match") {
        return None;
    }
    let data = event.get("data")?;
    let abs_path = PathBuf::from(data.pointer("/path/text")?.as_str()?);
    let line_number = u32::try_from(data.get("line_number")?.as_u64()?).ok()?;
    let line_text = data.pointer("/lines/text")?.as_str()?;
    let submatch = data.pointer("/submatches/0")?;
    let match_start = usize::try_from(submatch.get("start")?.as_u64()?).ok()?;
    let match_end = usize::try_from(submatch.get("end")?.as_u64()?).ok()?;

    // repo attribution: the hit belongs to the first searched folder that
    // contains it; a path under none of them is an rg quirk — skip it
    let mut repo = None;
    let mut rel_path = String::new();
    for folder in folders {
        if abs_path.starts_with(folder) {
            repo = Some(
                folder
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| folder.to_string_lossy().to_string()),
            );
            rel_path = abs_path
                .strip_prefix(folder)
                .unwrap_or(&abs_path)
                .to_string_lossy()
                .to_string();
            break;
        }
    }
    let repo = repo?;

    let (excerpt, match_start, match_end) = excerpt_around(line_text, match_start, match_end)?;
    Some(CodeHit {
        repo,
        rel_path,
        line: line_number,
        excerpt,
        match_start,
        match_end,
        abs_path,
    })
}

/// Trims rg's trailing newline off the line text and, for long lines, windows
/// MAX_EXCERPT bytes around the match — the returned offsets index the
/// returned excerpt and always slice the matched text back out.
fn excerpt_around(
    line_text: &str,
    match_start: usize,
    match_end: usize,
) -> Option<(String, usize, usize)> {
    let trimmed = line_text.strip_suffix('\n').unwrap_or(line_text);
    let trimmed = trimmed.strip_suffix('\r').unwrap_or(trimmed);
    if match_start >= match_end
        || match_end > trimmed.len()
        || !trimmed.is_char_boundary(match_start)
        || !trimmed.is_char_boundary(match_end)
    {
        return None;
    }
    if trimmed.len() <= MAX_EXCERPT {
        return Some((trimmed.to_string(), match_start, match_end));
    }
    let match_len = match_end - match_start;
    if match_len >= MAX_EXCERPT {
        // the match alone fills the window: the excerpt IS the match
        return Some((trimmed[match_start..match_end].to_string(), 0, match_len));
    }
    let pad = (MAX_EXCERPT - match_len) / 2;
    let mut start = match_start.saturating_sub(pad);
    let mut end = trimmed.len().min(start + MAX_EXCERPT);
    // slide onto char boundaries without ever crossing the match
    while start > 0 && !trimmed.is_char_boundary(start) {
        start += 1;
    }
    while end < trimmed.len() && !trimmed.is_char_boundary(end) {
        end += 1;
    }
    Some((
        trimmed[start..end].to_string(),
        match_start - start,
        match_end - start,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Two fake repos with nested files and realistic text. Returns the root
    /// the repos live under.
    fn fixture(name: &str) -> PathBuf {
        // the name keeps parallel tests out of each other's remove_dir_all
        let root = std::env::temp_dir().join(format!(
            "warp-station-code-search-{}-{name}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("alpha/src")).unwrap();
        fs::create_dir_all(root.join("beta/docs")).unwrap();
        fs::write(
            root.join("alpha/src/lib.rs"),
            "pub fn needle_finder() {}\n// a NEEDLE comment line\nfn plain() {}\n// needle again\n",
        )
        .unwrap();
        fs::write(root.join("alpha/README.md"), "the needle in the readme\n").unwrap();
        fs::write(
            root.join("beta/docs/guide.md"),
            "# Guide\nfollow the NEEDLE through the docs\n",
        )
        .unwrap();
        fs::write(root.join("beta/main.py"), "def needle(): return 'needle'\n").unwrap();
        root
    }

    fn folders(root: &Path) -> Vec<PathBuf> {
        vec![root.join("alpha"), root.join("beta")]
    }

    /// `None` skips the test (rg missing); `Some(empty)` is a real failure —
    /// the fixture is guaranteed to match, so empty means the sweep or its
    /// parsing broke.
    async fn search(root: &Path, term: &str, cap: usize) -> Option<Vec<CodeHit>> {
        let rg = crate::exe::locate("rg")?;
        Some(run_code_search(&rg, &folders(root), term, cap).await)
    }

    #[tokio::test]
    async fn finds_matches_across_repos_with_case_insensitive_fixed_strings() {
        let root = fixture("case-insensitive");
        let Some(hits) = search(&root, "NEEDLE", 12).await else {
            println!("skipping code-search test: rg not found on PATH");
            return;
        };
        // alpha/src/lib.rs caps at 2 lines (needle_finder + NEEDLE comment);
        // README, guide.md and main.py add one line each
        assert_eq!(hits.len(), 5, "one rg pass covers every repo: {hits:?}");
        let lib: Vec<_> = hits
            .iter()
            .filter(|h| h.repo == "alpha" && h.rel_path.ends_with("lib.rs"))
            .collect();
        assert_eq!(lib.len(), 2, "per-file max-count=2: {lib:?}");
        assert_eq!(lib[0].line, 1);
        assert_eq!(lib[1].line, 2);
        let guide = hits
            .iter()
            .find(|h| h.rel_path == "docs/guide.md")
            .expect("nested file found");
        assert_eq!(guide.repo, "beta");
        assert_eq!(guide.line, 2);
        // case-insensitive: the uppercase term matched the lowercase occurrences
        for hit in &hits {
            let matched = &hit.excerpt[hit.match_start..hit.match_end];
            assert_eq!(matched.len(), "NEEDLE".len());
            assert!(
                matched.eq_ignore_ascii_case("NEEDLE"),
                "matched {matched:?}"
            );
        }
        let _ = fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn cap_bounds_the_hit_list() {
        let root = fixture("cap");
        let Some(hits) = search(&root, "needle", 3).await else {
            println!("skipping code-search test: rg not found on PATH");
            return;
        };
        assert_eq!(hits.len(), 3, "the cap stops collection early");
        let _ = fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn long_lines_window_the_excerpt_around_the_match() {
        let root = fixture("long-lines");
        let filler = "x".repeat(400);
        fs::create_dir_all(root.join("alpha")).unwrap();
        fs::write(
            root.join("alpha/wide.txt"),
            format!("{filler} needle {filler}\n"),
        )
        .unwrap();
        let Some(hits) = search(&root, "needle", 12).await else {
            println!("skipping code-search test: rg not found on PATH");
            return;
        };
        let wide = hits
            .iter()
            .find(|h| h.rel_path == "wide.txt")
            .expect("the wide line");
        assert!(
            wide.excerpt.len() < 200,
            "excerpt windowed, got {} bytes",
            wide.excerpt.len()
        );
        let matched = &wide.excerpt[wide.match_start..wide.match_end];
        assert!(matched.eq_ignore_ascii_case("needle"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn excerpt_windows_are_byte_exact() {
        let line = format!("{} target {}", "a".repeat(80), "b".repeat(80));
        // match at byte 81..87 (80 a's + the space), line longer than the window
        let (excerpt, start, end) = excerpt_around(&line, 81, 87).expect("clean line");
        assert_eq!(&excerpt[start..end], "target");
        assert!(
            excerpt.len() <= MAX_EXCERPT + 4,
            "window stays tight: {excerpt}"
        );
        // a short line keeps its full text and original offsets
        let (excerpt, start, end) = excerpt_around("hit the target here\n", 8, 14).unwrap();
        assert_eq!(excerpt, "hit the target here");
        assert_eq!(&excerpt[start..end], "target");
        // anomalies never panic
        assert!(excerpt_around("short", 2, 10).is_none());
        assert!(excerpt_around("short", 3, 1).is_none());
        // multi-byte text stays on char boundaries
        let utf8 = "héllo wörld — ünicode needle text with a bit of padding";
        let at = utf8.find("needle").unwrap();
        let (excerpt, start, end) = excerpt_around(utf8, at, at + 6).unwrap();
        assert_eq!(&excerpt[start..end], "needle");
    }

    #[tokio::test]
    async fn empty_inputs_and_missing_folders_settle_empty() {
        let Some(rg) = crate::exe::locate("rg") else {
            println!("skipping code-search test: rg not found on PATH");
            return;
        };
        assert!(run_code_search(&rg, &[], "term", 12).await.is_empty());
        assert!(
            run_code_search(&rg, &folders(&fixture("empty-inputs")), "   ", 12)
                .await
                .is_empty()
        );
        // a folder that does not exist: rg errors, the sweep still settles empty
        assert!(
            run_code_search(&rg, &[PathBuf::from("/definitely/not/here")], "term", 12)
                .await
                .is_empty()
        );
    }
}
