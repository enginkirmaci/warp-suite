//! Plain data types shared by the git read layer, the CLI mutation layer and
//! the app store. Everything here is immutable once returned; the store owns
//! the mutable copies the UI renders.

use chrono::{DateTime, Local};

/// One working-tree change. `status_code` is the porcelain status ("M", "A",
/// "?"…); for staged+unstaged combinations the two XY characters are kept
/// verbatim. Counts come from the numstat diffs and are `None` for untracked
/// and binary files.
#[derive(Clone, Debug, PartialEq)]
pub struct GitChangedFile {
    pub path: String,
    pub status_code: String,
    pub additions: Option<u32>,
    pub deletions: Option<u32>,
}

impl GitChangedFile {
    pub fn has_counts(&self) -> bool {
        self.additions.is_some() || self.deletions.is_some()
    }

    /// The status letter as the UI shows it — "U" for untracked (VS Code's
    /// letter). A merged two-sided code (".M", "M.") shows the side that
    /// changed, the index when both did.
    pub fn display_status_code(&self) -> &str {
        if self.status_code == "?" {
            return "U";
        }
        let code = self.status_code.trim_matches('.');
        code.get(..1).unwrap_or(code)
    }
}

/// One recent commit for the Changes tab's history list.
#[derive(Clone, Debug)]
pub struct GitCommitInfo {
    pub hash: String,
    pub subject: String,
    pub author: Option<String>,
    pub date: DateTime<Local>,
}

impl GitCommitInfo {
    pub fn short_hash(&self) -> &str {
        if self.hash.len() <= 7 {
            &self.hash
        } else {
            &self.hash[..7]
        }
    }

    pub fn initials(&self) -> String {
        let Some(author) = self.author.as_deref().filter(|a| !a.trim().is_empty()) else {
            return "?".into();
        };
        let parts: Vec<&str> = author.split_whitespace().collect();
        match parts.as_slice() {
            // a byte slice would panic on a multi-byte first char ("Žan")
            [only] => only
                .chars()
                .next()
                .unwrap_or('?')
                .to_uppercase()
                .to_string(),
            [first, .., last] => {
                let f = first
                    .chars()
                    .next()
                    .unwrap_or('?')
                    .to_uppercase()
                    .to_string();
                let l = last
                    .chars()
                    .next()
                    .unwrap_or('?')
                    .to_uppercase()
                    .to_string();
                format!("{f}{l}")
            }
            _ => "?".into(),
        }
    }
}

/// One commit's changed files plus the message body after the subject block.
#[derive(Clone, Debug)]
pub struct GitCommitDetails {
    pub hash: String,
    pub files: Vec<GitChangedFile>,
    pub body: Option<String>,
}

/// One repo's parsed change probes: the status entries shaped for the flat
/// list and the staged/unstaged split, plus the per-side numstat maps the
/// views merge with different rules.
#[derive(Clone, Debug, Default)]
pub struct GitChangeSnapshot {
    pub status_files: Vec<GitChangedFile>,
    pub staged_files: Vec<GitChangedFile>,
    pub unstaged_files: Vec<GitChangedFile>,
    pub worktree_counts: std::collections::HashMap<String, (Option<u32>, Option<u32>)>,
    pub cached_counts: std::collections::HashMap<String, (Option<u32>, Option<u32>)>,
}

impl GitChangeSnapshot {
    /// The flat file list with merged counts: the worktree diff first, the
    /// staged (cached) diff layered on top, so a path present in both reports
    /// its staged counts. Untracked and binary files stay count-less.
    pub fn merged_files(&self) -> Vec<GitChangedFile> {
        let mut counts = self.worktree_counts.clone();
        for (path, count) in &self.cached_counts {
            counts.insert(path.clone(), *count);
        }
        self.status_files
            .iter()
            .map(|file| match counts.get(&file.path) {
                Some((additions, deletions)) => GitChangedFile {
                    additions: *additions,
                    deletions: *deletions,
                    ..file.clone()
                },
                None => file.clone(),
            })
            .collect()
    }

    /// The staged/unstaged split with per-side counts applied.
    pub fn change_groups(&self) -> (Vec<GitChangedFile>, Vec<GitChangedFile>) {
        let apply =
            |files: &[GitChangedFile],
             counts: &std::collections::HashMap<String, (Option<u32>, Option<u32>)>| {
                files
                    .iter()
                    .map(|file| match counts.get(&file.path) {
                        Some((additions, deletions)) => GitChangedFile {
                            additions: *additions,
                            deletions: *deletions,
                            ..file.clone()
                        },
                        None => file.clone(),
                    })
                    .collect()
            };
        (
            apply(&self.staged_files, &self.cached_counts),
            apply(&self.unstaged_files, &self.worktree_counts),
        )
    }
}

/// One commit made on the queried day (empty files for a merge commit).
#[derive(Clone, Debug)]
pub struct GitDayCommit {
    pub hash: String,
    pub subject: String,
    pub author: Option<String>,
    pub when: DateTime<Local>,
    pub files: Vec<GitChangedFile>,
}

/// One repo's activity for a single local day.
#[derive(Clone, Debug)]
pub struct GitDayActivity {
    pub branch_name: Option<String>,
    pub commits: Vec<GitDayCommit>,
    pub uncommitted_files: Vec<GitChangedFile>,
}

impl GitDayActivity {
    pub fn empty() -> Self {
        Self {
            branch_name: None,
            commits: Vec::new(),
            uncommitted_files: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.commits.is_empty() && self.uncommitted_files.is_empty()
    }
}

/// What a branch dropdown row represents.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GitBranchKind {
    /// A local branch; selecting it checks it out.
    Local,
    /// A remote-tracking branch; selecting it checks out the local tracking equivalent.
    Remote,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitBranchRef {
    pub name: String,
    pub kind: GitBranchKind,
}

/// Outcome of one sync command: success, the actionable stderr line on
/// failure, or a user abort (neither success nor a git error).
#[derive(Clone, Debug)]
pub struct GitSyncResult {
    pub success: bool,
    pub error: Option<String>,
    pub cancelled: bool,
}

impl GitSyncResult {
    pub fn ok() -> Self {
        Self {
            success: true,
            error: None,
            cancelled: false,
        }
    }

    pub fn err(error: Option<String>) -> Self {
        Self {
            success: false,
            error,
            cancelled: false,
        }
    }

    pub fn cancelled() -> Self {
        Self {
            success: false,
            error: None,
            cancelled: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commit(author: Option<&str>) -> GitCommitInfo {
        GitCommitInfo {
            hash: "0123456789abcdef".into(),
            subject: "s".into(),
            author: author.map(str::to_string),
            date: Local::now(),
        }
    }

    #[test]
    fn the_status_letter_reads_one_side_of_a_merged_code() {
        let shown = |code: &str| {
            GitChangedFile {
                path: "a".into(),
                status_code: code.into(),
                additions: None,
                deletions: None,
            }
            .display_status_code()
            .to_string()
        };

        assert_eq!(shown(".M"), "M");
        assert_eq!(shown("A."), "A");
        assert_eq!(shown("MD"), "M", "the index side wins when both changed");
        assert_eq!(shown("D"), "D", "a one-sided code passes through");
        assert_eq!(shown("?"), "U");
    }

    #[test]
    fn initials_handles_multi_byte_and_missing_authors() {
        // a byte-slice regression: single-token "Ünal" panicked on a
        // non-char boundary
        assert_eq!(commit(Some("Ünal")).initials(), "Ü");
        assert_eq!(commit(Some("Žan Koder")).initials(), "ŽK");
        assert_eq!(commit(Some("Emil Detjen")).initials(), "ED");
        assert_eq!(commit(Some("madonna")).initials(), "M");
        assert_eq!(commit(Some("   ")).initials(), "?");
        assert_eq!(commit(None).initials(), "?");
    }
}
