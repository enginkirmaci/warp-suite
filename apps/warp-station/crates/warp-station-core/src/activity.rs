//! The activity vocabulary the suite shares across sources: where a row came
//! from, what state its run is in, and one run in the words the lists and the
//! detail draw. Headless on purpose — no network and no gpui — so the Pipelines
//! workbench can interleave GitHub Actions and Azure Pipelines runs in one
//! list, and the pull-request and issue lists can join the same way later.
//!
//! The run detail's parts (`RunJob`, `RunStage`, `RunDetail`) live here too:
//! both sources produce them, one from a timeline and one from `gh run view`.
//! `azure` re-exports them, so nothing that drew an Azure run before had to
//! change its paths.

use chrono::{DateTime, Datelike as _, Local};

/// Where one activity row comes from. The tag on the row says it, and the
/// open path and the wording follow it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ActivitySource {
    #[default]
    GitHub,
    Azure,
}

impl ActivitySource {
    /// The row's quiet tag.
    pub fn label(self) -> &'static str {
        match self {
            Self::GitHub => "GitHub",
            Self::Azure => "Azure",
        }
    }

    /// Where the row opens, in the button's words.
    pub fn open_label(self) -> &'static str {
        match self {
            Self::GitHub => "Open on GitHub",
            Self::Azure => "Open on Azure DevOps",
        }
    }

    /// The platform's full name, for a sentence rather than a tag.
    pub fn platform(self) -> &'static str {
        match self {
            Self::GitHub => "GitHub",
            Self::Azure => "Azure DevOps",
        }
    }
}

/// What a list row is: a pull request, or an issue (a work item on Azure).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ActivityKind {
    #[default]
    PullRequest,
    Issue,
}

impl ActivityKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::PullRequest => "Pull request",
            Self::Issue => "Issue",
        }
    }

    /// The plural the list's empty states and headers use.
    pub fn plural(self) -> &'static str {
        match self {
            Self::PullRequest => "pull requests",
            Self::Issue => "issues",
        }
    }
}

impl ActivitySource {
    /// The platform's own reference for a row: `#482` on GitHub for both kinds,
    /// `!214` for an Azure pull request, `#3381` for a work item.
    pub fn reference(self, kind: ActivityKind, number: u64) -> String {
        match (self, kind) {
            (Self::Azure, ActivityKind::PullRequest) => format!("!{number}"),
            _ => format!("#{number}"),
        }
    }
}

/// One row of the Pull requests or Issues list, whichever platform answered.
/// The list and the filter read this; the detail is the platform's own view,
/// opened by `source` and `kind`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ActivityItem {
    pub source: ActivitySource,
    pub kind: ActivityKind,
    pub number: u64,
    pub title: String,
    pub url: String,
    pub author: Option<String>,
    pub labels: Vec<String>,
    /// The platform's state word: `OPEN`, `MERGED`, `active`, `Completed`…
    pub state: Option<String>,
    pub is_draft: bool,
    pub review_decision: Option<String>,
    pub updated_at: Option<String>,
    pub created_at: Option<String>,
    pub comments: u32,
    pub head_ref: Option<String>,
    pub base_ref: Option<String>,
    /// Azure work items only: `Bug`, `User Story`, …
    pub work_item_type: Option<String>,
    /// Azure work items only.
    pub assigned_to: Option<String>,
}

impl ActivityItem {
    pub fn of_github(item: &crate::github::GitHubItem) -> Self {
        Self {
            source: ActivitySource::GitHub,
            kind: if item.is_pull_request {
                ActivityKind::PullRequest
            } else {
                ActivityKind::Issue
            },
            number: item.number,
            title: item.title.clone(),
            url: item.url.clone(),
            author: item.author.clone(),
            labels: item.labels.clone(),
            state: item.state.clone().filter(|state| !state.trim().is_empty()),
            is_draft: item.is_draft,
            review_decision: item.review_decision.clone(),
            updated_at: item.updated_at.clone(),
            created_at: item.created_at.clone(),
            comments: item.comments,
            head_ref: item.head_ref_name.clone(),
            base_ref: item.base_ref_name.clone(),
            work_item_type: None,
            assigned_to: None,
        }
    }

    pub fn of_azure(item: &crate::azure::AzureItem, kind: ActivityKind) -> Self {
        Self {
            source: ActivitySource::Azure,
            kind,
            number: item.number,
            title: item.title.clone(),
            url: item.url.clone(),
            author: item.author.clone(),
            labels: item.labels.clone(),
            state: item.state.clone().filter(|state| !state.trim().is_empty()),
            is_draft: item.is_draft,
            review_decision: None,
            updated_at: item.changed_at.clone(),
            created_at: None,
            comments: 0,
            head_ref: item.source_ref_name.as_deref().map(short_ref),
            base_ref: item.target_ref_name.as_deref().map(short_ref),
            work_item_type: item.work_item_type.clone(),
            assigned_to: item.assigned_to.clone(),
        }
    }

    /// `#482`, `!214` or `#3381`, per the platform and the kind.
    pub fn reference(&self) -> String {
        self.source.reference(self.kind, self.number)
    }

    /// The row's state in the list's words: a draft says so, a merged pull
    /// request reads `merged` and an abandoned one `abandoned`.
    pub fn state_word(&self) -> String {
        match (self.source, self.kind) {
            (_, ActivityKind::PullRequest) if self.is_draft => "draft".to_string(),
            (ActivitySource::GitHub, ActivityKind::PullRequest) => match self.state.as_deref() {
                Some("MERGED") => "merged".to_string(),
                Some("CLOSED") => "closed".to_string(),
                _ => "open".to_string(),
            },
            (ActivitySource::Azure, ActivityKind::PullRequest) => match self.state.as_deref() {
                Some("completed") => "merged".to_string(),
                Some("abandoned") => "abandoned".to_string(),
                _ => "active".to_string(),
            },
            (ActivitySource::GitHub, ActivityKind::Issue) => {
                self.state.as_deref().unwrap_or("open").to_lowercase()
            }
            // Azure's work-item states are project-defined and read as written
            (ActivitySource::Azure, ActivityKind::Issue) => {
                self.state.clone().unwrap_or_else(|| "Active".to_string())
            }
        }
    }

    /// The row rebuilt as the GitHub detail seeds itself from: the payload
    /// fills the rest, so this only has to carry what the pane shows before
    /// it lands.
    pub fn to_github_item(&self) -> crate::github::GitHubItem {
        crate::github::GitHubItem {
            number: self.number,
            title: self.title.clone(),
            url: self.url.clone(),
            author: self.author.clone(),
            labels: self.labels.clone(),
            is_draft: self.is_draft,
            review_decision: self.review_decision.clone(),
            updated_at: self.updated_at.clone(),
            created_at: self.created_at.clone(),
            comments: self.comments,
            head_ref_name: self.head_ref.clone(),
            base_ref_name: self.base_ref.clone(),
            state: self.state.clone(),
            is_pull_request: self.kind == ActivityKind::PullRequest,
        }
    }

    /// The row rebuilt for the Azure pull-request detail, as above.
    pub fn to_azure_item(&self) -> crate::azure::AzureItem {
        crate::azure::AzureItem {
            number: self.number,
            title: self.title.clone(),
            url: self.url.clone(),
            author: self.author.clone(),
            labels: self.labels.clone(),
            is_draft: self.is_draft,
            state: self.state.clone(),
            source_ref_name: self.head_ref.clone(),
            target_ref_name: self.base_ref.clone(),
            work_item_type: self.work_item_type.clone(),
            assigned_to: self.assigned_to.clone(),
            changed_at: self.updated_at.clone(),
        }
    }

    /// The Issues filter: the reference, the title, the labels, the work-item
    /// type, the state and the assignee, all lowercase.
    pub fn matches(&self, query: &str) -> bool {
        if query.is_empty() {
            return true;
        }
        let haystack = format!(
            "{} {} {} {} {} {} {}",
            self.reference(),
            self.number,
            self.title,
            self.labels.join(" "),
            self.work_item_type.clone().unwrap_or_default(),
            self.state.clone().unwrap_or_default(),
            self.assigned_to.clone().unwrap_or_default(),
        );
        haystack.to_lowercase().contains(query)
    }
}

/// `refs/heads/main` → `main`.
fn short_ref(reference: &str) -> String {
    reference
        .strip_prefix("refs/heads/")
        .unwrap_or(reference)
        .to_string()
}

/// How the Issues list orders its rows: the union sorts as one list, so the
/// comparator lives on the shared type rather than on either source's item.
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

    /// Both platforms answer RFC 3339 timestamps, so they sort as text. A work
    /// item that carries no update time sorts last on `Updated`.
    pub fn sort(self, items: &mut [ActivityItem]) {
        match self {
            Self::Updated => items.sort_by(|a, b| b.updated_at.cmp(&a.updated_at)),
            Self::Newest => items.sort_by(|a, b| b.created_at.cmp(&a.created_at)),
            Self::Oldest => items.sort_by(|a, b| a.created_at.cmp(&b.created_at)),
            Self::Comments => items.sort_by_key(|item| std::cmp::Reverse(item.comments)),
        }
    }
}

/// The counts the rail, the tab strip and the summary read: both platforms
/// summed, because a row is a row whichever one answered.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ActivityCounts {
    pub pulls: usize,
    pub issues: usize,
}

impl ActivityCounts {
    pub fn of(items: &[ActivityItem]) -> Self {
        items
            .iter()
            .fold(Self::default(), |counts, item| match item.kind {
                ActivityKind::PullRequest => Self {
                    pulls: counts.pulls + 1,
                    issues: counts.issues,
                },
                ActivityKind::Issue => Self {
                    pulls: counts.pulls,
                    issues: counts.issues + 1,
                },
            })
    }
}

/// One run's state in the app's own words, whichever platform it ran on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RunState {
    #[default]
    Queued,
    Running,
    Succeeded,
    Failed,
    Skipped,
}

impl RunState {
    pub fn word(self) -> &'static str {
        match self {
            Self::Queued => "Queued",
            Self::Running => "Running",
            Self::Succeeded => "Succeeded",
            Self::Failed => "Failed",
            Self::Skipped => "Skipped",
        }
    }

    pub fn is_failed(self) -> bool {
        self == Self::Failed
    }

    pub fn is_running(self) -> bool {
        self == Self::Running
    }

    pub fn is_succeeded(self) -> bool {
        self == Self::Succeeded
    }

    /// Queued or skipped: the clock glyph, not a spinner.
    pub fn is_idle(self) -> bool {
        matches!(self, Self::Queued | Self::Skipped)
    }
}

/// One run, as the merged list draws it and the detail's head reads it. The
/// commit, the trigger and the times are filled by the detail's own call; a
/// list payload carries none of them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ActivityRun {
    pub source: ActivitySource,
    pub id: u64,
    /// The workflow or pipeline name.
    pub name: String,
    /// The run number, or the build number Azure names it by.
    pub label: String,
    pub branch: Option<String>,
    pub state: RunState,
    pub url: Option<String>,
    /// ISO timestamp the list sorts on: when the run started its life.
    pub at: Option<String>,
    pub sha: Option<String>,
    /// The platform's own word: `push`, `pull_request`, `manual`, …
    pub trigger: Option<String>,
    pub start: Option<String>,
    pub finish: Option<String>,
}

impl ActivityRun {
    pub fn of_github(run: &crate::github::GitHubRun) -> Self {
        Self {
            source: ActivitySource::GitHub,
            id: run.id,
            name: run.name.clone(),
            // the design's `#1284`: the run's number, and its title when gh
            // answered a payload without one
            label: if run.number > 0 {
                format!("#{}", run.number)
            } else {
                run.title.clone()
            },
            branch: run.branch.clone(),
            state: github_state(run.status.as_deref(), run.conclusion.as_deref()),
            url: run.url.clone(),
            at: run.created_at.clone(),
            sha: None,
            trigger: None,
            start: None,
            finish: None,
        }
    }

    pub fn of_azure(run: &crate::azure::PipelineRun) -> Self {
        Self {
            source: ActivitySource::Azure,
            id: run.id,
            name: run
                .definition_name
                .clone()
                .unwrap_or_else(|| "Pipeline".to_string()),
            label: run.build_number.clone(),
            branch: run.branch.clone(),
            state: azure_state(run),
            url: run.url.clone(),
            at: run.at.clone(),
            sha: run.source_version.clone(),
            trigger: run.reason.clone(),
            start: run.start.clone(),
            finish: run.finish.clone(),
        }
    }

    pub fn is_failed(&self) -> bool {
        self.state.is_failed()
    }

    pub fn is_running(&self) -> bool {
        self.state.is_running()
    }

    pub fn is_succeeded(&self) -> bool {
        self.state.is_succeeded()
    }

    pub fn state_word(&self) -> &'static str {
        self.state.word()
    }

    /// The branch without git's `refs/heads/` spelling.
    pub fn display_branch(&self) -> Option<String> {
        self.branch
            .as_deref()
            .map(|b| b.strip_prefix("refs/heads/").unwrap_or(b).to_string())
    }

    pub fn short_sha(&self) -> Option<String> {
        self.sha
            .as_deref()
            .filter(|sha| !sha.trim().is_empty())
            .map(|sha| sha.chars().take(7).collect())
    }

    /// Why the run happened, in the app's words. Both vocabularies answer:
    /// Azure's reasons and GitHub's events.
    pub fn trigger_word(&self) -> Option<&'static str> {
        match self.trigger.as_deref()? {
            "individualCI" | "batchedCI" | "push" => Some("Continuous integration"),
            "manual" | "workflow_dispatch" => Some("Manual"),
            "schedule" => Some("Schedule"),
            "pullRequest" | "pull_request" => Some("Pull request"),
            "buildCompletion" => Some("Build completion"),
            "workflow_run" => Some("Workflow run"),
            "repository_dispatch" => Some("Repository dispatch"),
            "release" => Some("Release"),
            _ => None,
        }
    }

    /// The sort key, newest first: seconds since the epoch, whatever the
    /// source's timestamp format.
    pub fn at_key(&self) -> i64 {
        self.at
            .as_deref()
            .and_then(|at| DateTime::parse_from_rfc3339(at).ok())
            .map(|at| at.timestamp())
            .unwrap_or(0)
    }
}

/// Newest first, by the timestamp the row carries. A pair that ties keeps the
/// order it arrived in, so the list does not jitter between frames.
pub fn sort_newest_first(runs: &mut [ActivityRun]) {
    runs.sort_by_key(|run| std::cmp::Reverse(run.at_key()));
}

/// GitHub's pair of words (`status`, `conclusion`) as one state. A run that
/// has not finished is running or waiting, whatever its conclusion says.
pub(crate) fn github_state(status: Option<&str>, conclusion: Option<&str>) -> RunState {
    match status {
        Some("completed") => match conclusion {
            Some("success" | "neutral") => RunState::Succeeded,
            Some("skipped") => RunState::Skipped,
            Some("failure" | "cancelled" | "timed_out" | "action_required" | "startup_failure") => {
                RunState::Failed
            }
            // completed without a conclusion is a payload we cannot read
            _ => RunState::Queued,
        },
        Some("in_progress") => RunState::Running,
        _ => RunState::Queued,
    }
}

/// Azure's `result`/`status` pair as one state.
pub(crate) fn azure_state(run: &crate::azure::PipelineRun) -> RunState {
    if run.is_running() {
        RunState::Running
    } else if run.is_failed() {
        RunState::Failed
    } else if run.is_succeeded() {
        RunState::Succeeded
    } else if run.result.as_deref() == Some("skipped") {
        RunState::Skipped
    } else {
        RunState::Queued
    }
}

/// One job inside a run's stage: Azure's timeline job, or a GitHub Actions
/// job's step. `state` speaks Azure's words (`inProgress`, `completed`,
/// `pending`) and `result` speaks its conclusions (`succeeded`, `failed`,
/// `canceled`, `skipped`) whichever platform parsed it, so the one row
/// renderer draws both.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RunJob {
    pub name: String,
    pub state: Option<String>,
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

    /// The short time the row prints: how long it took, or when it started.
    pub fn duration(&self) -> Option<String> {
        duration_between(self.start.as_deref(), self.finish.as_deref())
    }
}

/// One stage of a run, with the jobs it holds. The design draws one card per
/// stage and its jobs under it; a GitHub Actions run has no stages, so its
/// cards are one per job and the rows are the job's steps.
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

/// One run's detail: the head facts the list payload does not carry, the state
/// the platform reports for the run itself, the stage cards and the tail of
/// the log.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RunDetail {
    pub sha: Option<String>,
    pub trigger: Option<String>,
    pub start: Option<String>,
    pub finish: Option<String>,
    /// What the platform says the run itself is doing. The Pipelines pane's
    /// follow loop reads it to know when to stop asking, and the head takes it
    /// over the row's older word while the run is open.
    pub state: Option<RunState>,
    pub stages: Vec<RunStage>,
    pub log: String,
}

impl RunDetail {
    /// Whether the run can still change: queued, running, or a payload that
    /// left the state out. The follow loop keeps its cadence while this holds
    /// and stops on the first answer that says otherwise.
    pub fn still_going(&self) -> bool {
        !matches!(
            self.state,
            Some(RunState::Succeeded | RunState::Failed | RunState::Skipped)
        )
    }
}

/// A run's length in the short units the strip prints: `12s`, `4m 05s`,
/// `1h 02m`. A run still going counts up to now; Azure's year-1 sentinel (an
/// unset time) reads as no answer.
pub fn duration_between(start: Option<&str>, finish: Option<&str>) -> Option<String> {
    let parse = |text: Option<&str>| {
        let text = text?;
        let at = DateTime::parse_from_rfc3339(text).ok()?;
        (at.year() > 2000).then_some(at.with_timezone(&Local))
    };
    let start = parse(start)?;
    // a finish in the sentinel's year is no finish: the run counts up
    let finish = finish
        .and_then(|text| parse(Some(text)))
        .unwrap_or_else(Local::now);
    let seconds = (finish - start).num_seconds().max(0);
    Some(match seconds {
        0..=59 => format!("{seconds}s"),
        60..=3599 => format!("{}m {:02}s", seconds / 60, seconds % 60),
        _ => format!("{}h {:02}m", seconds / 3600, (seconds % 3600) / 60),
    })
}

/// The tail of a log, cut on a line boundary so the excerpt starts at a whole
/// line rather than mid-word.
pub fn log_tail(text: &str, cap: usize) -> String {
    if text.len() <= cap {
        return text.to_string();
    }
    let start = text.len() - cap;
    let start = text[start..]
        .find('\n')
        .map_or(start, |offset| start + offset + 1);
    text[start..].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn github(status: Option<&str>, conclusion: Option<&str>) -> crate::github::GitHubRun {
        crate::github::GitHubRun {
            id: 7,
            number: 1284,
            name: "ci".into(),
            title: "1234".into(),
            branch: Some("feat/x".into()),
            status: status.map(str::to_string),
            conclusion: conclusion.map(str::to_string),
            url: Some("https://github.com/o/r/actions/runs/7".into()),
            created_at: Some("2026-11-10T09:00:00Z".into()),
        }
    }

    fn azure(result: Option<&str>, status: Option<&str>) -> crate::azure::PipelineRun {
        crate::azure::PipelineRun {
            id: 11,
            build_number: "2026.11.10.1".into(),
            definition_name: Some("lumux-ci".into()),
            branch: Some("refs/heads/main".into()),
            result: result.map(str::to_string),
            status: status.map(str::to_string),
            at: Some("2026-11-10T10:00:00Z".into()),
            ..Default::default()
        }
    }

    #[test]
    fn a_github_row_prints_its_run_number() {
        let run = ActivityRun::of_github(&github(Some("completed"), Some("success")));
        assert_eq!(run.label, "#1284");
        assert_eq!(
            ActivityRun::of_github(&crate::github::GitHubRun {
                number: 0,
                title: "Unify the gutter".into(),
                ..github(None, None)
            })
            .label,
            "Unify the gutter",
            "a payload without a number falls back to the title"
        );
    }

    #[test]
    fn a_github_run_reads_its_state_from_status_and_conclusion() {
        assert_eq!(
            ActivityRun::of_github(&github(Some("in_progress"), None)).state,
            RunState::Running
        );
        assert_eq!(
            ActivityRun::of_github(&github(Some("queued"), None)).state,
            RunState::Queued
        );
        assert_eq!(
            ActivityRun::of_github(&github(Some("completed"), Some("success"))).state,
            RunState::Succeeded
        );
        assert_eq!(
            ActivityRun::of_github(&github(Some("completed"), Some("failure"))).state,
            RunState::Failed
        );
        assert_eq!(
            ActivityRun::of_github(&github(Some("completed"), Some("cancelled"))).state,
            RunState::Failed
        );
        assert_eq!(
            ActivityRun::of_github(&github(Some("completed"), Some("skipped"))).state,
            RunState::Skipped
        );
    }

    #[test]
    fn an_azure_run_reads_its_state_from_result_and_status() {
        assert_eq!(
            ActivityRun::of_azure(&azure(None, Some("inProgress"))).state,
            RunState::Running
        );
        assert_eq!(
            ActivityRun::of_azure(&azure(Some("failed"), Some("completed"))).state,
            RunState::Failed
        );
        assert_eq!(
            ActivityRun::of_azure(&azure(Some("succeeded"), Some("completed"))).state,
            RunState::Succeeded
        );
        assert_eq!(
            ActivityRun::of_azure(&azure(Some("skipped"), Some("completed"))).state,
            RunState::Skipped
        );
        assert_eq!(
            ActivityRun::of_azure(&azure(None, Some("notStarted"))).state,
            RunState::Queued
        );
    }

    #[test]
    fn both_vocabularies_name_their_trigger() {
        let github = ActivityRun {
            trigger: Some("pull_request".into()),
            ..Default::default()
        };
        let azure = ActivityRun {
            trigger: Some("individualCI".into()),
            ..Default::default()
        };

        assert_eq!(github.trigger_word(), Some("Pull request"));
        assert_eq!(azure.trigger_word(), Some("Continuous integration"));
        assert_eq!(ActivityRun::default().trigger_word(), None);
    }

    #[test]
    fn the_branch_and_the_sha_are_shortened_for_the_row() {
        let run = ActivityRun::of_azure(&azure(Some("succeeded"), Some("completed")));
        assert_eq!(run.display_branch().as_deref(), Some("main"));

        let run = ActivityRun {
            sha: Some("9f2c1ab7c0ffee".into()),
            ..Default::default()
        };
        assert_eq!(run.short_sha().as_deref(), Some("9f2c1ab"));
        assert_eq!(ActivityRun::default().short_sha(), None);
    }

    #[test]
    fn the_list_sorts_newest_first_across_sources() {
        let mut runs = vec![
            ActivityRun::of_azure(&azure(Some("succeeded"), Some("completed"))),
            ActivityRun::of_github(&github(Some("completed"), Some("success"))),
        ];
        // GitHub at 09:00, Azure at 10:00
        sort_newest_first(&mut runs);

        assert_eq!(runs[0].source, ActivitySource::Azure);
        assert_eq!(runs[1].source, ActivitySource::GitHub);
    }

    #[test]
    fn a_run_state_prints_the_word_the_design_gives_it() {
        for (state, word) in [
            (RunState::Queued, "Queued"),
            (RunState::Running, "Running"),
            (RunState::Succeeded, "Succeeded"),
            (RunState::Failed, "Failed"),
            (RunState::Skipped, "Skipped"),
        ] {
            assert_eq!(state.word(), word);
        }
        assert!(RunState::Skipped.is_idle());
        assert!(!RunState::Failed.is_idle());
    }

    #[test]
    fn a_duration_counts_up_and_reads_in_short_units() {
        assert_eq!(
            duration_between(Some("2026-11-10T09:00:00Z"), Some("2026-11-10T09:00:12Z")).as_deref(),
            Some("12s")
        );
        assert_eq!(
            duration_between(Some("2026-11-10T09:00:00Z"), Some("2026-11-10T09:04:05Z")).as_deref(),
            Some("4m 05s")
        );
        assert_eq!(
            duration_between(Some("2026-11-10T09:00:00Z"), Some("2026-11-10T10:02:00Z")).as_deref(),
            Some("1h 02m")
        );
        assert_eq!(duration_between(None, None), None);
        assert_eq!(duration_between(Some("not a date"), None), None);
        // Azure's sentinel year is no finish, so the run counts up to now
        assert!(
            duration_between(Some("2026-11-10T09:00:00Z"), Some("0001-01-01T00:00:00Z")).is_some()
        );
    }

    #[test]
    fn a_log_tail_starts_at_a_whole_line() {
        let text = "one\ntwo\nthree\nfour\n";
        assert_eq!(log_tail(text, 100), text);
        let tail = log_tail(text, 12);
        assert!(text.ends_with(&tail), "the tail is the end of the log");
        assert!(
            tail.starts_with("three"),
            "cut on a line, not mid-word: {tail}"
        );
    }

    fn github_row(number: u64, pull: bool) -> crate::github::GitHubItem {
        crate::github::GitHubItem {
            number,
            title: format!("item {number}"),
            url: format!("https://example.invalid/{number}"),
            author: Some("engin".into()),
            labels: vec!["bug".into()],
            is_draft: false,
            review_decision: None,
            updated_at: Some("2026-11-02T09:00:00Z".into()),
            created_at: Some("2026-11-01T09:00:00Z".into()),
            comments: if pull { 0 } else { 3 },
            head_ref_name: pull.then(|| "feat/x".to_string()),
            base_ref_name: pull.then(|| "main".to_string()),
            state: Some("OPEN".into()),
            is_pull_request: pull,
        }
    }

    fn azure_row(number: u64, pull: bool) -> crate::azure::AzureItem {
        crate::azure::AzureItem {
            number,
            title: format!("item {number}"),
            url: format!("https://example.invalid/{number}"),
            author: Some("engin".into()),
            labels: Vec::new(),
            is_draft: false,
            state: Some(if pull {
                "active".into()
            } else {
                "Active".into()
            }),
            source_ref_name: pull.then(|| "refs/heads/feat/x".to_string()),
            target_ref_name: pull.then(|| "refs/heads/main".to_string()),
            work_item_type: (!pull).then(|| "Bug".to_string()),
            assigned_to: (!pull).then(|| "engin".to_string()),
            changed_at: Some("2026-11-02T10:00:00Z".into()),
        }
    }

    #[test]
    fn each_platform_prints_its_own_reference() {
        let github_pull = ActivityItem::of_github(&github_row(482, true));
        let azure_pull = ActivityItem::of_azure(&azure_row(214, true), ActivityKind::PullRequest);
        let github_issue = ActivityItem::of_github(&github_row(3381, false));
        let azure_item = ActivityItem::of_azure(&azure_row(3381, false), ActivityKind::Issue);
        assert_eq!(github_pull.reference(), "#482");
        assert_eq!(azure_pull.reference(), "!214");
        assert_eq!(github_issue.reference(), "#3381");
        assert_eq!(
            azure_item.reference(),
            "#3381",
            "a work item keeps the GitHub shape"
        );
    }

    #[test]
    fn a_row_reads_the_state_word_its_platform_uses() {
        let mut pull = ActivityItem::of_github(&github_row(482, true));
        assert_eq!(pull.state_word(), "open");
        pull.is_draft = true;
        assert_eq!(pull.state_word(), "draft");
        pull.is_draft = false;
        pull.state = Some("MERGED".into());
        assert_eq!(pull.state_word(), "merged");

        let mut azure = ActivityItem::of_azure(&azure_row(214, true), ActivityKind::PullRequest);
        assert_eq!(azure.state_word(), "active");
        azure.state = Some("completed".into());
        assert_eq!(azure.state_word(), "merged");
        azure.state = Some("abandoned".into());
        assert_eq!(azure.state_word(), "abandoned");

        let item = ActivityItem::of_azure(&azure_row(3381, false), ActivityKind::Issue);
        assert_eq!(
            item.state_word(),
            "Active",
            "a work item's state reads as written"
        );
    }

    #[test]
    fn the_filter_looks_through_both_platforms() {
        let pull = ActivityItem::of_azure(&azure_row(214, true), ActivityKind::PullRequest);
        assert!(pull.matches("!214"));
        assert!(pull.matches("item 214"));
        assert!(pull.matches("214"));
        assert!(!pull.matches("no-such-thing"));

        let item = ActivityItem::of_azure(&azure_row(3381, false), ActivityKind::Issue);
        assert!(item.matches("bug"), "the work-item type");
        assert!(item.matches("active"), "the state");
        assert!(item.matches("engin"), "the assignee");
        assert!(item.matches("3381"), "the number without its prefix");
    }

    #[test]
    fn a_row_rebuilds_the_item_its_detail_seeds_from() {
        let pull = ActivityItem::of_azure(&azure_row(214, true), ActivityKind::PullRequest);
        let item = pull.to_azure_item();
        assert_eq!(item.number, 214);
        assert_eq!(item.source_ref_name.as_deref(), Some("feat/x"));
        assert_eq!(item.target_ref_name.as_deref(), Some("main"));

        let github = ActivityItem::of_github(&github_row(482, true));
        let rebuilt = github.to_github_item();
        assert_eq!(rebuilt.number, 482);
        assert!(rebuilt.is_pull_request);
        assert_eq!(rebuilt.head_ref_name.as_deref(), Some("feat/x"));
    }

    fn row(number: u64, updated: &str, comments: u32) -> ActivityItem {
        ActivityItem {
            number,
            title: format!("item {number}"),
            updated_at: Some(updated.into()),
            created_at: Some(updated.into()),
            comments,
            ..Default::default()
        }
    }

    #[test]
    fn the_issue_sorts_order_the_union() {
        let mut items = vec![
            row(1, "2026-11-01T00:00:00Z", 5),
            row(2, "2026-11-03T00:00:00Z", 1),
            row(3, "2026-11-02T00:00:00Z", 0),
        ];
        IssueSort::Updated.sort(&mut items);
        assert_eq!(
            items.iter().map(|i| i.number).collect::<Vec<_>>(),
            [2, 3, 1]
        );
        IssueSort::Newest.sort(&mut items);
        assert_eq!(
            items.iter().map(|i| i.number).collect::<Vec<_>>(),
            [2, 3, 1]
        );
        IssueSort::Oldest.sort(&mut items);
        assert_eq!(
            items.iter().map(|i| i.number).collect::<Vec<_>>(),
            [1, 3, 2]
        );
        IssueSort::Comments.sort(&mut items);
        assert_eq!(
            items.iter().map(|i| i.number).collect::<Vec<_>>(),
            [1, 2, 3]
        );
        // a work item carries no comments, which the sort reads as zero
        for sort in IssueSort::ALL {
            assert_eq!(IssueSort::from_key(Some(sort.key())), sort);
        }
        assert_eq!(IssueSort::from_key(None), IssueSort::Updated);
    }

    #[test]
    fn the_counts_sum_both_platforms() {
        let items = vec![
            ActivityItem::of_github(&github_row(1, true)),
            ActivityItem::of_azure(&azure_row(2, true), ActivityKind::PullRequest),
            ActivityItem::of_github(&github_row(3, false)),
            ActivityItem::of_azure(&azure_row(4, false), ActivityKind::Issue),
        ];
        let counts = ActivityCounts::of(&items);
        assert_eq!(counts.pulls, 2);
        assert_eq!(counts.issues, 2);
    }
}

#[cfg(test)]
mod follow_tests {
    use super::{RunDetail, RunState};

    /// The follow loop's stop rule: only a finished run's own state ends it,
    /// and an answer that leaves the state out keeps the loop alive rather
    /// than freezing a run that is visibly going.
    #[test]
    fn a_run_detail_knows_when_the_run_is_over() {
        let going = |state| {
            RunDetail {
                state: Some(state),
                ..Default::default()
            }
            .still_going()
        };
        assert!(going(RunState::Running));
        assert!(going(RunState::Queued));
        assert!(!going(RunState::Succeeded));
        assert!(!going(RunState::Failed));
        assert!(!going(RunState::Skipped));
        assert!(
            RunDetail::default().still_going(),
            "a payload without the state is not proof of a finish"
        );
    }

    /// GitHub's own pair maps a completed run with its conclusion, and an
    /// unfinished one to running or the queue whatever the conclusion says.
    #[test]
    fn the_github_pair_reads_as_the_run_state() {
        use super::github_state;
        assert_eq!(github_state(Some("in_progress"), None), RunState::Running);
        assert_eq!(github_state(Some("queued"), None), RunState::Queued);
        assert_eq!(
            github_state(Some("completed"), Some("failure")),
            RunState::Failed
        );
        assert_eq!(
            github_state(Some("completed"), Some("success")),
            RunState::Succeeded
        );
        assert_eq!(
            github_state(Some("completed"), Some("skipped")),
            RunState::Skipped
        );
    }
}
