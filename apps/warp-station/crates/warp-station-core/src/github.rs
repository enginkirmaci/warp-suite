//! The GitHub activity service: per-repo `gh` queries run inside the repo
//! folder (gh resolves owner/name from the git remote). One `gh repo view`
//! proves the repo lives on GitHub and yields its link, then the open PR and
//! issue lists feed the column chips and the details tabs. The closed lists and
//! one pull request's full view load on demand. Payloads are parsed defensively
//! from serde_json Values — gh's shapes drift.
//!
//! The pull request's own surface reads one `gh pr view`, then two lazy calls:
//! `gh pr diff` for the changed files' patches and `gh api .../commits/<sha>`
//! for one commit's files. Every verb is a `gh pr` subcommand, so the app never
//! speaks GraphQL itself.

use crate::process::{self, ProcessRunOptions};
use std::path::Path;
use std::time::Duration;

/// Upper bound for one gh invocation; a hung network call must not stall the pass.
const GH_TIMEOUT: Duration = Duration::from_secs(20);

/// Caps each list fetch (and therefore the chip counts).
const ITEM_LIMIT: usize = 50;

const PR_FIELDS: &str = "number,title,url,author,labels,isDraft,headRefName,baseRefName,\
                         reviewDecision,updatedAt,createdAt";
const ISSUE_FIELDS: &str = "number,title,url,author,labels,updatedAt,createdAt,comments";
const PULL_VIEW_FIELDS: &str = "number,title,url,author,state,isDraft,headRefName,baseRefName,\
                                additions,deletions,mergeable,mergeStateStatus,reviewDecision,\
                                updatedAt,createdAt,mergedAt,body,comments,reviews,\
                                statusCheckRollup,files,commits,reviewRequests,autoMergeRequest";

#[derive(Clone, Debug, Default)]
pub struct GitHubItem {
    pub number: u64,
    pub title: String,
    pub url: String,
    pub author: Option<String>,
    pub labels: Vec<String>,
    pub is_draft: bool,
    pub review_decision: Option<String>,
    pub updated_at: Option<String>,
    pub created_at: Option<String>,
    /// Issues only: gh answers pull request lists without it.
    pub comments: u32,
    pub head_ref_name: Option<String>,
    pub base_ref_name: Option<String>,
    pub is_pull_request: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckState {
    Passed,
    Failed,
    Running,
    Skipped,
}

#[derive(Clone, Debug)]
pub struct GitHubCheck {
    pub name: String,
    pub state: CheckState,
    pub url: Option<String>,
}

#[derive(Clone, Debug)]
pub struct GitHubComment {
    pub author: String,
    pub created_at: Option<String>,
    pub body: String,
}

/// One person's latest review. A later review replaces an earlier one.
#[derive(Clone, Debug)]
pub struct GitHubReview {
    pub author: String,
    pub state: String,
}

/// One changed file of a pull request, with its patch when the diff has landed.
/// `status` is the rail's letter: M, A, D or R.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GitHubFilePatch {
    pub path: String,
    pub status: String,
    pub additions: u64,
    pub deletions: u64,
    pub patch: String,
}

/// One commit of a pull request: what the Commits tab's list draws.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GitHubCommit {
    pub oid: String,
    pub headline: String,
    pub author: Option<String>,
    pub committed_at: Option<String>,
}

/// A verb the pull request's head offers. Each maps to one `gh pr` call, so the
/// app's copy of the rules is the argv itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PullAction {
    Approve,
    /// GitHub wants words with a request for changes; the app sends a short
    /// default until a review composer exists.
    RequestChanges(String),
    Comment(String),
    Ready,
    Draft,
    Close,
    Reopen,
}

impl PullAction {
    /// The argv `gh pr` needs, the pull request's number included.
    pub fn argv(&self, number: u64) -> Vec<String> {
        let number = number.to_string();
        let word = |w: &str| w.to_string();
        match self {
            Self::Approve => vec![word("pr"), word("review"), number, word("--approve")],
            Self::RequestChanges(body) => vec![
                word("pr"),
                word("review"),
                number,
                word("--request-changes"),
                word("--body"),
                body.clone(),
            ],
            Self::Comment(body) => vec![
                word("pr"),
                word("comment"),
                number,
                word("--body"),
                body.clone(),
            ],
            Self::Ready => vec![word("pr"), word("ready"), number],
            Self::Draft => vec![word("pr"), word("ready"), number, word("--undo")],
            Self::Close => vec![word("pr"), word("close"), number],
            Self::Reopen => vec![word("pr"), word("reopen"), number],
        }
    }

    /// What the toast says when the call lands.
    pub fn done_word(&self, number: u64) -> String {
        match self {
            Self::Approve => format!("Approved #{number}"),
            Self::RequestChanges(_) => format!("Changes requested on #{number}"),
            Self::Comment(_) => format!("Commented on #{number}"),
            Self::Ready => format!("Marked #{number} ready for review"),
            Self::Draft => format!("Marked #{number} as draft"),
            Self::Close => format!("Closed #{number}"),
            Self::Reopen => format!("Reopened #{number}"),
        }
    }
}

/// What the New pull request form sends. `head` is the source branch, `base`
/// the one it lands on.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NewPullRequest {
    pub title: String,
    pub body: String,
    pub base: String,
    pub head: String,
    /// An optional person to request a review from.
    pub reviewer: Option<String>,
    pub draft: bool,
}

impl NewPullRequest {
    /// The argv `gh pr create` needs. An empty body still passes `--body`, or
    /// gh would stop and ask for one at a prompt nobody can answer.
    pub fn argv(&self) -> Vec<String> {
        let word = |w: &str| w.to_string();
        let mut argv = vec![
            word("pr"),
            word("create"),
            word("--title"),
            self.title.trim().to_string(),
            word("--body"),
            self.body.clone(),
            word("--base"),
            self.base.trim().to_string(),
            word("--head"),
            self.head.trim().to_string(),
        ];
        if self.draft {
            argv.push(word("--draft"));
        }
        if let Some(reviewer) = self.reviewer.as_deref().map(str::trim)
            && !reviewer.is_empty()
        {
            argv.push(word("--reviewer"));
            argv.push(reviewer.to_string());
        }
        argv
    }

    /// What the form refuses before it sends: a title, and two different
    /// branches. gh would fail on both, but the form can say why first.
    pub fn problem(&self) -> Option<&'static str> {
        if self.title.trim().is_empty() {
            Some("A title is required.")
        } else if self.head.trim().is_empty() || self.base.trim().is_empty() {
            Some("Source and base must both name a branch.")
        } else if self.head.trim() == self.base.trim() {
            Some("Source and base must be different branches.")
        } else {
            None
        }
    }
}

/// One pull request's full view: `gh pr view` for the detail pane.
#[derive(Clone, Debug, Default)]
pub struct GitHubPull {
    pub number: u64,
    pub title: String,
    pub url: String,
    pub author: Option<String>,
    /// OPEN, CLOSED or MERGED.
    pub state: String,
    pub is_draft: bool,
    pub head_ref_name: String,
    pub base_ref_name: String,
    pub additions: u64,
    pub deletions: u64,
    /// MERGEABLE, CONFLICTING or UNKNOWN.
    pub mergeable: String,
    /// CLEAN, BLOCKED, DIRTY, BEHIND, UNSTABLE, ...
    pub merge_state: String,
    pub review_decision: Option<String>,
    pub updated_at: Option<String>,
    pub body: String,
    pub comments: Vec<GitHubComment>,
    pub reviews: Vec<GitHubReview>,
    pub checks: Vec<GitHubCheck>,
    /// The changed files, as the view asks for them (no patch yet).
    pub files: Vec<GitHubFilePatch>,
    pub commits: Vec<GitHubCommit>,
    /// People a review is still owed from.
    pub requested_reviewers: Vec<String>,
    /// Already armed outside the app; the strip shows the badge.
    pub auto_merge: bool,
    pub created_at: Option<String>,
    pub merged_at: Option<String>,
}

/// How the Issues tab orders its rows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum IssueSort {
    #[default]
    Updated,
    Newest,
    Oldest,
    Comments,
}

impl IssueSort {
    pub const ALL: [Self; 4] = [Self::Updated, Self::Newest, Self::Oldest, Self::Comments];

    pub fn label(self) -> &'static str {
        match self {
            Self::Updated => "Updated",
            Self::Newest => "Newest",
            Self::Oldest => "Oldest",
            Self::Comments => "Most comments",
        }
    }

    /// The word the settings file keeps.
    pub fn key(self) -> &'static str {
        match self {
            Self::Updated => "updated",
            Self::Newest => "newest",
            Self::Oldest => "oldest",
            Self::Comments => "comments",
        }
    }

    pub fn from_key(key: Option<&str>) -> Self {
        Self::ALL
            .into_iter()
            .find(|sort| Some(sort.key()) == key)
            .unwrap_or_default()
    }

    /// gh's timestamps are all UTC in one format, so they sort as text.
    pub fn sort(self, items: &mut [GitHubItem]) {
        match self {
            Self::Updated => items.sort_by(|a, b| b.updated_at.cmp(&a.updated_at)),
            Self::Newest => items.sort_by(|a, b| b.created_at.cmp(&a.created_at)),
            Self::Oldest => items.sort_by(|a, b| a.created_at.cmp(&b.created_at)),
            Self::Comments => items.sort_by_key(|item| std::cmp::Reverse(item.comments)),
        }
    }
}

impl GitHubItem {
    /// The Issues filter: `query` is lowercase and trimmed, and looks through
    /// the number, title and labels.
    pub fn matches(&self, query: &str) -> bool {
        format!("#{} {} {}", self.number, self.title, self.labels.join(" "))
            .to_lowercase()
            .contains(query)
    }
}

impl GitHubPull {
    pub fn count_checks(&self, state: CheckState) -> usize {
        self.checks.iter().filter(|c| c.state == state).count()
    }

    pub fn approvals(&self) -> usize {
        self.reviews
            .iter()
            .filter(|r| r.state == "APPROVED")
            .count()
    }

    /// The people who asked for changes and have not taken it back.
    pub fn changes_requested(&self) -> usize {
        self.reviews
            .iter()
            .filter(|r| r.state == "CHANGES_REQUESTED")
            .count()
    }

    pub fn has_conflicts(&self) -> bool {
        self.mergeable == "CONFLICTING"
    }

    pub fn is_open(&self) -> bool {
        self.state == "OPEN"
    }

    pub fn is_merged(&self) -> bool {
        self.state == "MERGED"
    }

    /// Closed without being merged: the one state that can be reopened.
    pub fn is_closed(&self) -> bool {
        self.state == "CLOSED"
    }

    /// The word the state pill carries.
    pub fn state_word(&self) -> &'static str {
        if self.is_merged() {
            "Merged"
        } else if self.is_closed() {
            "Closed"
        } else if self.is_draft {
            "Draft"
        } else {
            "Open"
        }
    }

    /// Open, not a draft, no conflict and no failing or running check. What
    /// branch protection still refuses, the merge itself reports.
    pub fn ready_to_merge(&self) -> bool {
        self.is_open()
            && !self.is_draft
            && !self.has_conflicts()
            // the repository wants a review before anything merges
            && self.review_decision.as_deref() != Some("REVIEW_REQUIRED")
            && self.count_checks(CheckState::Failed) == 0
            && self.count_checks(CheckState::Running) == 0
    }

    /// The lines the subtitle prints: `+adds −dels` across the patch.
    pub fn line_counts(&self) -> (u64, u64) {
        (self.additions, self.deletions)
    }
}

#[derive(Clone, Debug, Default)]
pub struct GitHubSnapshot {
    pub url: Option<String>,
    pub pulls: Vec<GitHubItem>,
    pub issues: Vec<GitHubItem>,
    /// False when `gh repo view` failed (not a GitHub repo, or gh/auth
    /// trouble) — the cell stays empty instead of showing a misleading OK.
    pub available: bool,
}

#[derive(Clone, Debug, Default)]
pub struct GitHubRepoDetails {
    pub owner: Option<String>,
    pub created_at: Option<String>,
    pub primary_language: Option<String>,
    pub license: Option<String>,
    pub topics: Vec<String>,
    pub default_branch: Option<String>,
    pub url: Option<String>,
}

async fn run_gh(gh_exe: &str, folder: &Path, args: &[&str]) -> Option<String> {
    run_gh_checked(gh_exe, folder, args).await.ok()
}

/// As [`run_gh`], with gh's own words on a failure.
async fn run_gh_checked(gh_exe: &str, folder: &Path, args: &[&str]) -> Result<String, String> {
    let argv: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    let result = process::run(ProcessRunOptions {
        file_name: gh_exe,
        args: &argv,
        working_dir: Some(folder),
        timeout: Some(GH_TIMEOUT),
        environment: vec![
            ("GIT_TERMINAL_PROMPT".into(), "0".into()),
            ("GH_PROMPT".into(), "disabled".into()),
        ],
        ..Default::default()
    })
    .await
    .map_err(|err| err.to_string())?;
    if result.succeeded() {
        Ok(result.stdout)
    } else {
        Err(result.stderr.trim().to_string())
    }
}

/// One repo's full GitHub probe. A missing gh binary returns an unavailable
/// snapshot (the pass gates on resolution before spawning anyway).
pub async fn refresh(folder: &Path, gh_exe: &str) -> GitHubSnapshot {
    let Some(repo_json) = run_gh(gh_exe, folder, &["repo", "view", "--json", "url"]).await else {
        return GitHubSnapshot::default();
    };
    let url = serde_json::from_str::<serde_json::Value>(&repo_json)
        .ok()
        .and_then(|v| v.get("url").and_then(|u| u.as_str()).map(str::to_string));
    let Some(url) = url.filter(|u| !u.is_empty()) else {
        return GitHubSnapshot::default();
    };

    let limit = ITEM_LIMIT.to_string();
    let pr_args: Vec<&str> = vec!["pr", "list", "--json", PR_FIELDS, "--limit", &limit];
    let issue_args: Vec<&str> = vec!["issue", "list", "--json", ISSUE_FIELDS, "--limit", &limit];
    let (pr_json, issue_json) = tokio::join!(
        run_gh(gh_exe, folder, &pr_args),
        run_gh(gh_exe, folder, &issue_args)
    );
    GitHubSnapshot {
        url: Some(url),
        pulls: parse_items(pr_json.as_deref(), true),
        issues: parse_items(issue_json.as_deref(), false),
        available: true,
    }
}

/// The closed pull requests (merged ones included) or issues, newest first as
/// gh lists them. `None` is a failed call, as opposed to an empty list.
pub async fn closed_items(
    folder: &Path,
    gh_exe: &str,
    pull_requests: bool,
) -> Option<Vec<GitHubItem>> {
    let limit = ITEM_LIMIT.to_string();
    let (kind, fields) = if pull_requests {
        ("pr", PR_FIELDS)
    } else {
        ("issue", ISSUE_FIELDS)
    };
    let args = [
        kind, "list", "--state", "closed", "--json", fields, "--limit", &limit,
    ];
    let json = run_gh(gh_exe, folder, &args).await?;
    Some(parse_items(Some(&json), pull_requests))
}

/// One pull request's full view.
pub async fn pull_request(folder: &Path, gh_exe: &str, number: u64) -> Option<GitHubPull> {
    let number = number.to_string();
    let args = ["pr", "view", &number, "--json", PULL_VIEW_FIELDS];
    let json = run_gh(gh_exe, folder, &args).await?;
    parse_pull(&json)
}

/// The pull request's whole patch, cut into one chunk per file. `None` is a
/// failed call; an empty patch (no changed files) is an empty list.
pub async fn pull_diff(folder: &Path, gh_exe: &str, number: u64) -> Option<Vec<GitHubFilePatch>> {
    let number = number.to_string();
    let text = run_gh(gh_exe, folder, &["pr", "diff", &number]).await?;
    Some(split_unified_diff(&text))
}

/// One commit's changed files, patches included. The `{owner}`/`{repo}`
/// placeholders are gh's own, resolved from the repository in `folder`.
pub async fn commit_files(folder: &Path, gh_exe: &str, sha: &str) -> Option<Vec<GitHubFilePatch>> {
    let path = format!("repos/{{owner}}/{{repo}}/commits/{sha}");
    let json = run_gh(gh_exe, folder, &["api", &path]).await?;
    parse_commit_files(&json)
}

/// One verb on the pull request. gh's own words come back on a failure, which
/// is what branch protection and permissions actually say.
pub async fn act_on_pull(
    folder: &Path,
    gh_exe: &str,
    number: u64,
    action: PullAction,
) -> Result<(), String> {
    let argv = action.argv(number);
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();
    run_gh_checked(gh_exe, folder, &args).await.map(|_| ())
}

/// Opens the pull request and answers its number, read from the URL gh prints
/// on success. gh's own words come back on a failure — an unpushed branch, a
/// title GitHub refuses, no permission on the base.
pub async fn create_pull(
    folder: &Path,
    gh_exe: &str,
    request: &NewPullRequest,
) -> Result<u64, String> {
    let argv = request.argv();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();
    let answer = run_gh_checked(gh_exe, folder, &args).await?;
    pull_number_from(&answer)
        .ok_or_else(|| "gh did not name the pull request it created".to_string())
}

/// The number in the last `…/pull/<n>` gh printed.
fn pull_number_from(text: &str) -> Option<u64> {
    text.split_whitespace().rev().find_map(|word| {
        let (_, tail) = word.rsplit_once("/pull/")?;
        tail.trim_end_matches(|c: char| !c.is_ascii_digit())
            .parse()
            .ok()
    })
}

/// Merges with a merge commit. gh refuses to pick a method on its own when
/// nobody is at a prompt, and what the repository forbids comes back as the error.
pub async fn merge_pull_request(folder: &Path, gh_exe: &str, number: u64) -> Result<(), String> {
    let number = number.to_string();
    run_gh_checked(gh_exe, folder, &["pr", "merge", &number, "--merge"])
        .await
        .map(|_| ())
}

/// The repo metadata the rail shows (owner, language, license, topics…).
pub async fn repo_details(folder: &Path, gh_exe: &str) -> Option<GitHubRepoDetails> {
    let json = run_gh(
        gh_exe,
        folder,
        &[
            "repo",
            "view",
            "--json",
            "owner,createdAt,primaryLanguage,licenseInfo,repositoryTopics,defaultBranchRef,url",
        ],
    )
    .await?;
    parse_repo_details(&json)
}

fn parse_repo_details(json: &str) -> Option<GitHubRepoDetails> {
    let v = serde_json::from_str::<serde_json::Value>(json).ok()?;
    Some(GitHubRepoDetails {
        owner: v
            .pointer("/owner/login")
            .and_then(|x| x.as_str())
            .map(str::to_string),
        created_at: v
            .get("createdAt")
            .and_then(|x| x.as_str())
            .map(str::to_string),
        primary_language: v
            .pointer("/primaryLanguage/name")
            .and_then(|x| x.as_str())
            .map(str::to_string),
        license: v
            .pointer("/licenseInfo/name")
            .and_then(|x| x.as_str())
            .map(str::to_string),
        topics: v
            .get("repositoryTopics")
            .and_then(|x| x.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|t| t.get("name").and_then(|n| n.as_str()).map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
        default_branch: v
            .pointer("/defaultBranchRef/name")
            .and_then(|x| x.as_str())
            .map(str::to_string),
        url: v.get("url").and_then(|x| x.as_str()).map(str::to_string),
    })
}

fn parse_items(json: Option<&str>, is_pull_request: bool) -> Vec<GitHubItem> {
    let Some(json) = json else { return Vec::new() };
    let Ok(serde_json::Value::Array(items)) = serde_json::from_str::<serde_json::Value>(json)
    else {
        return Vec::new();
    };
    items
        .into_iter()
        .filter_map(|item| {
            let number = item.get("number").and_then(|n| n.as_u64())?;
            Some(GitHubItem {
                number,
                title: item
                    .get("title")
                    .and_then(|t| t.as_str())
                    .unwrap_or_default()
                    .to_string(),
                url: item
                    .get("url")
                    .and_then(|u| u.as_str())
                    .unwrap_or_default()
                    .to_string(),
                author: item
                    .pointer("/author/login")
                    .and_then(|a| a.as_str())
                    .map(str::to_string),
                labels: item
                    .get("labels")
                    .and_then(|l| l.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|l| {
                                l.get("name").and_then(|n| n.as_str()).map(str::to_string)
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                is_draft: item
                    .get("isDraft")
                    .and_then(|d| d.as_bool())
                    .unwrap_or(false),
                review_decision: item
                    .get("reviewDecision")
                    .and_then(|r| r.as_str())
                    .map(str::to_string),
                updated_at: text(&item, "updatedAt"),
                created_at: text(&item, "createdAt"),
                comments: item
                    .get("comments")
                    .and_then(|c| c.as_array())
                    .map_or(0, |c| c.len() as u32),
                head_ref_name: item
                    .get("headRefName")
                    .and_then(|b| b.as_str())
                    .map(str::to_string),
                base_ref_name: item
                    .get("baseRefName")
                    .and_then(|b| b.as_str())
                    .map(str::to_string),
                is_pull_request,
            })
        })
        .collect()
}

fn text(value: &serde_json::Value, key: &str) -> Option<String> {
    value.get(key).and_then(|v| v.as_str()).map(str::to_string)
}

fn parse_pull(json: &str) -> Option<GitHubPull> {
    let v = serde_json::from_str::<serde_json::Value>(json).ok()?;
    let number = v.get("number").and_then(|n| n.as_u64())?;
    let author_of = |item: &serde_json::Value| {
        item.pointer("/author/login")
            .and_then(|a| a.as_str())
            .unwrap_or("ghost")
            .to_string()
    };
    let list = |key: &str| {
        v.get(key)
            .and_then(|items| items.as_array())
            .cloned()
            .unwrap_or_default()
    };

    let comments = list("comments")
        .iter()
        .map(|c| GitHubComment {
            author: author_of(c),
            created_at: text(c, "createdAt"),
            body: text(c, "body").unwrap_or_default(),
        })
        .collect();

    // gh lists every review in order, and only a person's last one stands
    let mut reviews: Vec<GitHubReview> = Vec::new();
    for review in list("reviews") {
        let author = author_of(&review);
        let state = text(&review, "state").unwrap_or_default();
        // a comment-only review doesn't take back an approval
        if state == "COMMENTED" && reviews.iter().any(|r| r.author == author) {
            continue;
        }
        reviews.retain(|r| r.author != author);
        reviews.push(GitHubReview { author, state });
    }

    Some(GitHubPull {
        number,
        title: text(&v, "title").unwrap_or_default(),
        url: text(&v, "url").unwrap_or_default(),
        author: v
            .pointer("/author/login")
            .and_then(|a| a.as_str())
            .map(str::to_string),
        state: text(&v, "state").unwrap_or_default(),
        is_draft: v.get("isDraft").and_then(|d| d.as_bool()).unwrap_or(false),
        head_ref_name: text(&v, "headRefName").unwrap_or_default(),
        base_ref_name: text(&v, "baseRefName").unwrap_or_default(),
        additions: v.get("additions").and_then(|n| n.as_u64()).unwrap_or(0),
        deletions: v.get("deletions").and_then(|n| n.as_u64()).unwrap_or(0),
        mergeable: text(&v, "mergeable").unwrap_or_default(),
        merge_state: text(&v, "mergeStateStatus").unwrap_or_default(),
        review_decision: text(&v, "reviewDecision").filter(|d| !d.is_empty()),
        updated_at: text(&v, "updatedAt"),
        created_at: text(&v, "createdAt"),
        merged_at: text(&v, "mergedAt"),
        body: text(&v, "body").unwrap_or_default(),
        comments,
        reviews,
        checks: list("statusCheckRollup").iter().map(parse_check).collect(),
        files: list("files")
            .iter()
            .filter_map(|file| {
                Some(GitHubFilePatch {
                    path: text(file, "path")?,
                    status: "M".into(),
                    additions: file.get("additions").and_then(|n| n.as_u64()).unwrap_or(0),
                    deletions: file.get("deletions").and_then(|n| n.as_u64()).unwrap_or(0),
                    patch: String::new(),
                })
            })
            .collect(),
        commits: list("commits")
            .iter()
            .filter_map(|commit| {
                Some(GitHubCommit {
                    oid: text(commit, "oid")?,
                    headline: text(commit, "messageHeadline").unwrap_or_default(),
                    author: commit
                        .get("authors")
                        .and_then(|a| a.as_array())
                        .and_then(|a| a.first())
                        .and_then(|a| a.get("login"))
                        .and_then(|l| l.as_str())
                        .map(str::to_string),
                    committed_at: text(commit, "committedDate"),
                })
            })
            .collect(),
        // teams answer with a slug where a person answers with a login
        requested_reviewers: list("reviewRequests")
            .iter()
            .filter_map(|request| {
                text(request, "login")
                    .or_else(|| text(request, "slug"))
                    .or_else(|| text(request, "name"))
            })
            .collect(),
        auto_merge: v
            .get("autoMergeRequest")
            .is_some_and(|request| !request.is_null()),
    })
}

/// One commit's files as `gh api .../commits/<sha>` answers them.
fn parse_commit_files(json: &str) -> Option<Vec<GitHubFilePatch>> {
    let v = serde_json::from_str::<serde_json::Value>(json).ok()?;
    let files = v.get("files")?.as_array()?;
    Some(
        files
            .iter()
            .filter_map(|file| {
                let path = text(file, "filename")?;
                let status = match text(file, "status").unwrap_or_default().as_str() {
                    "added" => "A",
                    "removed" => "D",
                    "renamed" => "R",
                    _ => "M",
                };
                Some(GitHubFilePatch {
                    path,
                    status: status.into(),
                    additions: file.get("additions").and_then(|n| n.as_u64()).unwrap_or(0),
                    deletions: file.get("deletions").and_then(|n| n.as_u64()).unwrap_or(0),
                    patch: text(file, "patch").unwrap_or_default(),
                })
            })
            .collect(),
    )
}

/// Cuts a whole-patch text into one chunk per file: the `diff --git` header is
/// kept, so each chunk is a patch the kit's viewer draws on its own. The status
/// letter and the counts come from the chunk, which is why the rail does not
/// need a second source for either.
pub fn split_unified_diff(text: &str) -> Vec<GitHubFilePatch> {
    let mut files: Vec<GitHubFilePatch> = Vec::new();
    let mut current: Option<GitHubFilePatch> = None;
    let mut body = String::new();

    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            if let Some(mut file) = current.take() {
                file.patch = std::mem::take(&mut body);
                files.push(file);
            }
            body.push_str(line);
            body.push('\n');
            current = Some(GitHubFilePatch {
                path: header_path(rest),
                status: "M".into(),
                ..Default::default()
            });
            continue;
        }
        let Some(file) = current.as_mut() else {
            // preamble before the first file (a commit header, say) is not ours
            continue;
        };
        if line.starts_with("new file mode") {
            file.status = "A".into();
        } else if line.starts_with("deleted file mode") {
            file.status = "D".into();
        } else if line.starts_with("rename from") || line.starts_with("copy from") {
            file.status = "R".into();
        }
        // the `+++`/`---` header pair is not a changed line
        if line.starts_with('+') && !line.starts_with("+++") {
            file.additions += 1;
        } else if line.starts_with('-') && !line.starts_with("---") {
            file.deletions += 1;
        }
        body.push_str(line);
        body.push('\n');
    }
    if let Some(mut file) = current.take() {
        file.patch = body;
        files.push(file);
    }
    files
}

/// `a/src/foo.rs b/src/foo.rs` → `src/foo.rs`. A quoted header (a path with a
/// space in it) is unquoted; a rename's two paths resolve to the new one.
fn header_path(rest: &str) -> String {
    let rest = rest.trim();
    if !rest.starts_with('"')
        && let Some((_, right)) = rest.rsplit_once(" b/")
    {
        return right.to_string();
    }
    // "a/path with a space" "b/path with a space"
    let quoted: Vec<&str> = rest
        .split('"')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect();
    if let Some(last) = quoted.last() {
        return last.strip_prefix("b/").unwrap_or(last).to_string();
    }
    let left = rest.split_whitespace().next().unwrap_or_default();
    left.strip_prefix("a/").unwrap_or(left).to_string()
}

/// The rollup mixes two shapes: `CheckRun` (name, status, conclusion) and
/// `StatusContext` (context, state).
fn parse_check(item: &serde_json::Value) -> GitHubCheck {
    let word = |key: &str| text(item, key).unwrap_or_default();
    let state = if item.get("context").is_some() {
        match word("state").as_str() {
            "SUCCESS" => CheckState::Passed,
            "FAILURE" | "ERROR" => CheckState::Failed,
            _ => CheckState::Running,
        }
    } else if word("status") != "COMPLETED" {
        CheckState::Running
    } else {
        match word("conclusion").as_str() {
            "SUCCESS" => CheckState::Passed,
            "NEUTRAL" | "SKIPPED" => CheckState::Skipped,
            _ => CheckState::Failed,
        }
    };
    GitHubCheck {
        name: text(item, "name")
            .or_else(|| text(item, "context"))
            .unwrap_or_default(),
        state,
        url: text(item, "detailsUrl")
            .or_else(|| text(item, "targetUrl"))
            .filter(|url| !url.is_empty()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PULL: &str = r#"{
        "number": 482,
        "title": "Unify the diff gutter",
        "url": "https://github.com/o/r/pull/482",
        "author": {"login": "engin"},
        "state": "OPEN",
        "isDraft": false,
        "headRefName": "feat/gutter",
        "baseRefName": "main",
        "additions": 120,
        "deletions": 45,
        "mergeable": "MERGEABLE",
        "mergeStateStatus": "CLEAN",
        "reviewDecision": "",
        "updatedAt": "2026-11-01T10:00:00Z",
        "body": "Body",
        "comments": [{"author": {"login": "mert"}, "createdAt": "2026-11-01T09:00:00Z", "body": "Nice"}],
        "reviews": [
            {"author": {"login": "aykut"}, "state": "CHANGES_REQUESTED"},
            {"author": {"login": "aykut"}, "state": "APPROVED"},
            {"author": {"login": "aykut"}, "state": "COMMENTED"},
            {"author": {"login": "mert"}, "state": "COMMENTED"}
        ],
        "statusCheckRollup": [
            {"__typename": "CheckRun", "name": "build", "status": "COMPLETED", "conclusion": "SUCCESS", "detailsUrl": "https://ci/1"},
            {"__typename": "CheckRun", "name": "lint", "status": "COMPLETED", "conclusion": "SKIPPED"},
            {"__typename": "StatusContext", "context": "ci/legacy", "state": "SUCCESS", "targetUrl": ""}
        ]
    }"#;

    #[test]
    fn a_pull_request_reads_both_check_shapes() {
        let pull = parse_pull(PULL).expect("a pull request");

        assert_eq!(pull.number, 482);
        assert_eq!(pull.count_checks(CheckState::Passed), 2);
        assert_eq!(pull.count_checks(CheckState::Skipped), 1);
        assert_eq!(pull.checks[2].name, "ci/legacy");
        assert_eq!(pull.checks[2].url, None);
        assert_eq!(pull.review_decision, None);
        assert_eq!(pull.comments[0].author, "mert");
    }

    #[test]
    fn a_persons_last_review_stands() {
        let pull = parse_pull(PULL).expect("a pull request");

        assert_eq!(pull.approvals(), 1, "a later comment keeps the approval");
        assert_eq!(pull.reviews.len(), 2);
    }

    #[test]
    fn a_failing_or_running_check_blocks_the_merge() {
        let mut pull = parse_pull(PULL).expect("a pull request");
        assert!(pull.ready_to_merge());

        pull.checks.push(parse_check(&serde_json::json!({
            "name": "test", "status": "IN_PROGRESS", "conclusion": ""
        })));
        assert_eq!(pull.count_checks(CheckState::Running), 1);
        assert!(!pull.ready_to_merge());

        pull.checks.pop();
        pull.checks.push(parse_check(&serde_json::json!({
            "name": "test", "status": "COMPLETED", "conclusion": "TIMED_OUT"
        })));
        assert_eq!(pull.count_checks(CheckState::Failed), 1);
        assert!(!pull.ready_to_merge());
    }

    #[test]
    fn a_conflict_a_draft_or_a_merged_pull_blocks_the_merge() {
        let pull = parse_pull(PULL).expect("a pull request");
        for blocked in [
            GitHubPull {
                mergeable: "CONFLICTING".into(),
                ..pull.clone()
            },
            GitHubPull {
                is_draft: true,
                ..pull.clone()
            },
            GitHubPull {
                state: "MERGED".into(),
                ..pull.clone()
            },
        ] {
            assert!(!blocked.ready_to_merge());
        }
    }

    fn issue(number: u64, title: &str, created: &str, comments: u32) -> GitHubItem {
        GitHubItem {
            number,
            title: title.into(),
            labels: vec!["Bug".into()],
            created_at: Some(created.into()),
            updated_at: Some(created.into()),
            comments,
            ..Default::default()
        }
    }

    #[test]
    fn the_issue_sorts_order_their_rows() {
        let mut items = vec![
            issue(1, "a", "2026-01-01T00:00:00Z", 2),
            issue(2, "b", "2026-03-01T00:00:00Z", 0),
            issue(3, "c", "2026-02-01T00:00:00Z", 9),
        ];
        let numbers = |items: &[GitHubItem]| items.iter().map(|i| i.number).collect::<Vec<_>>();

        IssueSort::Newest.sort(&mut items);
        assert_eq!(numbers(&items), [2, 3, 1]);
        IssueSort::Oldest.sort(&mut items);
        assert_eq!(numbers(&items), [1, 3, 2]);
        IssueSort::Comments.sort(&mut items);
        assert_eq!(numbers(&items), [3, 1, 2]);
    }

    #[test]
    fn the_sort_survives_its_settings_word() {
        for sort in IssueSort::ALL {
            assert_eq!(IssueSort::from_key(Some(sort.key())), sort);
        }
        assert_eq!(IssueSort::from_key(None), IssueSort::Updated);
        assert_eq!(IssueSort::from_key(Some("gone")), IssueSort::Updated);
    }

    #[test]
    fn the_filter_reads_the_number_the_title_and_the_labels() {
        let item = issue(477, "Settings on Wayland", "2026-01-01T00:00:00Z", 0);

        assert!(item.matches("#477"));
        assert!(item.matches("wayland"));
        assert!(item.matches("bug"));
        assert!(!item.matches("windows"));
    }

    #[test]
    fn issues_count_their_comments_from_the_list() {
        let json = r#"[{"number": 7, "title": "t", "comments": [{}, {}], "createdAt": "2026-11-01T09:00:00Z"}]"#;

        let items = parse_items(Some(json), false);

        assert_eq!(items[0].comments, 2);
        assert_eq!(items[0].created_at.as_deref(), Some("2026-11-01T09:00:00Z"));
    }

    #[test]
    fn repo_details_read_the_nested_fields_gh_answers_with() {
        let json = r#"{
            "createdAt": "2026-09-18T10:00:00Z",
            "defaultBranchRef": {"name": "main"},
            "licenseInfo": {"name": "MIT License"},
            "owner": {"login": "enginkirmaci"},
            "primaryLanguage": {"name": "Rust"},
            "repositoryTopics": [{"name": "gpui"}, {"name": "rust"}],
            "url": "https://github.com/enginkirmaci/Warp Station"
        }"#;

        let details = parse_repo_details(json).expect("a details object");

        assert_eq!(details.owner.as_deref(), Some("enginkirmaci"));
        assert_eq!(details.primary_language.as_deref(), Some("Rust"));
        assert_eq!(details.license.as_deref(), Some("MIT License"));
        assert_eq!(details.default_branch.as_deref(), Some("main"));
        assert_eq!(details.topics, ["gpui", "rust"]);
    }

    /// gh answers `null`, not an empty value, for what a repo doesn't have.
    #[test]
    fn repo_details_tolerate_absent_fields() {
        let json = r#"{"licenseInfo": null, "primaryLanguage": null, "repositoryTopics": null}"#;

        let details = parse_repo_details(json).expect("a details object");

        assert_eq!(details.primary_language, None);
        assert_eq!(details.license, None);
        assert!(details.topics.is_empty());
    }

    const PULL_SURFACE: &str = r#"{
        "number": 7,
        "state": "OPEN",
        "headRefName": "feat/x",
        "baseRefName": "main",
        "createdAt": "2026-10-01T10:00:00Z",
        "mergedAt": null,
        "autoMergeRequest": {"enabledAt": "2026-10-02T10:00:00Z"},
        "files": [
            {"path": "src/main.rs", "additions": 12, "deletions": 3},
            {"path": "README.md", "additions": 1, "deletions": 0}
        ],
        "commits": [
            {"oid": "abc123", "messageHeadline": "Add the thing",
             "authors": [{"login": "engin"}], "committedDate": "2026-10-01T11:00:00Z"}
        ],
        "reviewRequests": [{"login": "mert"}, {"slug": "reviewers/team"}]
    }"#;

    #[test]
    fn a_pull_request_reads_its_files_commits_and_review_requests() {
        let pull = parse_pull(PULL_SURFACE).expect("a pull request");

        assert_eq!(pull.files.len(), 2);
        assert_eq!(pull.files[0].path, "src/main.rs");
        assert_eq!((pull.files[0].additions, pull.files[0].deletions), (12, 3));
        assert_eq!(pull.commits[0].oid, "abc123");
        assert_eq!(pull.commits[0].author.as_deref(), Some("engin"));
        assert_eq!(pull.requested_reviewers, ["mert", "reviewers/team"]);
        assert!(pull.auto_merge, "an armed auto-merge shows its badge");
        assert_eq!(pull.created_at.as_deref(), Some("2026-10-01T10:00:00Z"));
        assert_eq!(pull.merged_at, None);
    }

    /// gh omits a null it has no value for, and an unarmed auto-merge is null.
    #[test]
    fn a_pull_request_without_the_new_fields_is_still_a_pull_request() {
        let json = r#"{"number": 1, "state": "OPEN", "autoMergeRequest": null}"#;

        let pull = parse_pull(json).expect("a pull request");

        assert!(pull.files.is_empty());
        assert!(pull.commits.is_empty());
        assert!(pull.requested_reviewers.is_empty());
        assert!(!pull.auto_merge);
        assert_eq!(pull.state_word(), "Open");
    }

    const DIFF: &str = "diff --git a/src/main.rs b/src/main.rs\n\
                        index 111..222 100644\n\
                        --- a/src/main.rs\n\
                        +++ b/src/main.rs\n\
                        @@ -1,3 +1,4 @@\n\
                        -old\n\
                        +new\n\
                        +added\n\
                        diff --git a/docs/new.md b/docs/new.md\n\
                        new file mode 100644\n\
                        --- /dev/null\n\
                        +++ b/docs/new.md\n\
                        @@ -0,0 +1,2 @@\n\
                        +one\n\
                        +two\n";

    #[test]
    fn a_patch_is_cut_per_file_with_its_letter_and_counts() {
        let files = split_unified_diff(DIFF);

        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path, "src/main.rs");
        assert_eq!(files[0].status, "M");
        assert_eq!((files[0].additions, files[0].deletions), (2, 1));
        assert!(files[0].patch.starts_with("diff --git a/src/main.rs"));
        assert!(files[0].patch.contains("+added"));
        assert!(
            !files[0].patch.contains("docs/new.md"),
            "chunks do not leak"
        );

        assert_eq!(files[1].path, "docs/new.md");
        assert_eq!(files[1].status, "A");
        assert_eq!((files[1].additions, files[1].deletions), (2, 0));
    }

    #[test]
    fn a_removal_and_a_rename_take_their_own_letters() {
        let diff = "diff --git a/gone.rs b/gone.rs\n\
                    deleted file mode 100644\n\
                    --- a/gone.rs\n\
                    +++ /dev/null\n\
                    @@ -1 +0,0 @@\n\
                    -bye\n\
                    diff --git a/old.rs b/new.rs\n\
                    similarity index 90%\n\
                    rename from old.rs\n\
                    rename to new.rs\n";

        let files = split_unified_diff(diff);

        assert_eq!(files[0].status, "D");
        assert_eq!((files[0].additions, files[0].deletions), (0, 1));
        assert_eq!(files[1].path, "new.rs");
        assert_eq!(files[1].status, "R");
    }

    #[test]
    fn a_patch_without_files_is_empty_not_a_panic() {
        assert!(split_unified_diff("").is_empty());
        assert!(split_unified_diff("nothing to see\n").is_empty());
    }

    #[test]
    fn commit_files_read_the_api_shape() {
        let json = r#"{"files": [
            {"filename": "a.rs", "status": "added", "additions": 4, "deletions": 0, "patch": "@@ -0,0 +1 @@\n+x"},
            {"filename": "b.rs", "status": "modified", "additions": 1, "deletions": 2, "patch": "@@\n-y\n+z"},
            {"filename": "c.rs", "status": "renamed", "additions": 0, "deletions": 0}
        ]}"#;

        let files = parse_commit_files(json).expect("a commit");

        assert_eq!(files.len(), 3);
        assert_eq!(files[0].status, "A");
        assert_eq!(files[1].status, "M");
        assert_eq!(files[2].status, "R");
        assert_eq!(files[1].patch, "@@\n-y\n+z");
        assert!(parse_commit_files("{}").is_none(), "no files key at all");
    }

    #[test]
    fn every_verb_maps_to_one_gh_call() {
        assert_eq!(
            PullAction::Approve.argv(482),
            ["pr", "review", "482", "--approve"]
        );
        assert_eq!(
            PullAction::RequestChanges("No.".into()).argv(482),
            ["pr", "review", "482", "--request-changes", "--body", "No."]
        );
        assert_eq!(
            PullAction::Comment("Nice.".into()).argv(482),
            ["pr", "comment", "482", "--body", "Nice."]
        );
        assert_eq!(PullAction::Ready.argv(482), ["pr", "ready", "482"]);
        assert_eq!(
            PullAction::Draft.argv(482),
            ["pr", "ready", "482", "--undo"]
        );
        assert_eq!(PullAction::Close.argv(482), ["pr", "close", "482"]);
        assert_eq!(PullAction::Reopen.argv(482), ["pr", "reopen", "482"]);
        assert_eq!(PullAction::Close.done_word(482), "Closed #482");
    }

    fn new_pull(title: &str, head: &str, base: &str) -> NewPullRequest {
        NewPullRequest {
            title: title.into(),
            body: "Body".into(),
            base: base.into(),
            head: head.into(),
            reviewer: None,
            draft: false,
        }
    }

    #[test]
    fn a_new_pull_request_maps_to_gh_pr_create() {
        let request = new_pull("Add the thing", "feat/x", "main");

        assert_eq!(
            request.argv(),
            [
                "pr",
                "create",
                "--title",
                "Add the thing",
                "--body",
                "Body",
                "--base",
                "main",
                "--head",
                "feat/x"
            ]
        );
        assert_eq!(request.problem(), None);
    }

    #[test]
    fn a_draft_and_a_reviewer_ride_along() {
        let request = NewPullRequest {
            draft: true,
            reviewer: Some("mert".into()),
            ..new_pull("Add the thing", "feat/x", "main")
        };
        let argv = request.argv();

        assert!(argv.contains(&"--draft".to_string()));
        assert_eq!(argv[argv.len() - 2..], ["--reviewer", "mert"]);

        // an empty reviewer is no reviewer, and no flag
        let blank = NewPullRequest {
            reviewer: Some("  ".into()),
            ..new_pull("Add the thing", "feat/x", "main")
        };
        assert!(!blank.argv().contains(&"--reviewer".to_string()));
    }

    /// The form refuses what gh would refuse, before it spends a process on it.
    #[test]
    fn the_form_says_what_is_missing() {
        assert_eq!(
            new_pull("  ", "feat/x", "main").problem(),
            Some("A title is required.")
        );
        assert_eq!(
            new_pull("Add the thing", "", "main").problem(),
            Some("Source and base must both name a branch.")
        );
        assert_eq!(
            new_pull("Add the thing", "main", "main").problem(),
            Some("Source and base must be different branches.")
        );
    }

    #[test]
    fn the_created_number_is_read_from_the_url_gh_prints() {
        assert_eq!(
            pull_number_from("https://github.com/o/r/pull/482\n"),
            Some(482)
        );
        assert_eq!(
            pull_number_from(
                "Creating pull request for feat/x into main\n\nhttps://github.com/o/r/pull/7"
            ),
            Some(7)
        );
        assert_eq!(pull_number_from("no url here"), None);
    }

    /// A repository that wants a review blocks the merge before any check runs.
    #[test]
    fn a_required_review_blocks_the_merge() {
        let pull = parse_pull(PULL).expect("a pull request");
        assert!(pull.ready_to_merge());

        let blocked = GitHubPull {
            review_decision: Some("REVIEW_REQUIRED".into()),
            ..pull
        };
        assert!(!blocked.ready_to_merge());
    }
}
