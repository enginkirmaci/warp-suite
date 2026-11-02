//! The GitHub activity service: per-repo `gh` queries run inside the repo
//! folder (gh resolves owner/name from the git remote). One `gh repo view`
//! proves the repo lives on GitHub and yields its link, then the open PR and
//! issue lists feed the column chips and the details tabs. The closed lists and
//! one pull request's full view load on demand. Payloads are parsed defensively
//! from serde_json Values — gh's shapes drift.

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
                                updatedAt,body,comments,reviews,statusCheckRollup";

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

    pub fn has_conflicts(&self) -> bool {
        self.mergeable == "CONFLICTING"
    }

    /// Open, not a draft, no conflict and no failing or running check. What
    /// branch protection still refuses, the merge itself reports.
    pub fn ready_to_merge(&self) -> bool {
        self.state == "OPEN"
            && !self.is_draft
            && !self.has_conflicts()
            && self.count_checks(CheckState::Failed) == 0
            && self.count_checks(CheckState::Running) == 0
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
        body: text(&v, "body").unwrap_or_default(),
        comments,
        reviews,
        checks: list("statusCheckRollup").iter().map(parse_check).collect(),
    })
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
}
