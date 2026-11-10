//! The Azure DevOps integration: the remote parser (a repo's coordinates out
//! of .git/config — public dev.azure.com, the legacy visualstudio.com host,
//! the v3 SSH form, and company-hosted Azure DevOps Server when a custom URL
//! is configured) plus the REST client (repo metadata, active pull requests,
//! WIQL work items, pipeline runs) behind the PAT Basic auth.

use serde::Deserialize;
use std::path::Path;

const ITEM_LIMIT: usize = 50;
const BUILD_FETCH_LIMIT: usize = 30;
const PIPELINE_LIMIT: usize = 10;

const WORK_ITEM_QUERY: &str = "SELECT [System.Id] FROM WorkItems \
WHERE [System.TeamProject] = @project \
AND [System.State] NOT IN ('Done', 'Closed', 'Removed', 'Completed') \
ORDER BY [System.ChangedDate] DESC";

const WORK_ITEM_FIELDS: &str =
    "System.Id,System.Title,System.State,System.WorkItemType,System.AssignedTo";

// ---- remote parsing -----------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct AzureRemote {
    /// API base up to the organization (https://dev.azure.com/{org}).
    pub base_url: String,
    pub project: String,
    pub repository: String,
}

/// Reads a remote URL from .git/config; `None`-name picks the first remote.
pub fn read_remote_url(folder: &Path, name: Option<&str>) -> Option<String> {
    let config = folder.join(".git").join("config");
    let text = std::fs::read_to_string(config).ok()?;
    let mut in_section = false;
    for raw in text.lines() {
        let line = raw.trim();
        if line.starts_with('[') {
            in_section = match name {
                Some(n) => line.eq_ignore_ascii_case(&format!("[remote \"{n}\"]")),
                None => line.to_lowercase().starts_with("[remote"),
            };
            continue;
        }
        if !in_section {
            continue;
        }
        if let Some((key, value)) = line.split_once('=')
            && key.trim().eq_ignore_ascii_case("url")
        {
            let value = value.trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

/// Trims the custom server URL and adds the https scheme when written bare.
pub fn normalize_server_url(value: &str) -> Option<String> {
    let trimmed = value.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.to_lowercase().starts_with("http://")
        || trimmed.to_lowercase().starts_with("https://")
    {
        Some(trimmed.to_string())
    } else {
        Some(format!("https://{trimmed}"))
    }
}

/// The custom base's host (the matching key for company-hosted remotes).
fn custom_host(custom_base: Option<&str>) -> Option<String> {
    let base = normalize_server_url(custom_base?)?;
    let rest = base.split("://").nth(1)?;
    let authority = rest.split('/').next()?;
    let host = authority.rsplit('@').next()?.split(':').next()?;
    Some(host.to_string())
}

fn escape(segment: &str) -> String {
    // per-segment escaping for URL path safety
    segment.replace(' ', "%20")
}

fn strip_dot_git(segment: &str) -> String {
    segment
        .strip_suffix(".git")
        .or_else(|| segment.strip_suffix(".GIT"))
        .unwrap_or(segment)
        .to_string()
}

/// Parses a git remote URL for Azure DevOps coordinates.
pub fn parse_url(url: &str, custom_base: Option<&str>) -> Option<AzureRemote> {
    // SSH: git@ssh.dev.azure.com:v3/{org}/{project}/{repo}
    if let Some(idx) = url.find("ssh.dev.azure.com:v3/") {
        let rest = &url[idx + "ssh.dev.azure.com:v3/".len()..];
        let parts: Vec<&str> = rest.trim_end_matches('/').split('/').collect();
        if parts.len() < 3 {
            return None;
        }
        return Some(AzureRemote {
            base_url: format!("https://dev.azure.com/{}", escape(parts[0])),
            project: escape(parts[1]),
            repository: escape(&strip_dot_git(parts[2])),
        });
    }

    if url.to_lowercase().starts_with("ssh://") {
        return None; // custom-host SSH: rare; the HTTPS/HTTPS-form coverage suffices
    }

    let lower = url.to_lowercase();
    if !lower.starts_with("http://") && !lower.starts_with("https://") {
        return None;
    }
    let (scheme, rest) = url.split_once("://")?;
    let (authority, path) = match rest.split_once('/') {
        Some((a, p)) => (a, p),
        None => (rest, ""),
    };
    let host = authority
        .rsplit('@')
        .next()?
        .split(':')
        .next()?
        .to_lowercase();
    let segments: Vec<String> = path
        .split('/')
        .filter(|s| !s.is_empty())
        .map(percent_decode)
        .collect();

    if host == "dev.azure.com" {
        // https://dev.azure.com/{org}/{project}/_git/{repo}
        if segments.len() >= 4 && segments[2].eq_ignore_ascii_case("_git") {
            return Some(AzureRemote {
                base_url: format!("https://dev.azure.com/{}", escape(&segments[0])),
                project: escape(&segments[1]),
                repository: escape(&strip_dot_git(&segments[3])),
            });
        }
        return None;
    }

    // Legacy: https://{org}.visualstudio.com/{project}/_git/{repo}
    if let Some(org) = host.strip_suffix(".visualstudio.com")
        && segments.len() >= 3
        && segments[1].eq_ignore_ascii_case("_git")
    {
        return Some(AzureRemote {
            base_url: format!("https://dev.azure.com/{}", escape(org)),
            project: escape(&segments[0]),
            repository: escape(&strip_dot_git(&segments[2])),
        });
    }

    // Custom server: {scheme}://{host}[/app-path]/{collection}/{project}/_git/{repo}
    // (the app-path arrives via the segments themselves, never from the
    // configured base)
    if custom_host(custom_base).is_some_and(|h| h == host) {
        return from_git_segments(&format!("{scheme}://{authority}"), &segments);
    }
    None
}

fn from_git_segments(base_without_path: &str, segments: &[String]) -> Option<AzureRemote> {
    for i in 1..segments.len().saturating_sub(1) {
        if segments[i].eq_ignore_ascii_case("_git") {
            let org_path = segments[..i - 1]
                .iter()
                .map(|s| escape(s))
                .collect::<Vec<_>>()
                .join("/");
            return Some(AzureRemote {
                base_url: if org_path.is_empty() {
                    base_without_path.to_string()
                } else {
                    format!("{base_without_path}/{org_path}")
                },
                project: escape(&segments[i - 1]),
                repository: escape(&strip_dot_git(&segments[i + 1])),
            });
        }
    }
    None
}

fn percent_decode(segment: &str) -> String {
    // minimal %XX decoding for the path segments we compare — decode at the
    // byte level and reassemble as UTF-8, so "%C3%A9" is "é" rather than the
    // per-byte Latin-1 mangling a `byte as char` cast produces
    let bytes = segment.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(hex) = segment.get(i + 1..i + 3)
            && let Ok(byte) = u8::from_str_radix(hex, 16)
        {
            out.push(byte);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

// ---- REST client ---------------------------------------------------------------

#[derive(Clone, Debug, Default)]
pub struct AzureItem {
    pub number: u64,
    pub title: String,
    pub url: String,
    pub author: Option<String>,
    pub labels: Vec<String>,
    pub is_draft: bool,
    pub state: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct PipelineRun {
    pub id: u64,
    pub build_number: String,
    pub definition_name: Option<String>,
    pub branch: Option<String>,
    pub result: Option<String>,
    pub status: Option<String>,
    pub url: Option<String>,
    /// The commit the build ran on.
    pub source_version: Option<String>,
    /// Why it ran: `individualCI`, `manual`, `schedule`, `pullRequest`…
    pub reason: Option<String>,
    /// ISO timestamps, as Azure answers them.
    pub start: Option<String>,
    pub finish: Option<String>,
}

impl PipelineRun {
    pub fn is_running(&self) -> bool {
        !self.is_failed()
            && !self.is_succeeded()
            && matches!(
                self.status.as_deref(),
                Some("notStarted" | "inProgress" | "postponed")
            )
    }
    pub fn is_failed(&self) -> bool {
        matches!(self.result.as_deref(), Some("failed" | "canceled"))
    }
    pub fn is_succeeded(&self) -> bool {
        matches!(
            self.result.as_deref(),
            Some("succeeded" | "partiallySucceeded")
        )
    }
    pub fn display_branch(&self) -> Option<String> {
        self.branch
            .as_ref()
            .map(|b| b.strip_prefix("refs/heads/").unwrap_or(b).to_string())
    }

    pub fn short_source(&self) -> Option<String> {
        self.source_version
            .as_deref()
            .filter(|sha| !sha.trim().is_empty())
            .map(|sha| sha.chars().take(7).collect())
    }

    /// The run in one word, as the row and the detail's pill read it.
    pub fn state_word(&self) -> &'static str {
        if self.is_running() {
            "Running"
        } else if self.is_failed() {
            "Failed"
        } else if self.is_succeeded() {
            "Succeeded"
        } else if self.result.as_deref() == Some("skipped") {
            "Skipped"
        } else {
            "Queued"
        }
    }

    /// The trigger in the app's words, when Azure named one.
    pub fn trigger_word(&self) -> Option<&'static str> {
        match self.reason.as_deref()? {
            "individualCI" | "batchedCI" => Some("Continuous integration"),
            "manual" => Some("Manual"),
            "schedule" => Some("Schedule"),
            "pullRequest" => Some("Pull request"),
            "buildCompletion" => Some("Build completion"),
            _ => None,
        }
    }
}

/// One job inside a run's stage, as the timeline lists it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RunJob {
    pub name: String,
    /// `inProgress`, `completed` or `pending`.
    pub state: Option<String>,
    /// `succeeded`, `failed`, `partiallySucceeded`, `canceled` or `skipped`.
    pub result: Option<String>,
    pub start: Option<String>,
    pub finish: Option<String>,
}

impl RunJob {
    pub fn is_failed(&self) -> bool {
        matches!(
            self.result.as_deref(),
            Some("failed" | "canceled" | "partiallySucceeded")
        )
    }

    pub fn is_running(&self) -> bool {
        !self.is_failed() && self.state.as_deref() == Some("inProgress")
    }

    pub fn is_done(&self) -> bool {
        self.state.as_deref() == Some("completed") && !self.is_failed()
    }
}

/// One stage of a run, with the jobs it holds. The design draws one card per
/// stage and the jobs under it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RunStage {
    pub name: String,
    pub jobs: Vec<RunJob>,
}

impl RunStage {
    pub fn failed(&self) -> usize {
        self.jobs.iter().filter(|job| job.is_failed()).count()
    }

    pub fn done(&self) -> usize {
        self.jobs.iter().filter(|job| job.is_done()).count()
    }
}

/// One run's detail: the stage cards and the tail of its log.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RunDetail {
    pub stages: Vec<RunStage>,
    pub log: String,
}

#[derive(Clone, Debug, Default)]
pub struct AzureSnapshot {
    pub url: Option<String>,
    pub pulls: Vec<AzureItem>,
    pub work_items: Vec<AzureItem>,
    pub pipelines: Vec<PipelineRun>,
    pub available: bool,
}

impl AzureSnapshot {
    pub fn pipeline_running(&self) -> bool {
        self.pipelines.first().is_some_and(|p| p.is_running())
    }
    pub fn pipeline_failed(&self) -> bool {
        self.pipelines.first().is_some_and(|p| p.is_failed())
    }
    pub fn pipeline_ok(&self) -> bool {
        self.pipelines.first().is_some_and(|p| p.is_succeeded())
    }
}

/// The PAT from settings or the environment fallbacks.
pub fn resolve_pat(configured: &str) -> Option<String> {
    if !configured.trim().is_empty() {
        return Some(configured.trim().to_string());
    }
    for name in ["AZURE_DEVOPS_PAT", "AZURE_DEVOPS_EXT_PAT"] {
        if let Ok(value) = std::env::var(name)
            && !value.trim().is_empty()
        {
            return Some(value.trim().to_string());
        }
    }
    None
}

/// The repository's Azure coordinates, from its first remote and the
/// configured custom server URL. Every per-thing call (a run's timeline, a pull
/// request's threads) resolves them the same way the pass does.
pub fn remote_for(folder: &Path, custom_base: &str) -> Option<AzureRemote> {
    let url = read_remote_url(folder, None)?;
    parse_url(&url, Some(custom_base))
}

/// One repo's full Azure DevOps probe. A 401 anywhere means the token is bad
/// — the snapshot comes back unavailable without hammering the API.
pub async fn refresh(folder: &Path, pat: &str, custom_base: &str) -> AzureSnapshot {
    let Some(url) = read_remote_url(folder, None) else {
        return AzureSnapshot::default();
    };
    let Some(remote) = parse_url(&url, Some(custom_base)) else {
        return AzureSnapshot::default();
    };
    let client = Client::new(pat);

    // repo metadata proves the remote resolves and yields the web link
    let meta = client
        .get_json::<serde_json::Value>(&format!(
            "{}/{}/_apis/git/repositories/{}?api-version=7.1",
            remote.base_url, remote.project, remote.repository
        ))
        .await;
    let Some(meta) = meta else {
        return AzureSnapshot::default();
    };
    if meta.get("id").is_none() {
        return AzureSnapshot::default();
    }
    let repo_id = meta
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let web_url = meta
        .pointer("/_links/web/href")
        .and_then(|v| v.as_str())
        .map(str::to_string);

    let (pulls, work_items, pipelines) = tokio::join!(
        fetch_pulls(&client, &remote, &repo_id),
        fetch_work_items(&client, &remote),
        fetch_pipelines(&client, &remote, &repo_id)
    );

    AzureSnapshot {
        url: web_url,
        pulls,
        work_items,
        pipelines,
        available: true,
    }
}

async fn fetch_pulls(client: &Client, remote: &AzureRemote, repo_id: &str) -> Vec<AzureItem> {
    let Some(payload) = client
        .get_json::<serde_json::Value>(&format!(
            "{}/{}/_apis/git/repositories/{}/pullrequests?api-version=7.1&searchCriteria.status=active&$top={ITEM_LIMIT}",
            remote.base_url, remote.project, repo_id
        ))
        .await
    else {
        return Vec::new();
    };
    let mut items: Vec<AzureItem> = payload
        .get("value")
        .and_then(|v| v.as_array())
        .map(|rows| {
            rows.iter()
                .filter_map(|p| {
                    let number = p.get("pullRequestId").and_then(|v| v.as_u64())?;
                    Some(AzureItem {
                        number,
                        title: p
                            .get("title")
                            .and_then(|t| t.as_str())
                            .unwrap_or_default()
                            .to_string(),
                        url: p
                            .pointer("/_links/web/href")
                            .and_then(|u| u.as_str())
                            .unwrap_or_default()
                            .to_string(),
                        author: p
                            .pointer("/createdBy/displayName")
                            .and_then(|a| a.as_str())
                            .or_else(|| p.pointer("/createdBy/uniqueName").and_then(|a| a.as_str()))
                            .map(str::to_string),
                        labels: p
                            .get("labels")
                            .and_then(|l| l.as_array())
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|l| {
                                        l.get("name")
                                            .and_then(|n| n.as_str())
                                            .filter(|n| !n.trim().is_empty())
                                            .map(str::to_string)
                                    })
                                    .collect()
                            })
                            .unwrap_or_default(),
                        is_draft: p.get("isDraft").and_then(|d| d.as_bool()).unwrap_or(false),
                        state: None,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    items.sort_by_key(|i| i.number);
    items
}

async fn fetch_work_items(client: &Client, remote: &AzureRemote) -> Vec<AzureItem> {
    // WIQL for the ids, then one batch call for the fields (WIQL results
    // carry ids only)
    #[derive(serde::Serialize)]
    struct WiqlRequest<'a> {
        query: &'a str,
    }
    let wiql_url = format!(
        "{}/{}/_apis/wit/wiql?api-version=7.1&$top={ITEM_LIMIT}",
        remote.base_url, remote.project
    );
    let wiql_body = WiqlRequest {
        query: WORK_ITEM_QUERY,
    };
    let wiql_result: Option<serde_json::Value> = client.post_json(&wiql_url, &wiql_body).await;
    let Some(wiql) = wiql_result else {
        return Vec::new();
    };
    let ids: Vec<u64> = wiql
        .get("workItems")
        .and_then(|w| w.as_array())
        .map(|rows| {
            rows.iter()
                .filter_map(|w| w.get("id").and_then(|i| i.as_u64()))
                .collect()
        })
        .unwrap_or_default();
    if ids.is_empty() {
        return Vec::new();
    }
    let id_list = ids
        .iter()
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let Some(payload) = client
        .get_json::<serde_json::Value>(&format!(
            "{}/{}/_apis/wit/workitems?api-version=7.1&ids={id_list}&fields={WORK_ITEM_FIELDS}",
            remote.base_url, remote.project
        ))
        .await
    else {
        return Vec::new();
    };
    let mut items: Vec<AzureItem> = payload
        .get("value")
        .and_then(|v| v.as_array())
        .map(|rows| {
            rows.iter()
                .filter_map(|w| {
                    let id = w.get("id").and_then(|i| i.as_u64())?;
                    let field = |key: &str| -> Option<String> {
                        let value = w.pointer(&format!("/fields/{key}"))?;
                        if let Some(s) = value.as_str() {
                            Some(s.to_string())
                        } else {
                            value
                                .get("displayName")
                                .and_then(|n| n.as_str())
                                .map(str::to_string)
                        }
                    };
                    Some(AzureItem {
                        number: id,
                        title: field("System.Title")?,
                        url: format!(
                            "{}/{}/_workitems/edit/{id}",
                            remote.base_url, remote.project
                        ),
                        author: field("System.AssignedTo"),
                        labels: field("System.WorkItemType")
                            .map(|t| vec![t])
                            .unwrap_or_default(),
                        is_draft: false,
                        state: field("System.State"),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    items.sort_by_key(|i| i.number);
    items
}

async fn fetch_pipelines(client: &Client, remote: &AzureRemote, repo_id: &str) -> Vec<PipelineRun> {
    let Some(payload) = client
        .get_json::<serde_json::Value>(&format!(
            "{}/{}/_apis/build/builds?api-version=7.1&$top={BUILD_FETCH_LIMIT}&queryOrder=queueTimeDescending",
            remote.base_url, remote.project
        ))
        .await
    else {
        return Vec::new();
    };
    // builds have no server-side repository filter — client-side by repo id
    payload
        .get("value")
        .and_then(|v| v.as_array())
        .map(|rows| {
            rows.iter()
                .filter(|b| {
                    b.pointer("/repository/id")
                        .and_then(|r| r.as_str())
                        .is_some_and(|r| r.eq_ignore_ascii_case(repo_id))
                })
                .take(PIPELINE_LIMIT)
                .filter_map(parse_build)
                .collect()
        })
        .unwrap_or_default()
}

/// One build record. Defensive like the other payloads: a build without an id
/// is not a run the app can open.
fn parse_build(b: &serde_json::Value) -> Option<PipelineRun> {
    let text = |key: &str| b.get(key).and_then(|v| v.as_str()).map(str::to_string);
    Some(PipelineRun {
        id: b.get("id").and_then(|i| i.as_u64())?,
        build_number: text("buildNumber").unwrap_or_default(),
        definition_name: b
            .pointer("/definition/name")
            .and_then(|n| n.as_str())
            .map(str::to_string),
        branch: b
            .pointer("/sourceBranch")
            .and_then(|n| n.as_str())
            .map(str::to_string),
        result: text("result"),
        status: text("status"),
        url: b
            .pointer("/_links/web/href")
            .and_then(|u| u.as_str())
            .map(str::to_string),
        source_version: text("sourceVersion"),
        reason: text("reason"),
        start: text("startTime"),
        finish: text("finishTime"),
    })
}

// ---- one pull request ----------------------------------------------------------

/// One reviewer on an Azure pull request, with their vote.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AzureReviewer {
    pub id: String,
    pub name: String,
    pub unique_name: Option<String>,
    /// 10 approved, 5 approved with suggestions, 0 no vote, -5 waiting,
    /// -10 rejected.
    pub vote: i64,
    pub required: bool,
}

impl AzureReviewer {
    pub fn is_approved(&self) -> bool {
        self.vote >= 5
    }

    pub fn is_rejected(&self) -> bool {
        self.vote <= -10
    }

    /// The vote in the app's words.
    pub fn vote_word(&self) -> &'static str {
        match self.vote {
            v if v >= 10 => "Approved",
            v if v >= 5 => "Approved with suggestions",
            v if v <= -10 => "Rejected",
            v if v <= -5 => "Waiting for author",
            _ => "No vote",
        }
    }
}

/// One policy evaluation: a required reviewer, a build validation, a comment
/// requirement or the merge status itself.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AzurePolicy {
    pub name: String,
    /// The policy's own kind, which says what the row is about.
    pub kind: String,
    /// `approved`, `rejected`, `queued`, `running`, `notApplicable`.
    pub status: String,
}

impl AzurePolicy {
    /// A policy that still stands between the pull request and completion.
    pub fn is_blocking(&self) -> bool {
        matches!(self.status.as_str(), "rejected" | "queued" | "running")
    }

    pub fn is_running(&self) -> bool {
        matches!(self.status.as_str(), "queued" | "running")
    }

    pub fn word(&self) -> &'static str {
        match self.status.as_str() {
            "approved" => "Approved",
            "rejected" => "Rejected",
            "queued" => "Queued",
            "running" => "Running",
            "notApplicable" => "Not applicable",
            _ => "Waiting",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AzureComment {
    pub author: String,
    pub when: Option<String>,
    pub text: String,
}

/// One comment thread. Azure keeps its resolution on the thread, not the
/// comment, which is what the "comments must be resolved" policy reads.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AzureThread {
    /// `active`, `fixed`, `wontFix`, `closed`, `pending`.
    pub status: String,
    pub comments: Vec<AzureComment>,
}

impl AzureThread {
    pub fn is_open(&self) -> bool {
        matches!(self.status.as_str(), "active" | "pending")
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AzureCommit {
    pub id: String,
    pub message: String,
    pub author: Option<String>,
    pub when: Option<String>,
}

impl AzureCommit {
    pub fn short_id(&self) -> String {
        self.id.chars().take(7).collect()
    }
}

/// One changed file. Azure hands out no patch with the change list, so the
/// line counts come from the file's own diff when it is opened.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AzureFile {
    pub path: String,
    /// `add`, `edit`, `delete`, `rename`, `undelete`, or a comma-joined pair.
    pub change: String,
}

impl AzureFile {
    /// The rail's letter.
    pub fn letter(&self) -> &'static str {
        let change = self.change.to_ascii_lowercase();
        if change.contains("delete") {
            "D"
        } else if change.contains("add") || change.contains("undelete") {
            "A"
        } else if change.contains("rename") {
            "R"
        } else {
            "M"
        }
    }
}

/// One Azure pull request's record.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AzurePull {
    pub number: u64,
    pub title: String,
    /// `active`, `completed`, `abandoned`.
    pub status: String,
    pub is_draft: bool,
    pub source: String,
    pub target: String,
    pub author: Option<String>,
    pub created: Option<String>,
    pub closed: Option<String>,
    pub url: Option<String>,
    /// The pull request's own words, as markdown.
    pub description: Option<String>,
    /// `succeeded`, `conflicts`, `rejectedByPolicy`, `notSet`…
    pub merge_status: Option<String>,
    pub reviewers: Vec<AzureReviewer>,
    /// Armed for completion when the policies clear.
    pub auto_complete: bool,
    /// The commit the completion call has to name.
    pub last_merge_source: Option<String>,
    /// The signed-in user, for the reviewer vote.
    pub me: Option<String>,
}

impl AzurePull {
    pub fn is_active(&self) -> bool {
        self.status == "active"
    }

    pub fn is_completed(&self) -> bool {
        self.status == "completed"
    }

    pub fn is_abandoned(&self) -> bool {
        self.status == "abandoned"
    }

    /// The pill's word. Azure names the states Active/Completed/Abandoned.
    pub fn state_word(&self) -> &'static str {
        if self.is_completed() {
            "Completed"
        } else if self.is_abandoned() {
            "Abandoned"
        } else if self.is_draft {
            "Draft"
        } else {
            "Active"
        }
    }

    pub fn source_branch(&self) -> String {
        short_ref(&self.source)
    }

    pub fn target_branch(&self) -> String {
        short_ref(&self.target)
    }

    pub fn approved(&self) -> usize {
        self.reviewers.iter().filter(|r| r.is_approved()).count()
    }

    pub fn rejected(&self) -> usize {
        self.reviewers.iter().filter(|r| r.is_rejected()).count()
    }

    /// The signed-in reviewer, when they are on the list already.
    pub fn my_reviewer(&self) -> Option<&AzureReviewer> {
        let me = self.me.as_deref()?;
        self.reviewers
            .iter()
            .find(|reviewer| reviewer.id.eq_ignore_ascii_case(me))
    }

    pub fn has_conflicts(&self) -> bool {
        self.merge_status.as_deref() == Some("conflicts")
    }

    /// Active, not a draft, no conflict, and no blocking policy left. What
    /// Azure still refuses comes back from the complete call itself.
    pub fn ready_to_complete(&self, policies: &[AzurePolicy]) -> bool {
        self.is_active()
            && !self.is_draft
            && !self.has_conflicts()
            && !policies.iter().any(AzurePolicy::is_blocking)
    }
}

/// The record, its policies and its threads: what the Overview draws, asked
/// for in one go.
#[derive(Clone, Debug, Default)]
pub struct AzurePullDetail {
    pub pull: AzurePull,
    pub policies: Vec<AzurePolicy>,
    pub threads: Vec<AzureThread>,
}

/// The verb the detail sends. Each is one REST call, and this is the mapping
/// the app keeps its copy of the rules in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AzurePullAction {
    /// A reviewer vote: 10 approves, -10 rejects.
    Vote {
        reviewer: String,
        vote: i64,
    },
    Complete,
    /// Reopen an abandoned one.
    Reopen,
    Abandon,
    SetAutoComplete {
        reviewer: String,
    },
    CancelAutoComplete,
}

impl AzurePullAction {
    /// The HTTP method the verb needs.
    pub fn method(&self) -> &'static str {
        match self {
            Self::CancelAutoComplete => "DELETE",
            Self::Complete | Self::Reopen | Self::Abandon => "PATCH",
            Self::Vote { .. } | Self::SetAutoComplete { .. } => "PUT",
        }
    }

    /// What the toast says when the call lands.
    pub fn done_word(&self, number: u64) -> String {
        match self {
            Self::Vote { vote, .. } if *vote >= 5 => format!("Approved !{number}"),
            Self::Vote { .. } => format!("Declined !{number}"),
            Self::Complete => format!("Completed !{number}"),
            Self::Reopen => format!("Reopened !{number}"),
            Self::Abandon => format!("Abandoned !{number}"),
            Self::SetAutoComplete { .. } => format!("Auto-complete set on !{number}"),
            Self::CancelAutoComplete => format!("Auto-complete cancelled on !{number}"),
        }
    }
}

fn short_ref(reference: &str) -> String {
    reference
        .strip_prefix("refs/heads/")
        .unwrap_or(reference)
        .to_string()
}

/// Resolves a pull request into every call the detail makes: the record with
/// its reviewers and the web link, the policy evaluations, the comment
/// threads, and the signed-in user for the vote.
pub async fn pull_detail(
    folder: &Path,
    pat: &str,
    custom_base: &str,
    number: u64,
) -> Option<AzurePullDetail> {
    let remote = remote_for(folder, custom_base)?;
    let client = Client::new(pat);
    let base = format!(
        "{}/{}/_apis/git/repositories/{}/pullrequests/{number}",
        remote.base_url, remote.project, remote.repository
    );
    let record_url = format!("{base}?api-version=7.1");
    let policies_url = format!("{base}/evaluations?api-version=7.1");
    let threads_url = format!("{base}/threads?api-version=7.1");
    let me_url = format!("{}/_apis/connectionData", remote.base_url);
    let (record, policies, threads, me) = tokio::join!(
        client.get_json::<serde_json::Value>(&record_url),
        client.get_json::<serde_json::Value>(&policies_url),
        client.get_json::<serde_json::Value>(&threads_url),
        client.get_json::<serde_json::Value>(&me_url),
    );
    let mut pull = parse_pull(record.as_ref())?;
    pull.me = me
        .as_ref()
        .and_then(|me| me.pointer("/authenticatedUser/id"))
        .and_then(|id| id.as_str())
        .map(str::to_string);
    Some(AzurePullDetail {
        pull,
        policies: policies.as_ref().map(parse_policies).unwrap_or_default(),
        threads: threads.as_ref().map(parse_threads).unwrap_or_default(),
    })
}

/// The changed files: the newest iteration's changes, which is the whole
/// branch's delta against the target.
pub async fn pull_files(
    folder: &Path,
    pat: &str,
    custom_base: &str,
    number: u64,
) -> Option<Vec<AzureFile>> {
    let remote = remote_for(folder, custom_base)?;
    let client = Client::new(pat);
    let base = format!(
        "{}/{}/_apis/git/repositories/{}/pullrequests/{number}",
        remote.base_url, remote.project, remote.repository
    );
    let iterations_url = format!("{base}/iterations?api-version=7.1");
    let iterations = client
        .get_json::<serde_json::Value>(&iterations_url)
        .await?;
    let iteration = iterations
        .get("value")?
        .as_array()?
        .iter()
        .filter_map(|entry| entry.get("id").and_then(|id| id.as_u64()))
        .max()?;
    let changes_url = format!("{base}/iterations/{iteration}/changes?api-version=7.1");
    let changes = client.get_json::<serde_json::Value>(&changes_url).await?;
    Some(parse_files(&changes))
}

/// One commit's list: the pull request's commits, newest first as Azure keeps
/// them.
pub async fn pull_commits(
    folder: &Path,
    pat: &str,
    custom_base: &str,
    number: u64,
) -> Option<Vec<AzureCommit>> {
    let remote = remote_for(folder, custom_base)?;
    let client = Client::new(pat);
    let url = format!(
        "{}/{}/_apis/git/repositories/{}/pullrequests/{number}/commits?api-version=7.1",
        remote.base_url, remote.project, remote.repository
    );
    let payload = client.get_json::<serde_json::Value>(&url).await?;
    Some(parse_commits(&payload))
}

/// One file's diff, built from the two versions Azure serves. Azure hands out
/// no patch text with the change list, so this is the second call per file.
pub async fn pull_file_diff(
    folder: &Path,
    pat: &str,
    custom_base: &str,
    number: u64,
    path: &str,
) -> Option<String> {
    let detail = pull_refs(folder, pat, custom_base, number).await?;
    let remote = remote_for(folder, custom_base)?;
    let client = Client::new(pat);
    let items = |version: &str| {
        format!(
            "{}/{}/_apis/git/repositories/{}/items?path={}&includeContent=true&api-version=7.1&versionDescriptor.versionType=branch&versionDescriptor.version={}",
            remote.base_url,
            remote.project,
            remote.repository,
            escape_query(path),
            escape_query(version)
        )
    };
    let before = client
        .get_json::<serde_json::Value>(&items(&detail.0))
        .await;
    let after = client
        .get_json::<serde_json::Value>(&items(&detail.1))
        .await;
    let text = |payload: Option<serde_json::Value>| {
        payload
            .as_ref()
            .and_then(|value| value.get("content"))
            .and_then(|content| content.as_str())
            .map(str::to_string)
    };
    let before = text(before);
    let after = text(after);
    if before.is_none() && after.is_none() {
        return None;
    }
    Some(unified_diff(
        path,
        before.as_deref().unwrap_or_default(),
        after.as_deref().unwrap_or_default(),
    ))
}

/// The pull request's two branch names, for the item lookups.
async fn pull_refs(
    folder: &Path,
    pat: &str,
    custom_base: &str,
    number: u64,
) -> Option<(String, String)> {
    let remote = remote_for(folder, custom_base)?;
    let client = Client::new(pat);
    let url = format!(
        "{}/{}/_apis/git/repositories/{}/pullrequests/{number}?api-version=7.1",
        remote.base_url, remote.project, remote.repository
    );
    let record = client.get_json::<serde_json::Value>(&url).await?;
    let pull = parse_pull(Some(&record))?;
    Some((pull.source_branch(), pull.target_branch()))
}

/// One verb on the pull request. `remote` and `client` are the resolved
/// coordinates, so a test can check the request shape without a network.
pub async fn pull_action(
    folder: &Path,
    pat: &str,
    custom_base: &str,
    number: u64,
    action: AzurePullAction,
    pull: &AzurePull,
) -> Result<(), String> {
    let remote = remote_for(folder, custom_base).ok_or("not an Azure DevOps repository")?;
    let client = Client::new(pat);
    let base = format!(
        "{}/{}/_apis/git/repositories/{}/pullrequests/{number}",
        remote.base_url, remote.project, remote.repository
    );
    let (method, url, body) = action_request(&base, action, pull)?;
    client.request(method, &url, body).await.map(|_| ())
}

/// The request a verb becomes: method, URL and JSON body. Split out so the
/// rules (which vote an approval is, what a completion has to name) are
/// testable without a server.
fn action_request(
    base: &str,
    action: AzurePullAction,
    pull: &AzurePull,
) -> Result<(&'static str, String, Option<serde_json::Value>), String> {
    // the options a completion and an auto-complete share; the merge itself
    // keeps the source branch and makes a merge commit
    let options = || {
        serde_json::json!({
            "deleteSourceBranch": false,
            "squashMerge": false,
            "mergeCommitMessage": "",
            "bypassPolicy": false,
        })
    };
    Ok(match action {
        AzurePullAction::Vote { reviewer, vote } => (
            "PUT",
            format!(
                "{base}/reviewers/{}?api-version=7.1",
                escape_query(&reviewer)
            ),
            Some(serde_json::json!({ "vote": vote })),
        ),
        AzurePullAction::Complete => {
            let source = pull
                .last_merge_source
                .clone()
                .ok_or("Azure did not name the source commit")?;
            (
                "PATCH",
                format!("{base}?api-version=7.1"),
                Some(serde_json::json!({
                    "status": "completed",
                    "lastMergeSourceCommit": { "commitId": source },
                    "completionOptions": options(),
                })),
            )
        }
        AzurePullAction::Reopen => (
            "PATCH",
            format!("{base}?api-version=7.1"),
            Some(serde_json::json!({ "status": "active" })),
        ),
        AzurePullAction::Abandon => (
            "PATCH",
            format!("{base}?api-version=7.1"),
            Some(serde_json::json!({ "status": "abandoned" })),
        ),
        AzurePullAction::SetAutoComplete { reviewer } => (
            "PUT",
            format!("{base}/autocomplete?api-version=7.1"),
            Some(serde_json::json!({
                "autoCompleteSetBy": { "id": reviewer },
                "completionOptions": options(),
            })),
        ),
        AzurePullAction::CancelAutoComplete => (
            "DELETE",
            format!("{base}/autocomplete?api-version=7.1"),
            None,
        ),
    })
}

/// One pull request's record, defensively parsed.
pub fn parse_pull(payload: Option<&serde_json::Value>) -> Option<AzurePull> {
    let p = payload?;
    let text = |key: &str| p.get(key).and_then(|v| v.as_str()).map(str::to_string);
    let reviewers = p
        .get("reviewers")
        .and_then(|v| v.as_array())
        .map(|rows| {
            rows.iter()
                .filter_map(|r| {
                    Some(AzureReviewer {
                        id: r.get("id").and_then(|v| v.as_str())?.to_string(),
                        name: r
                            .get("displayName")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string(),
                        unique_name: r
                            .get("uniqueName")
                            .and_then(|v| v.as_str())
                            .map(str::to_string),
                        vote: r.get("vote").and_then(|v| v.as_i64()).unwrap_or(0),
                        required: r
                            .get("isRequired")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Some(AzurePull {
        number: p.get("pullRequestId").and_then(|v| v.as_u64())?,
        title: text("title").unwrap_or_default(),
        status: text("status").unwrap_or_default(),
        is_draft: p.get("isDraft").and_then(|v| v.as_bool()).unwrap_or(false),
        source: text("sourceRefName").unwrap_or_default(),
        target: text("targetRefName").unwrap_or_default(),
        author: p
            .pointer("/createdBy/displayName")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        created: text("creationDate"),
        closed: text("closedDate"),
        url: p
            .pointer("/_links/web/href")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        description: text("description").filter(|text| !text.trim().is_empty()),
        merge_status: text("mergeStatus"),
        reviewers,
        auto_complete: p.get("autoCompleteSetBy").is_some_and(|v| !v.is_null()),
        last_merge_source: p
            .pointer("/lastMergeSourceCommit/commitId")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        me: None,
    })
}

/// The policy checklist.
pub fn parse_policies(payload: &serde_json::Value) -> Vec<AzurePolicy> {
    payload
        .get("value")
        .and_then(|v| v.as_array())
        .map(|rows| {
            rows.iter()
                .filter_map(|row| {
                    let kind = row
                        .pointer("/configuration/type/displayName")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string();
                    let name = row
                        .pointer("/configuration/settings/displayName")
                        .and_then(|v| v.as_str())
                        .map(str::to_string)
                        .unwrap_or_else(|| kind.clone());
                    if name.trim().is_empty() && kind.trim().is_empty() {
                        return None;
                    }
                    Some(AzurePolicy {
                        name,
                        kind,
                        status: row
                            .get("status")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The comment threads, each with its resolution state.
pub fn parse_threads(payload: &serde_json::Value) -> Vec<AzureThread> {
    payload
        .get("value")
        .and_then(|v| v.as_array())
        .map(|rows| {
            rows.iter()
                .filter_map(|row| {
                    let comments: Vec<AzureComment> = row
                        .get("comments")
                        .and_then(|v| v.as_array())
                        .map(|rows| {
                            rows.iter()
                                .map(|c| AzureComment {
                                    author: c
                                        .pointer("/author/displayName")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or_default()
                                        .to_string(),
                                    when: c
                                        .get("publishedDate")
                                        .and_then(|v| v.as_str())
                                        .map(str::to_string),
                                    text: c
                                        .get("content")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or_default()
                                        .to_string(),
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    (!comments.is_empty()).then(|| AzureThread {
                        status: row
                            .get("status")
                            .and_then(|v| v.as_str())
                            .unwrap_or("active")
                            .to_string(),
                        comments,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn parse_commits(payload: &serde_json::Value) -> Vec<AzureCommit> {
    payload
        .get("value")
        .and_then(|v| v.as_array())
        .map(|rows| {
            rows.iter()
                .filter_map(|row| {
                    Some(AzureCommit {
                        id: row.get("commitId").and_then(|v| v.as_str())?.to_string(),
                        message: row
                            .pointer("/comment")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string(),
                        author: row
                            .pointer("/author/name")
                            .and_then(|v| v.as_str())
                            .map(str::to_string),
                        when: row
                            .pointer("/author/date")
                            .and_then(|v| v.as_str())
                            .map(str::to_string),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The iteration changes' file rows.
pub fn parse_files(payload: &serde_json::Value) -> Vec<AzureFile> {
    payload
        .get("changeEntries")
        .and_then(|v| v.as_array())
        .map(|rows| {
            rows.iter()
                .filter_map(|row| {
                    let path =
                        row.pointer("/item/path")
                            .and_then(|v| v.as_str())
                            .or_else(|| {
                                row.pointer("/originalPaths/0/path")
                                    .and_then(|v| v.as_str())
                            })?;
                    Some(AzureFile {
                        path: path.trim_start_matches('/').to_string(),
                        change: row
                            .get("changeType")
                            .and_then(|v| v.as_str())
                            .unwrap_or("edit")
                            .to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// A path or a query value, encoded for a URL's path segment.
fn escape_query(value: &str) -> String {
    escape(value).replace('#', "%23").replace('?', "%3F")
}

/// A whole-file unified diff of two texts, for the files Azure does not hand
/// a patch for. Hunks with three lines of context, so the kit's viewer draws
/// it like any other patch. Identical texts are an empty diff.
pub fn unified_diff(path: &str, before: &str, after: &str) -> String {
    if before == after {
        return String::new();
    }
    let a: Vec<&str> = before.lines().collect();
    let b: Vec<&str> = after.lines().collect();
    let ops = diff_ops(&a, &b);
    let body = hunks(&ops, &a, &b);
    if body.is_empty() {
        return String::new();
    }
    format!("--- a/{path}\n+++ b/{path}\n{body}")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DiffOp {
    Same,
    Del,
    Add,
}

/// The line diff, as one op per source or target line. A pathological pair
/// (over a thousand lines a side) falls back to "every line changed", which is
/// honest and keeps the table small.
fn diff_ops(a: &[&str], b: &[&str]) -> Vec<DiffOp> {
    const CAP: usize = 1200;
    if a.len() > CAP || b.len() > CAP {
        let mut ops = vec![DiffOp::Del; a.len()];
        ops.extend(std::iter::repeat_n(DiffOp::Add, b.len()));
        return ops;
    }
    let (n, m) = (a.len(), b.len());
    let mut table = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            table[i][j] = if a[i] == b[j] {
                table[i + 1][j + 1] + 1
            } else {
                table[i + 1][j].max(table[i][j + 1])
            };
        }
    }
    let mut ops = Vec::with_capacity(n + m);
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if a[i] == b[j] {
            ops.push(DiffOp::Same);
            i += 1;
            j += 1;
        } else if table[i + 1][j] >= table[i][j + 1] {
            ops.push(DiffOp::Del);
            i += 1;
        } else {
            ops.push(DiffOp::Add);
            j += 1;
        }
    }
    ops.extend(std::iter::repeat_n(DiffOp::Del, n - i));
    ops.extend(std::iter::repeat_n(DiffOp::Add, m - j));
    ops
}

/// The hunks: each change with three lines around it, changes closer than
/// twice that gathered into one hunk.
fn hunks(ops: &[DiffOp], a: &[&str], b: &[&str]) -> String {
    const CONTEXT: usize = 3;
    // the (source, target) line index each op starts at
    let mut positions = Vec::with_capacity(ops.len());
    let (mut ai, mut bi) = (0usize, 0usize);
    for op in ops {
        positions.push((ai, bi));
        match op {
            DiffOp::Same => {
                ai += 1;
                bi += 1;
            }
            DiffOp::Del => ai += 1,
            DiffOp::Add => bi += 1,
        }
    }
    let changed: Vec<usize> = ops
        .iter()
        .enumerate()
        .filter(|(_, op)| **op != DiffOp::Same)
        .map(|(ix, _)| ix)
        .collect();
    if changed.is_empty() {
        return String::new();
    }
    let mut text = String::new();
    let mut k = 0;
    while k < changed.len() {
        let first = changed[k];
        let mut last = first;
        while k + 1 < changed.len() && changed[k + 1] <= last + 2 * CONTEXT {
            k += 1;
            last = changed[k];
        }
        let start = first.saturating_sub(CONTEXT);
        let end = (last + CONTEXT).min(ops.len().saturating_sub(1));
        let (a_start, b_start) = positions[start];
        let mut a_len = 0;
        let mut b_len = 0;
        for op in &ops[start..=end] {
            match op {
                DiffOp::Same => {
                    a_len += 1;
                    b_len += 1;
                }
                DiffOp::Del => a_len += 1,
                DiffOp::Add => b_len += 1,
            }
        }
        if b_len == 0 {
            text.push_str(&format!(
                "@@ -{},{} +{},0 @@\n",
                a_start + 1,
                a_len,
                b_start
            ));
        } else if a_len == 0 {
            text.push_str(&format!(
                "@@ -{},0 +{},{} @@\n",
                a_start,
                b_start + 1,
                b_len
            ));
        } else {
            text.push_str(&format!(
                "@@ -{},{} +{},{} @@\n",
                a_start + 1,
                a_len,
                b_start + 1,
                b_len
            ));
        }
        let mut line_a = a_start;
        let mut line_b = b_start;
        for op in &ops[start..=end] {
            match op {
                DiffOp::Same => {
                    text.push(' ');
                    text.push_str(a[line_a]);
                    line_a += 1;
                    line_b += 1;
                }
                DiffOp::Del => {
                    text.push('-');
                    text.push_str(a[line_a]);
                    line_a += 1;
                }
                DiffOp::Add => {
                    text.push('+');
                    text.push_str(b[line_b]);
                    line_b += 1;
                }
            }
            text.push('\n');
        }
        k += 1;
    }
    text
}

// ---- one run's detail ----------------------------------------------------------

/// The log tail the detail shows: the end of the log is where a failure is.
const RUN_LOG_CAP: usize = 8000;

/// A run's stages and the tail of its log, in the two calls Azure wants for
/// it: the timeline for the stage/job records, then the last log entry's text.
pub async fn run_detail(
    folder: &Path,
    pat: &str,
    custom_base: &str,
    build_id: u64,
) -> Option<RunDetail> {
    let remote = remote_for(folder, custom_base)?;
    let client = Client::new(pat);
    let base = format!(
        "{}/{}/_apis/build/builds/{build_id}",
        remote.base_url, remote.project
    );
    let timeline_url = format!("{base}/timeline?api-version=7.1");
    let logs_url = format!("{base}/logs?api-version=7.1");
    let (timeline, logs) = tokio::join!(
        client.get_json::<serde_json::Value>(&timeline_url),
        client.get_json::<serde_json::Value>(&logs_url),
    );
    let stages = timeline.as_ref().map(parse_timeline).unwrap_or_default();
    // the last entry is the run's closing words, which is the excerpt worth
    // showing; a log that failed to list leaves the card empty
    let log = match last_log_url(logs.as_ref()) {
        Some(url) => client
            .get_text(&url)
            .await
            .map(|text| tail(&text, RUN_LOG_CAP))
            .unwrap_or_default(),
        None => String::new(),
    };
    Some(RunDetail { stages, log })
}

/// One record of a build's timeline, before it is grouped.
struct TimelineRecord {
    id: String,
    parent: Option<String>,
    kind: String,
    name: String,
    state: Option<String>,
    result: Option<String>,
    start: Option<String>,
    finish: Option<String>,
    order: u64,
}

/// The stage a record belongs to: its own id for a stage, else the nearest
/// stage up the parent chain (multi-config builds put a phase in between).
fn stage_id_of<'a>(
    record: &'a TimelineRecord,
    by_id: &std::collections::HashMap<&str, &'a TimelineRecord>,
) -> Option<&'a str> {
    let mut current = record;
    for _ in 0..8 {
        if current.kind.eq_ignore_ascii_case("stage") {
            return Some(current.id.as_str());
        }
        let parent = current.parent.as_deref()?;
        current = by_id.get(parent)?;
    }
    None
}

/// Groups the timeline's flat records into stages and their jobs. A job sits
/// under a stage directly, or under a phase (multi-config builds add one), so
/// the parent chain is walked up to the stage.
pub fn parse_timeline(json: &serde_json::Value) -> Vec<RunStage> {
    let records: Vec<TimelineRecord> = json
        .get("value")
        .and_then(|v| v.as_array())
        .map(|rows| {
            rows.iter()
                .filter_map(|r| {
                    let text = |key: &str| r.get(key).and_then(|v| v.as_str()).map(str::to_string);
                    Some(TimelineRecord {
                        id: r.get("id").and_then(|v| v.as_str())?.to_string(),
                        parent: text("parentId"),
                        kind: text("type").unwrap_or_default(),
                        name: text("name").unwrap_or_default(),
                        state: text("state"),
                        result: text("result"),
                        start: text("startTime"),
                        finish: text("finishTime"),
                        order: r.get("order").and_then(|v| v.as_u64()).unwrap_or(0),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let by_id: std::collections::HashMap<&str, &TimelineRecord> =
        records.iter().map(|r| (r.id.as_str(), r)).collect();

    let mut stages: Vec<(&TimelineRecord, Vec<&TimelineRecord>)> = records
        .iter()
        .filter(|r| r.kind.eq_ignore_ascii_case("stage"))
        .map(|stage| (stage, Vec::new()))
        .collect();
    stages.sort_by_key(|(stage, _)| stage.order);
    for record in &records {
        let is_job =
            record.kind.eq_ignore_ascii_case("job") || record.kind.eq_ignore_ascii_case("phase");
        if !is_job {
            continue;
        }
        let Some(stage_id) = stage_id_of(record, &by_id) else {
            continue;
        };
        if let Some((_, jobs)) = stages.iter_mut().find(|(stage, _)| stage.id == stage_id) {
            jobs.push(record);
        }
    }
    stages
        .into_iter()
        .map(|(stage, mut jobs)| {
            jobs.sort_by_key(|job| job.order);
            RunStage {
                name: stage.name.clone(),
                jobs: jobs
                    .into_iter()
                    .map(|job| RunJob {
                        name: job.name.clone(),
                        state: job.state.clone(),
                        result: job.result.clone(),
                        start: job.start.clone(),
                        finish: job.finish.clone(),
                    })
                    .collect(),
            }
        })
        .collect()
}

/// The `url` of the log list's last entry.
fn last_log_url(logs: Option<&serde_json::Value>) -> Option<String> {
    logs?
        .get("value")?
        .as_array()?
        .last()?
        .get("url")?
        .as_str()
        .map(str::to_string)
}

/// Wall time between two Azure timestamps, in the app's words: `12s`, `4m 12s`,
/// `1h 02m`. A run that has not finished counts up to now, so a running job's
/// row still says something. Azure's "not started yet" sentinel (year 1) and a
/// future start answer None.
pub fn duration_between(start: Option<&str>, finish: Option<&str>) -> Option<String> {
    let parse = |text: Option<&str>| {
        let text = text?;
        let at = chrono::DateTime::parse_from_rfc3339(text).ok()?;
        (chrono::Datelike::year(&at) > 2000).then_some(at.with_timezone(&chrono::Local))
    };
    let start = parse(start)?;
    let end = parse(finish).unwrap_or_else(chrono::Local::now);
    let seconds = (end - start).num_seconds().max(0);
    Some(match seconds {
        0..=59 => format!("{seconds}s"),
        60..=3599 => format!("{}m {:02}s", seconds / 60, seconds % 60),
        _ => format!("{}h {:02}m", seconds / 3600, (seconds % 3600) / 60),
    })
}

/// The end of a long text, cut on a line boundary so the excerpt never starts
/// mid-line.
fn tail(text: &str, cap: usize) -> String {
    if text.len() <= cap {
        return text.to_string();
    }
    let start = text.len() - cap;
    match text[start..].find('\n') {
        Some(offset) => text[start + offset + 1..].to_string(),
        None => text[start..].to_string(),
    }
}

// ---- the HTTP shape ------------------------------------------------------------

struct Client {
    http: reqwest::Client,
    auth_header: String,
}

impl Client {
    fn new(pat: &str) -> Self {
        Self {
            http: reqwest::Client::new(),
            // Basic base64(":" + PAT) — the empty-user form
            auth_header: format!("Basic {}", base64(format!(":{pat}").as_bytes())),
        }
    }

    async fn get_json<T: for<'de> Deserialize<'de>>(&self, url: &str) -> Option<T> {
        let response = self.send(reqwest::Method::GET, url, None).await?;
        if response.status().as_u16() == 401 {
            tracing::warn!("azure devops: 401 — the PAT was rejected");
            return None;
        }
        if !response.status().is_success() {
            tracing::debug!("azure devops GET {url} -> {}", response.status());
            return None;
        }
        response.json().await.ok()
    }

    async fn post_json<T: for<'de> Deserialize<'de>, B: serde::Serialize>(
        &self,
        url: &str,
        body: &B,
    ) -> Option<T> {
        let response = self
            .send(
                reqwest::Method::POST,
                url,
                Some(serde_json::to_vec(body).ok()?),
            )
            .await?;
        if response.status().as_u16() == 401 || !response.status().is_success() {
            tracing::debug!("azure devops POST {url} -> {}", response.status());
            return None;
        }
        response.json().await.ok()
    }

    /// One JSON request, with Azure's own words on a failure: a verb has to
    /// tell the user what the server refused, not just that it refused.
    async fn request(
        &self,
        method: &str,
        url: &str,
        body: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, String> {
        let method = reqwest::Method::from_bytes(method.as_bytes())
            .map_err(|_| format!("{method} is not a method"))?;
        let payload = body
            .map(|value| serde_json::to_vec(&value).map_err(|err| err.to_string()))
            .transpose()?;
        let response = self
            .send(method, url, payload)
            .await
            .ok_or_else(|| "Azure DevOps did not answer".to_string())?;
        let status = response.status();
        if status.as_u16() == 401 {
            return Err("Azure DevOps rejected the personal access token".to_string());
        }
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            // Azure wraps its message in a JSON envelope; the raw line reads
            // fine when it does not
            let message = serde_json::from_str::<serde_json::Value>(&body)
                .ok()
                .and_then(|value| {
                    value
                        .get("message")
                        .and_then(|m| m.as_str())
                        .map(str::to_string)
                })
                .unwrap_or_else(|| body.lines().next().unwrap_or_default().to_string());
            return Err(if message.trim().is_empty() {
                format!("Azure DevOps answered {status}")
            } else {
                message
            });
        }
        if status.as_u16() == 204 {
            return Ok(serde_json::Value::Null);
        }
        Ok(response
            .json::<serde_json::Value>()
            .await
            .unwrap_or(serde_json::Value::Null))
    }

    async fn get_text(&self, url: &str) -> Option<String> {
        let response = self.send(reqwest::Method::GET, url, None).await?;
        if response.status().as_u16() == 401 || !response.status().is_success() {
            tracing::debug!("azure devops GET {url} -> {}", response.status());
            return None;
        }
        response.text().await.ok()
    }

    async fn send(
        &self,
        method: reqwest::Method,
        url: &str,
        body: Option<Vec<u8>>,
    ) -> Option<reqwest::Response> {
        let mut request = self
            .http
            .request(method, url)
            .header("Authorization", &self.auth_header);
        if let Some(body) = body {
            request = request
                .header("Content-Type", "application/json")
                .body(body);
        }
        request.send().await.ok()
    }
}

/// The base64 the Basic auth header needs (standard alphabet, padded).
fn base64(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(TABLE[(n >> 18) as usize & 63] as char);
        out.push(TABLE[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            TABLE[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_decode_decodes_utf8_sequences() {
        // a `byte as char` regression: "%C3%A9" decoded as two Latin-1 chars
        assert_eq!(percent_decode("caf%C3%A9"), "café");
        assert_eq!(percent_decode("my%20org"), "my org");
        assert_eq!(percent_decode("plain"), "plain");
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%ZZ"), "%ZZ");
    }

    #[test]
    fn parses_public_https_remote() {
        let remote = parse_url("https://dev.azure.com/my-org/proj/_git/repo.git", None).unwrap();
        assert_eq!(remote.base_url, "https://dev.azure.com/my-org");
        assert_eq!(remote.project, "proj");
        assert_eq!(remote.repository, "repo");
    }

    #[test]
    fn parses_legacy_and_ssh_forms() {
        let legacy = parse_url("https://myorg.visualstudio.com/proj/_git/repo", None).unwrap();
        assert_eq!(legacy.base_url, "https://dev.azure.com/myorg");
        assert_eq!(legacy.repository, "repo");

        let ssh = parse_url("git@ssh.dev.azure.com:v3/my-org/proj/repo", None).unwrap();
        assert_eq!(ssh.project, "proj");
        assert_eq!(ssh.repository, "repo");
    }

    #[test]
    fn parses_custom_server_remote() {
        let custom = "https://devops.company.com/tfs";
        let url = "https://devops.company.com/tfs/DefaultCollection/Proj/_git/repo";
        let remote = parse_url(url, Some(custom)).unwrap();
        assert_eq!(
            remote.base_url,
            "https://devops.company.com/tfs/DefaultCollection"
        );
        assert_eq!(remote.project, "Proj");
        assert_eq!(remote.repository, "repo");
        // a different host does not match
        assert!(parse_url("https://other.company.com/a/b/_git/c", Some(custom)).is_none());
    }

    #[test]
    fn non_azure_urls_rejected() {
        assert!(parse_url("https://github.com/owner/repo.git", None).is_none());
        assert!(parse_url("git@github.com:owner/repo.git", None).is_none());
    }

    #[test]
    fn base64_matches_known_vectors() {
        assert_eq!(base64(b":"), "Og==");
        assert_eq!(base64(b":pat123"), "OnBhdDEyMw==");
        assert_eq!(base64(b"abc"), "YWJj");
    }

    /// A build record carries the commit, the trigger and both timestamps; the
    /// row's words come from them.
    #[test]
    fn a_build_reads_its_commit_trigger_and_times() {
        let build = serde_json::json!({
            "id": 7,
            "buildNumber": "20261102.3",
            "definition": {"name": "Build and test"},
            "sourceBranch": "refs/heads/main",
            "sourceVersion": "abcdef1234567890",
            "reason": "individualCI",
            "result": null,
            "status": "inProgress",
            "startTime": "2026-11-02T09:00:00Z",
            "finishTime": null,
            "_links": {"web": {"href": "https://dev.azure.com/o/p/_build/results?buildId=7"}}
        });

        let run = parse_build(&build).expect("a run");

        assert_eq!(run.id, 7);
        assert_eq!(run.definition_name.as_deref(), Some("Build and test"));
        assert_eq!(run.display_branch().as_deref(), Some("main"));
        assert_eq!(run.short_source().as_deref(), Some("abcdef1"));
        assert_eq!(run.trigger_word(), Some("Continuous integration"));
        assert_eq!(run.state_word(), "Running");
        assert_eq!(run.start.as_deref(), Some("2026-11-02T09:00:00Z"));
        assert!(run.finish.is_none());
        assert!(build.get("id").is_some());
        assert!(parse_build(&serde_json::json!({"buildNumber": "x"})).is_none());
    }

    const TIMELINE: &str = r#"{"records": [], "value": [
        {"id": "s1", "parentId": null, "type": "Stage", "name": "Build",
         "state": "completed", "result": "succeeded", "order": 1},
        {"id": "s2", "parentId": null, "type": "Stage", "name": "Test",
         "state": "completed", "result": "failed", "order": 2},
        {"id": "j1", "parentId": "s1", "type": "Job", "name": "Compile",
         "state": "completed", "result": "succeeded", "order": 1,
         "startTime": "2026-11-02T09:00:00Z", "finishTime": "2026-11-02T09:04:00Z"},
        {"id": "p1", "parentId": "s2", "type": "Phase", "name": "Linux pytest",
         "state": "completed", "result": "failed", "order": 1},
        {"id": "j2", "parentId": "p1", "type": "Job", "name": "pytest (3.12)",
         "state": "completed", "result": "failed", "order": 1},
        {"id": "t1", "parentId": "j2", "type": "Task", "name": "Run tests",
         "state": "completed", "result": "failed", "order": 1},
        {"id": "j3", "parentId": "s2", "type": "Job", "name": "Lint",
         "state": "inProgress", "result": null, "order": 2}
    ]}"#;

    /// Stages in order, each with the jobs under it — one phase deep, which is
    /// where multi-config builds put them.
    #[test]
    fn the_timeline_groups_its_jobs_under_their_stage() {
        let stages = parse_timeline(&serde_json::from_str(TIMELINE).unwrap());

        assert_eq!(stages.len(), 2);
        assert_eq!(stages[0].name, "Build");
        assert_eq!(stages[0].jobs.len(), 1);
        assert_eq!(stages[0].jobs[0].name, "Compile");
        assert_eq!(stages[1].name, "Test");
        let names: Vec<&str> = stages[1].jobs.iter().map(|j| j.name.as_str()).collect();
        assert_eq!(names, ["Linux pytest", "pytest (3.12)", "Lint"]);
        assert_eq!(stages[0].done(), 1, "Compile succeeded");
        assert_eq!(stages[1].failed(), 2);
        assert_eq!(stages[1].done(), 0, "its only clean job is still running");
        assert!(stages[1].jobs[2].is_running());
        // a task is not a job row
        assert!(!names.contains(&"Run tests"));
    }

    #[test]
    fn a_timeline_without_stages_is_empty_not_a_panic() {
        assert!(parse_timeline(&serde_json::json!({})).is_empty());
        assert!(parse_timeline(&serde_json::json!({"value": []})).is_empty());
        assert!(
            parse_timeline(&serde_json::json!({"value": [
                {"id": "j1", "type": "Job", "name": "Orphan"}
            ]}))
            .is_empty()
        );
    }

    #[test]
    fn the_log_excerpt_is_the_tail_cut_on_a_line() {
        let text = "line one\nline two\nline three\n";
        assert_eq!(tail(text, 4096), text);

        let long = "a".repeat(100) + "\nlast line\n";
        let cut = tail(&long, 20);
        assert!(cut.ends_with("last line\n"), "cut was {cut:?}");
        assert!(!cut.contains("aaaa"), "the tail keeps the end only");
    }

    const PULL: &str = r#"{
        "pullRequestId": 214,
        "title": "Sync the bridge",
        "status": "active",
        "isDraft": false,
        "sourceRefName": "refs/heads/fix/bridge",
        "targetRefName": "refs/heads/main",
        "createdBy": {"displayName": "Engin K"},
        "creationDate": "2026-10-01T10:00:00Z",
        "closedDate": null,
        "mergeStatus": "succeeded",
        "autoCompleteSetBy": null,
        "lastMergeSourceCommit": {"commitId": "abc123"},
        "_links": {"web": {"href": "https://dev.azure.com/o/p/_git/r/pullrequest/214"}},
        "reviewers": [
            {"id": "me-1", "displayName": "Engin K", "vote": 10, "isRequired": true},
            {"id": "r-2", "displayName": "Mert", "vote": -10, "isRequired": false}
        ]
    }"#;

    #[test]
    fn an_azure_pull_reads_its_record_and_reviewers() {
        let payload: serde_json::Value = serde_json::from_str(PULL).unwrap();
        let mut pull = parse_pull(Some(&payload)).expect("a pull request");

        assert_eq!(pull.number, 214);
        assert_eq!(pull.state_word(), "Active");
        assert_eq!(pull.source_branch(), "fix/bridge");
        assert_eq!(pull.target_branch(), "main");
        assert_eq!(pull.approved(), 1);
        assert_eq!(pull.rejected(), 1);
        assert_eq!(pull.reviewers[0].vote_word(), "Approved");
        assert_eq!(pull.reviewers[1].vote_word(), "Rejected");
        assert!(!pull.auto_complete);
        assert_eq!(pull.last_merge_source.as_deref(), Some("abc123"));
        assert_eq!(
            pull.url.as_deref(),
            Some("https://dev.azure.com/o/p/_git/r/pullrequest/214")
        );

        pull.me = Some("me-1".into());
        assert_eq!(pull.my_reviewer().map(|r| r.name.as_str()), Some("Engin K"));
        assert!(pull.ready_to_complete(&[]), "active, clean and not a draft");
        assert_eq!(parse_pull(Some(&serde_json::json!({"title": "x"}))), None);
    }

    #[test]
    fn the_state_word_follows_the_status() {
        let record = |status: &str, draft: bool| {
            serde_json::json!({
                "pullRequestId": 1, "status": status, "isDraft": draft,
                "sourceRefName": "refs/heads/a", "targetRefName": "refs/heads/b"
            })
        };
        let word = |status: &str, draft: bool| {
            parse_pull(Some(&record(status, draft)))
                .unwrap()
                .state_word()
        };

        assert_eq!(word("active", false), "Active");
        assert_eq!(word("active", true), "Draft");
        assert_eq!(word("completed", false), "Completed");
        assert_eq!(word("abandoned", false), "Abandoned");
    }

    #[test]
    fn the_policy_checklist_reads_status_and_kind() {
        let payload = serde_json::json!({"value": [
            {"status": "approved", "configuration": {
                "type": {"displayName": "Required"},
                "settings": {"displayName": "Mert must approve"}}},
            {"status": "running", "configuration": {
                "type": {"displayName": "Build"}, "settings": {}}},
            {"status": "rejected", "configuration": {
                "type": {"displayName": "Merge strategy"}, "settings": {}}}
        ]});

        let policies = parse_policies(&payload);

        assert_eq!(policies.len(), 3);
        assert_eq!(policies[0].name, "Mert must approve");
        assert!(!policies[0].is_blocking());
        assert_eq!(policies[1].name, "Build", "no display name falls back");
        assert!(policies[1].is_running());
        assert!(policies[2].is_blocking());
        assert_eq!(policies[2].word(), "Rejected");
    }

    #[test]
    fn threads_keep_their_resolution_and_comments() {
        let payload = serde_json::json!({"value": [
            {"status": "active", "comments": [
                {"author": {"displayName": "Mert"}, "publishedDate": "2026-10-02T09:00:00Z",
                 "content": "This leaks a handle."}
            ]},
            {"status": "fixed", "comments": []}
        ]});

        let threads = parse_threads(&payload);

        assert_eq!(threads.len(), 1, "an empty thread is not a row");
        assert_eq!(threads[0].comments[0].author, "Mert");
        assert!(threads[0].is_open());
        assert_eq!(threads[0].comments[0].text, "This leaks a handle.");
    }

    #[test]
    fn files_and_commits_read_their_lists() {
        let changes = serde_json::json!({"changeEntries": [
            {"item": {"path": "/src/main.rs"}, "changeType": "edit"},
            {"item": {"path": "/docs/new.md"}, "changeType": "add"},
            {"item": {"path": "/old.rs"}, "changeType": "delete"},
            {"item": {"path": "/a.rs"}, "changeType": "rename, edit"}
        ]});
        let files = parse_files(&changes);
        assert_eq!(files.len(), 4);
        assert_eq!(files[0].path, "src/main.rs", "the leading slash goes");
        let letters: Vec<&str> = files.iter().map(|f| f.letter()).collect();
        assert_eq!(letters, ["M", "A", "D", "R"]);

        let commits = parse_commits(&serde_json::json!({"value": [
            {"commitId": "abcdef123456", "comment": "Fix the bridge",
             "author": {"name": "Engin K", "date": "2026-10-01T10:00:00Z"}}
        ]}));
        assert_eq!(commits[0].short_id(), "abcdef1");
        assert_eq!(commits[0].message, "Fix the bridge");
        assert_eq!(commits[0].when.as_deref(), Some("2026-10-01T10:00:00Z"));
    }

    fn pull_for_verbs() -> AzurePull {
        parse_pull(Some(&serde_json::from_str(PULL).unwrap())).unwrap()
    }

    /// Every verb is one REST call, and the body says what Azure needs: a vote,
    /// a status, or the commit a completion merges.
    #[test]
    fn every_azure_verb_maps_to_one_call() {
        let pull = pull_for_verbs();
        let base = "https://dev.azure.com/o/p/_apis/git/repositories/r/pullrequests/214";

        let (method, url, body) = action_request(
            base,
            AzurePullAction::Vote {
                reviewer: "me-1".into(),
                vote: 10,
            },
            &pull,
        )
        .unwrap();
        assert_eq!(method, "PUT");
        assert_eq!(
            url,
            "https://dev.azure.com/o/p/_apis/git/repositories/r/pullrequests/214/reviewers/me-1?api-version=7.1"
        );
        assert_eq!(body, Some(serde_json::json!({ "vote": 10 })));

        let (_, _, body) = action_request(base, AzurePullAction::Complete, &pull).unwrap();
        let body = body.unwrap();
        assert_eq!(body["status"], "completed");
        assert_eq!(
            body["lastMergeSourceCommit"]["commitId"], "abc123",
            "the completion names the source commit"
        );
        assert_eq!(body["completionOptions"]["deleteSourceBranch"], false);

        let (_, _, body) = action_request(base, AzurePullAction::Abandon, &pull).unwrap();
        assert_eq!(body.unwrap()["status"], "abandoned");

        let (method, url, body) = action_request(
            base,
            AzurePullAction::SetAutoComplete {
                reviewer: "me-1".into(),
            },
            &pull,
        )
        .unwrap();
        assert_eq!(method, "PUT");
        assert!(url.ends_with("/autocomplete?api-version=7.1"));
        let body = body.unwrap();
        assert_eq!(body["autoCompleteSetBy"]["id"], "me-1");
        assert!(body.get("completionOptions").is_some());

        let (method, _, body) =
            action_request(base, AzurePullAction::CancelAutoComplete, &pull).unwrap();
        assert_eq!(method, "DELETE");
        assert!(body.is_none());

        // a completion without the source commit is refused before the call
        let mut bare = pull.clone();
        bare.last_merge_source = None;
        assert!(action_request(base, AzurePullAction::Complete, &bare).is_err());
        assert_eq!(AzurePullAction::Abandon.method(), "PATCH");
        assert_eq!(AzurePullAction::Abandon.done_word(214), "Abandoned !214");
    }

    #[test]
    fn a_whole_file_diff_keeps_the_lines_that_hold() {
        let diff = unified_diff("src/a.rs", "one\ntwo\nthree\n", "one\nTWO\nthree\n");

        assert!(diff.starts_with("--- a/src/a.rs\n+++ b/src/a.rs\n"));
        assert!(diff.contains("-two\n+TWO\n"), "the changed line: {diff}");
        assert!(diff.contains(" one\n"), "context rides along: {diff}");

        // identical texts are no diff at all
        assert_eq!(unified_diff("a", "same\n", "same\n"), "");
        // an added file is all additions, a deleted one all removals
        let added = unified_diff("a", "", "x\ny\n");
        assert!(added.contains("+++ b/a"), "{added}");
        assert!(added.contains("+x\n"), "{added}");
        let removed = unified_diff("a", "x\ny\n", "");
        assert!(removed.contains("-x\n"), "{removed}");
    }

    #[test]
    fn a_duration_reads_in_the_short_units() {
        assert_eq!(
            duration_between(Some("2026-11-02T09:00:00Z"), Some("2026-11-02T09:00:12Z")),
            Some("12s".into())
        );
        assert_eq!(
            duration_between(Some("2026-11-02T09:00:00Z"), Some("2026-11-02T09:04:05Z")),
            Some("4m 05s".into())
        );
        assert_eq!(
            duration_between(Some("2026-11-02T09:00:00Z"), Some("2026-11-02T10:02:30Z")),
            Some("1h 02m".into())
        );
        // Azure's not-started sentinel and a missing end are both "no answer"
        assert_eq!(duration_between(Some("0001-01-01T00:00:00Z"), None), None);
        assert_eq!(duration_between(None, None), None);
        // a running job counts up rather than saying nothing
        assert!(duration_between(Some("2026-11-02T09:00:00Z"), None).is_some());
    }

    #[test]
    fn the_last_log_entry_is_the_one_fetched() {
        let logs = serde_json::json!({"value": [
            {"id": 1, "url": "https://dev.azure.com/o/p/_apis/build/builds/7/logs/1"},
            {"id": 2, "url": "https://dev.azure.com/o/p/_apis/build/builds/7/logs/2"}
        ]});

        assert_eq!(
            last_log_url(Some(&logs)).as_deref(),
            Some("https://dev.azure.com/o/p/_apis/build/builds/7/logs/2")
        );
        assert_eq!(last_log_url(None), None);
        assert_eq!(last_log_url(Some(&serde_json::json!({"value": []}))), None);
    }
}
