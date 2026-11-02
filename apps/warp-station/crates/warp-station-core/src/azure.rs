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
                .map(|b| PipelineRun {
                    id: b.get("id").and_then(|i| i.as_u64()).unwrap_or_default(),
                    build_number: b
                        .get("buildNumber")
                        .and_then(|n| n.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    definition_name: b
                        .pointer("/definition/name")
                        .and_then(|n| n.as_str())
                        .map(str::to_string),
                    branch: b
                        .pointer("/sourceBranch")
                        .and_then(|n| n.as_str())
                        .map(str::to_string),
                    result: b.get("result").and_then(|r| r.as_str()).map(str::to_string),
                    status: b.get("status").and_then(|s| s.as_str()).map(str::to_string),
                    url: b
                        .pointer("/_links/web/href")
                        .and_then(|u| u.as_str())
                        .map(str::to_string),
                })
                .collect()
        })
        .unwrap_or_default()
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
}
