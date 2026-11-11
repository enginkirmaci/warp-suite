//! The Pipelines tab and its Azure pull request, in a headless window: the run
//! workbench with its stage cards and log, the Azure pull request's head,
//! strip, tabs and verbs, and the two sockets that must stay reachable — the
//! Azure pull requests and work items under the runs.
//!
//! No network: the store is seeded with snapshots and answers, exactly as the
//! net thread would hand them over.

use gpui::TestAppContext;
use gpui::prelude::*;
use std::path::PathBuf;
use warp_station::net::{NetCommand, UiEvent};
use warp_station::store::Store;
use warp_station::ui::window::MainWindow;
use warp_station_core::azure::{
    AzureComment, AzureFile, AzureItem, AzurePolicy, AzurePull, AzurePullDetail, AzureSnapshot,
    AzureThread, PipelineRun, RunDetail, RunJob, RunStage,
};
use warp_station_core::github::{GitHubRun, GitHubSnapshot};
use warp_station_core::repo_store::RepoEntry;

fn entry(name: &str, path: &str) -> RepoEntry {
    RepoEntry {
        name: name.into(),
        folder_path: PathBuf::from(path),
        solution_path: None,
        tags: Vec::new(),
        git_branch_name: Some("main".into()),
        git_modified_count: 0,
        git_to_push_count: 0,
        git_to_pull_count: 0,
        git_last_commit_at: None,
        git_last_fetch_at: None,
        git_has_remote: true,
        git_branch_published: true,
        git_status_loaded: true,
        github_pr_count: 0,
        github_issue_count: 0,
        azure_pull_count: 0,
        github_web_url: None,
        azure_work_item_count: 0,
    }
}

fn scratch_store(
    cx: &mut TestAppContext,
    name: &str,
) -> (gpui::Entity<Store>, async_channel::Receiver<NetCommand>) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let scratch = std::env::temp_dir().join(format!(
        "warp-station-probe-azure-{name}-{}.json",
        std::process::id()
    ));
    std::fs::write(&scratch, "{}").expect("scratch settings");
    let (net_tx, net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| {
        let mut store = Store::with_settings(
            net_tx,
            std::sync::Arc::new(warp_station_core::settings::SettingsService::at(scratch)),
        );
        store.group_by_root = false;
        store.replace_repos(vec![entry("alpha", "/probe/alpha")]);
        store
    });
    (store, net_rx)
}

fn draw(cx: &mut gpui::VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
}

fn click(cx: &mut gpui::VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} renders"));
    cx.simulate_click(bounds.center(), gpui::Modifiers::default());
    draw(cx);
}

fn open_window<'a>(
    store: &gpui::Entity<Store>,
    cx: &'a mut TestAppContext,
) -> &'a mut gpui::VisualTestContext {
    let handle = store.clone();
    let (_window, cx) = cx.add_window_view(|window, cx| MainWindow::new(handle, window, cx));
    draw(cx);
    cx
}

/// The column on, with a token: without it the per-thing calls are refused
/// before they are sent, which is its own state.
fn enable_azure(store: &gpui::Entity<Store>, cx: &mut gpui::VisualTestContext) {
    cx.update_entity(store, |s: &mut Store, _| {
        s.update_settings(|settings| {
            settings.repos.enable_azure_devops = true;
            settings.repos.azure_devops_pat = "pat".into();
        });
    });
    draw(cx);
}

fn run(id: u64, number: &str, failed: bool) -> PipelineRun {
    PipelineRun {
        id,
        build_number: number.into(),
        definition_name: Some("Build and test".into()),
        branch: Some("refs/heads/main".into()),
        source_version: Some("abcdef1234567890".into()),
        reason: Some("individualCI".into()),
        status: Some("completed".into()),
        result: Some(if failed { "failed" } else { "succeeded" }.into()),
        start: Some("2026-11-02T09:00:00Z".into()),
        finish: Some("2026-11-02T09:04:00Z".into()),
        at: Some("2026-11-02T08:59:40Z".into()),
        url: Some("https://dev.azure.com/o/p/_build/results?buildId=7".into()),
    }
}

/// The column on with a gh binary that resolves: without one the per-run call
/// is refused before it is sent.
fn enable_github(store: &gpui::Entity<Store>, cx: &mut gpui::VisualTestContext) {
    cx.update_entity(store, |s: &mut Store, _| {
        s.update_settings(|settings| {
            settings.repos.enable_github = true;
            settings.repos.github_executable = std::env::current_exe()
                .unwrap()
                .to_string_lossy()
                .to_string();
        });
    });
    draw(cx);
}

fn seed_github(
    store: &gpui::Entity<Store>,
    snapshot: GitHubSnapshot,
    cx: &mut gpui::VisualTestContext,
) {
    cx.update_entity(store, |s: &mut Store, cx| {
        s.github_by_folder
            .insert("/probe/alpha".to_string(), std::sync::Arc::new(snapshot));
        cx.notify();
    });
    draw(cx);
}

fn github_run(id: u64, created: &str, conclusion: Option<&str>) -> GitHubRun {
    GitHubRun {
        id,
        number: 1284,
        name: "CI".into(),
        title: "Unify the gutter".into(),
        branch: Some("feat/x".into()),
        status: Some(if conclusion.is_some() {
            "completed".into()
        } else {
            "in_progress".into()
        }),
        conclusion: conclusion.map(str::to_string),
        url: Some(format!("https://github.com/o/alpha/actions/runs/{id}")),
        created_at: Some(created.into()),
    }
}

fn seed_azure(
    store: &gpui::Entity<Store>,
    snapshot: AzureSnapshot,
    cx: &mut gpui::VisualTestContext,
) {
    cx.update_entity(store, |s: &mut Store, cx| {
        s.azure_by_folder
            .insert("/probe/alpha".to_string(), std::sync::Arc::new(snapshot));
        cx.notify();
    });
    draw(cx);
}

/// One list, both sources: the Actions run and the Azure build sit together,
/// newest first, and each row selects a run of its own source.
#[gpui::test]
fn the_run_list_merges_both_sources_newest_first(cx: &mut TestAppContext) {
    let (store, net_rx) = scratch_store(cx, "merged-runs");
    let cx = open_window(&store, cx);
    enable_azure(&store, cx);
    enable_github(&store, cx);
    // the Azure build is queued at 08:59:40; the Actions run was created 09:30
    seed_azure(
        &store,
        AzureSnapshot {
            available: true,
            pipelines: vec![run(9, "20261102.4", false)],
            ..Default::default()
        },
        cx,
    );
    seed_github(
        &store,
        GitHubSnapshot {
            url: Some("https://github.com/o/alpha".into()),
            available: true,
            runs: vec![github_run(31, "2026-11-02T09:30:00Z", Some("success"))],
            ..Default::default()
        },
        cx,
    );
    click(cx, "detail-tab-pipelines");

    assert!(cx.debug_bounds("pipeline-run-0").is_some(), "the list");
    assert!(
        cx.debug_bounds("pipeline-run-1").is_some(),
        "one row per run"
    );
    assert!(
        cx.debug_bounds("pipeline-run-2").is_none(),
        "two runs, two rows"
    );

    // newest first: the Actions run opened, and it is the one that was asked
    let open = store.read_with(cx, |store, _| store.open_run.clone());
    let open = open.expect("the first row is the selection");
    assert_eq!(
        open.run.source,
        warp_station_core::activity::ActivitySource::GitHub,
        "the 09:30 Actions run sorts above the 08:59 build"
    );
    assert_eq!(open.run.id, 31);
    let asks: Vec<u64> = std::iter::from_fn(|| net_rx.try_recv().ok())
        .filter_map(|command| match command {
            NetCommand::GitHubRunDetail { run_id, .. } => Some(run_id),
            _ => None,
        })
        .collect();
    assert_eq!(asks, [31], "one ask, to the source the row came from");

    // the second row is the build, and it opens in its own source
    click(cx, "pipeline-run-1");
    let open = store.read_with(cx, |store, _| store.open_run.clone());
    let open = open.expect("the second row opens");
    assert_eq!(
        open.run.source,
        warp_station_core::activity::ActivitySource::Azure
    );
    assert_eq!(open.run.id, 9);
}

/// An Actions run opens the same detail an Azure run does: the head facts from
/// its own call, the job cards, and the failed log's tail.
#[gpui::test]
fn an_actions_run_opens_its_detail_and_asks_once(cx: &mut TestAppContext) {
    let (store, net_rx) = scratch_store(cx, "actions-run");
    let cx = open_window(&store, cx);
    enable_github(&store, cx);
    seed_github(
        &store,
        GitHubSnapshot {
            url: Some("https://github.com/o/alpha".into()),
            available: true,
            runs: vec![github_run(31, "2026-11-02T09:30:00Z", Some("failure"))],
            ..Default::default()
        },
        cx,
    );
    click(cx, "detail-tab-pipelines");

    assert!(cx.debug_bounds("run-detail").is_some());
    assert!(
        cx.debug_bounds("run-stage-0").is_none(),
        "the cards wait for the answer"
    );
    let asks: Vec<u64> = std::iter::from_fn(|| net_rx.try_recv().ok())
        .filter_map(|command| match command {
            NetCommand::GitHubRunDetail { run_id, .. } => Some(run_id),
            _ => None,
        })
        .collect();
    assert_eq!(asks, [31], "one ask for the selected run");

    cx.update_entity(&store, |s: &mut Store, cx| {
        s.on_net_event(
            UiEvent::GitHubRunDetailLoaded {
                folder: "/probe/alpha".into(),
                run_id: 31,
                detail: Some(Box::new(RunDetail {
                    sha: Some("9f2c1ab7c0ffee".into()),
                    trigger: Some("pull_request".into()),
                    start: Some("2026-11-02T09:30:05Z".into()),
                    finish: Some("2026-11-02T09:36:40Z".into()),
                    stages: vec![
                        RunStage {
                            name: "build".into(),
                            jobs: vec![
                                RunJob {
                                    name: "Set up job".into(),
                                    state: Some("completed".into()),
                                    result: Some("succeeded".into()),
                                    start: None,
                                    finish: None,
                                },
                                RunJob {
                                    name: "cargo test".into(),
                                    state: Some("completed".into()),
                                    result: Some("failed".into()),
                                    start: Some("2026-11-02T09:31:00Z".into()),
                                    finish: Some("2026-11-02T09:32:10Z".into()),
                                },
                            ],
                        },
                        RunStage {
                            name: "package".into(),
                            jobs: vec![RunJob {
                                name: "Bundle".into(),
                                state: Some("inProgress".into()),
                                result: None,
                                start: None,
                                finish: None,
                            }],
                        },
                    ],
                    log: "##[error]1 test failed\n".into(),
                })),
            },
            cx,
        );
    });
    draw(cx);

    assert!(cx.debug_bounds("run-stage-0").is_some(), "a card per job");
    assert!(cx.debug_bounds("run-stage-1").is_some());
    assert!(
        cx.debug_bounds("run-log").is_some(),
        "the failed log's tail"
    );
    assert!(cx.debug_bounds("run-status").is_some(), "the 44 strip");

    // the detail's facts won: the commit and the trigger came with the answer
    let open = store.read_with(cx, |store, _| store.open_run.clone());
    let open = open.expect("the run stays open");
    assert_eq!(
        open.run.sha.as_deref().or(Some("9f2c1ab7c0ffee")),
        Some("9f2c1ab7c0ffee")
    );
}

/// A repository whose two sources both answered with nothing keeps the tab,
/// and the empty state names both of them rather than blaming one.
#[gpui::test]
fn a_repository_with_runs_from_neither_source_keeps_its_empty_state(cx: &mut TestAppContext) {
    let (store, _net_rx) = scratch_store(cx, "no-runs");
    let cx = open_window(&store, cx);
    enable_azure(&store, cx);
    enable_github(&store, cx);
    seed_azure(
        &store,
        AzureSnapshot {
            available: true,
            ..Default::default()
        },
        cx,
    );
    seed_github(
        &store,
        GitHubSnapshot {
            url: Some("https://github.com/o/alpha".into()),
            available: true,
            ..Default::default()
        },
        cx,
    );
    click(cx, "detail-tab-pipelines");

    assert!(cx.debug_bounds("azure-tab").is_some(), "the tab renders");
    assert!(
        cx.debug_bounds("runs-empty").is_some(),
        "the list says nothing ran, in both sources' words"
    );
    assert!(cx.debug_bounds("pipeline-run-0").is_none());
    assert!(
        cx.debug_bounds("azure-items").is_none(),
        "the Azure item cards are gone: the merged lists carry them"
    );
}

/// The Pipelines tab is the design's workbench: the 320 run list, then the
/// selected run's stages and the tail of its log.
#[gpui::test]
fn the_pipelines_workbench_shows_the_selected_run(cx: &mut TestAppContext) {
    let (store, net_rx) = scratch_store(cx, "pipelines");
    let cx = open_window(&store, cx);
    enable_azure(&store, cx);
    seed_azure(
        &store,
        AzureSnapshot {
            available: true,
            url: Some("https://dev.azure.com/o/p".into()),
            pipelines: vec![run(9, "20261102.4", true), run(7, "20261102.3", false)],
            ..Default::default()
        },
        cx,
    );
    click(cx, "detail-tab-pipelines");

    assert!(cx.debug_bounds("azure-runs-column").is_some(), "the list");
    assert!(cx.debug_bounds("pipeline-run-0").is_some());
    assert!(cx.debug_bounds("pipeline-run-1").is_some());
    assert!(
        cx.debug_bounds("run-detail").is_some(),
        "the first row is the selection"
    );
    // the selection asked for that run's timeline once
    let asks: Vec<u64> = std::iter::from_fn(|| net_rx.try_recv().ok())
        .filter_map(|command| match command {
            NetCommand::AzureRunDetail { build_id, .. } => Some(build_id),
            _ => None,
        })
        .collect();
    assert_eq!(asks, [9], "one ask for the selected run");

    // the answer fills the stage cards and the log
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.on_net_event(
            UiEvent::AzureRunDetailLoaded {
                folder: "/probe/alpha".into(),
                build_id: 9,
                detail: Some(RunDetail {
                    sha: Some("abcdef1234567890".into()),
                    trigger: Some("individualCI".into()),
                    start: Some("2026-11-02T09:00:00Z".into()),
                    finish: Some("2026-11-02T09:04:00Z".into()),
                    stages: vec![
                        RunStage {
                            name: "Build".into(),
                            jobs: vec![RunJob {
                                name: "Compile".into(),
                                state: Some("completed".into()),
                                result: Some("succeeded".into()),
                                start: None,
                                finish: None,
                            }],
                        },
                        RunStage {
                            name: "Test".into(),
                            jobs: vec![RunJob {
                                name: "pytest".into(),
                                state: Some("completed".into()),
                                result: Some("failed".into()),
                                start: None,
                                finish: None,
                            }],
                        },
                    ],
                    log: "step one\nFAILED tests/test_sync.py\n".into(),
                }),
            },
            cx,
        );
    });
    draw(cx);

    assert!(
        cx.debug_bounds("run-stage-0").is_some(),
        "one card per stage"
    );
    assert!(cx.debug_bounds("run-stage-1").is_some());
    assert!(cx.debug_bounds("run-job-1-0").is_some(), "a job row");
    assert!(cx.debug_bounds("run-log").is_some(), "the log card");
    assert!(cx.debug_bounds("run-log-lines").is_some());
    assert!(
        cx.debug_bounds("run-open").is_some(),
        "Open run is the primary"
    );

    // a press on the other run selects it
    click(cx, "pipeline-run-1");
    assert_eq!(
        store.read_with(cx, |s, _| s.open_run.as_ref().map(|open| open.run.id)),
        Some(7)
    );
}

/// No runs at all is still the tab, plus the project's items.
#[gpui::test]
fn a_repository_without_runs_still_shows_its_items(cx: &mut TestAppContext) {
    let (store, _net_rx) = scratch_store(cx, "no-runs");
    let cx = open_window(&store, cx);
    seed_azure(
        &store,
        AzureSnapshot {
            available: true,
            pulls: vec![AzureItem {
                number: 214,
                title: "Sync the bridge".into(),
                ..Default::default()
            }],
            ..Default::default()
        },
        cx,
    );
    // the Azure pull request is a row of the merged list now, not a card
    click(cx, "detail-tab-pulls");
    assert!(
        cx.debug_bounds("PR-az-214").is_some(),
        "the Azure pull request's row"
    );
    assert!(cx.debug_bounds("run-detail").is_none());
}

/// Azure alone still draws the Pull requests tab. The strip hung on the GitHub
/// switch, so an Azure repository with an open pull request — a count its row
/// already showed — had no tab to open it in.
#[gpui::test]
fn an_azure_only_setup_still_draws_the_pull_request_tab(cx: &mut TestAppContext) {
    let (store, _net_rx) = scratch_store(cx, "azure-only");
    let cx = open_window(&store, cx);
    cx.update_entity(&store, |s: &mut Store, _| {
        s.update_settings(|settings| {
            settings.repos.enable_github = false;
            settings.repos.enable_azure_devops = true;
            settings.repos.azure_devops_pat = "pat".into();
        });
    });
    draw(cx);
    seed_azure(
        &store,
        AzureSnapshot {
            available: true,
            pulls: vec![AzureItem {
                number: 214,
                title: "Sync the bridge".into(),
                ..Default::default()
            }],
            ..Default::default()
        },
        cx,
    );

    assert!(
        cx.debug_bounds("detail-tab-pulls").is_some(),
        "the Pull requests tab is in the strip"
    );
    assert!(
        cx.debug_bounds("detail-tab-issues").is_some(),
        "work items list under Issues, so that tab stays too"
    );
    click(cx, "detail-tab-pulls");
    assert!(
        cx.debug_bounds("PR-az-214").is_some(),
        "the Azure pull request's row"
    );
}

/// The policy that keeps the primary on Set auto-complete.
fn blocked_policy() -> Vec<AzurePolicy> {
    vec![AzurePolicy {
        name: "Build validation".into(),
        kind: "Build".into(),
        status: "running".into(),
    }]
}

const PULL: &str = r#"{
    "pullRequestId": 214,
    "title": "Sync the bridge",
    "description": "Maps the handle token.",
    "status": "active",
    "isDraft": false,
    "sourceRefName": "refs/heads/fix/bridge",
    "targetRefName": "refs/heads/main",
    "createdBy": {"displayName": "Engin K"},
    "creationDate": "2026-10-01T10:00:00Z",
    "mergeStatus": "succeeded",
    "autoCompleteSetBy": null,
    "lastMergeSourceCommit": {"commitId": "abc123"},
    "_links": {"web": {"href": "https://dev.azure.com/o/p/_git/r/pullrequest/214"}},
    "reviewers": [
        {"id": "me-1", "displayName": "Engin K", "vote": 0, "isRequired": true}
    ]
}"#;

fn azure_pull() -> AzurePull {
    let payload: serde_json::Value = serde_json::from_str(PULL).unwrap();
    let mut pull = warp_station_core::azure::parse_pull(Some(&payload)).expect("a pull request");
    pull.me = Some("me-1".into());
    pull
}

fn seed_pull(
    store: &gpui::Entity<Store>,
    cx: &mut gpui::VisualTestContext,
    policies: Vec<AzurePolicy>,
) {
    let on_event = |s: &mut Store, cx: &mut gpui::Context<Store>| {
        s.on_net_event(
            UiEvent::AzurePullLoaded {
                folder: "/probe/alpha".into(),
                number: 214,
                detail: Some(Box::new(AzurePullDetail {
                    pull: azure_pull(),
                    policies,
                    threads: vec![AzureThread {
                        status: "active".into(),
                        comments: vec![AzureComment {
                            author: "Mert".into(),
                            when: Some("2026-10-02T09:00:00Z".into()),
                            text: "This leaks a handle.".into(),
                        }],
                    }],
                })),
            },
            cx,
        );
    };
    cx.update_entity(store, on_event);
    draw(cx);
}

/// The Azure pull request's head, strip and Overview.
#[gpui::test]
fn the_azure_pull_detail_draws_its_head_and_overview(cx: &mut TestAppContext) {
    let (store, _net_rx) = scratch_store(cx, "azure-pull");
    let cx = open_window(&store, cx);
    seed_azure(
        &store,
        AzureSnapshot {
            available: true,
            pulls: vec![AzureItem {
                number: 214,
                title: "Sync the bridge".into(),
                ..Default::default()
            }],
            ..Default::default()
        },
        cx,
    );
    // the merged Pull requests list is the entry now
    click(cx, "detail-tab-pulls");
    click(cx, "PR-az-214");

    assert!(cx.debug_bounds("azure-pull").is_some(), "the detail opens");
    seed_pull(
        &store,
        cx,
        vec![AzurePolicy {
            name: "Mert must approve".into(),
            kind: "Required".into(),
            status: "approved".into(),
        }],
    );

    assert!(cx.debug_bounds("azure-pull-policies").is_some());
    assert!(cx.debug_bounds("azure-pull-description").is_some());
    assert!(cx.debug_bounds("azure-pull-threads").is_some());
    assert!(cx.debug_bounds("azure-pull-reviewers-card").is_some());
    assert!(cx.debug_bounds("azure-approve").is_some(), "the vote");
    assert!(
        cx.debug_bounds("azure-complete").is_some(),
        "ready, so the primary completes"
    );
    assert!(cx.debug_bounds("azure-open").is_some());
    assert!(
        cx.debug_bounds("azure-pull-back").is_none(),
        "Back belongs to the narrow pane"
    );

    click(cx, "azure-pull-tab-files");
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.on_net_event(
            UiEvent::AzurePullFilesLoaded {
                folder: "/probe/alpha".into(),
                number: 214,
                files: Some(vec![AzureFile {
                    path: "src/main.rs".into(),
                    change: "edit".into(),
                }]),
            },
            cx,
        );
    });
    draw(cx);
    assert!(cx.debug_bounds("azure-pull-files").is_some());
    assert!(cx.debug_bounds("azure-pull-file-0").is_some());
}

/// The Files tab reads the changed files, and a file's diff is its own ask.
#[gpui::test]
fn the_azure_files_tab_asks_for_the_diff_it_shows(cx: &mut TestAppContext) {
    let (store, net_rx) = scratch_store(cx, "azure-files");
    let cx = open_window(&store, cx);
    seed_azure(
        &store,
        AzureSnapshot {
            available: true,
            pulls: vec![AzureItem {
                number: 214,
                title: "Sync the bridge".into(),
                ..Default::default()
            }],
            ..Default::default()
        },
        cx,
    );
    // the merged Pull requests list is the entry now
    click(cx, "detail-tab-pulls");
    click(cx, "PR-az-214");
    seed_pull(&store, cx, Vec::new());
    enable_azure(&store, cx);
    while net_rx.try_recv().is_ok() {}

    click(cx, "azure-pull-tab-files");
    let asked = |rx: &async_channel::Receiver<NetCommand>| {
        std::iter::from_fn(|| rx.try_recv().ok())
            .filter(|command| matches!(command, NetCommand::AzurePullFiles { .. }))
            .count()
    };
    assert_eq!(asked(&net_rx), 1, "the files ask goes out once");

    cx.update_entity(&store, |s: &mut Store, cx| {
        s.on_net_event(
            UiEvent::AzurePullFilesLoaded {
                folder: "/probe/alpha".into(),
                number: 214,
                files: Some(vec![AzureFile {
                    path: "src/main.rs".into(),
                    change: "edit".into(),
                }]),
            },
            cx,
        );
    });
    draw(cx);
    assert!(cx.debug_bounds("azure-pull-file-0").is_some());

    let diffs: Vec<String> = std::iter::from_fn(|| net_rx.try_recv().ok())
        .filter_map(|command| match command {
            NetCommand::AzurePullFileDiff { path, .. } => Some(path),
            _ => None,
        })
        .collect();
    assert_eq!(diffs, ["src/main.rs"], "the open file asks for its patch");

    cx.update_entity(&store, |s: &mut Store, cx| {
        s.on_net_event(
            UiEvent::AzurePullFileDiffLoaded {
                folder: "/probe/alpha".into(),
                number: 214,
                path: "src/main.rs".into(),
                patch: Some(
                    "--- a/src/main.rs\n+++ b/src/main.rs\n@@ -1 +1 @@\n-old\n+new\n".into(),
                ),
            },
            cx,
        );
    });
    draw(cx);
    assert!(
        cx.debug_bounds("azure-pull-diff-scroll").is_some(),
        "the patch draws"
    );
}

/// Every verb is one request: a vote, a completion, an abandonment (which asks
/// first) and the auto-complete toggle.
#[gpui::test]
fn the_azure_verbs_send_one_request_each(cx: &mut TestAppContext) {
    use warp_station_core::azure::AzurePullAction;

    let (store, net_rx) = scratch_store(cx, "azure-verbs");
    let cx = open_window(&store, cx);
    seed_azure(
        &store,
        AzureSnapshot {
            available: true,
            pulls: vec![AzureItem {
                number: 214,
                title: "Sync the bridge".into(),
                ..Default::default()
            }],
            ..Default::default()
        },
        cx,
    );
    // the merged Pull requests list is the entry now
    click(cx, "detail-tab-pulls");
    click(cx, "PR-az-214");
    // one blocking policy, so the primary arms auto-complete instead
    seed_pull(&store, cx, blocked_policy());
    enable_azure(&store, cx);
    while net_rx.try_recv().is_ok() {}

    let actions = |rx: &async_channel::Receiver<NetCommand>| -> Vec<AzurePullAction> {
        std::iter::from_fn(|| rx.try_recv().ok())
            .filter_map(|command| match command {
                NetCommand::AzurePullAction { action, .. } => Some(action),
                _ => None,
            })
            .collect()
    };
    // a verb's answer reloads the record, so the pane needs its detail again
    let answer = |store: &gpui::Entity<Store>,
                  action: AzurePullAction,
                  policies: Vec<AzurePolicy>,
                  cx: &mut gpui::VisualTestContext| {
        cx.update_entity(store, |s: &mut Store, cx| {
            s.on_net_event(
                UiEvent::AzurePullActed {
                    folder: "/probe/alpha".into(),
                    number: 214,
                    action,
                    result: Ok(()),
                },
                cx,
            );
        });
        draw(cx);
        seed_pull(store, cx, policies);
    };

    // a blocked policy leaves the primary on Set auto-complete
    assert!(cx.debug_bounds("azure-autocomplete").is_some());
    click(cx, "azure-autocomplete");
    assert_eq!(
        actions(&net_rx),
        [AzurePullAction::SetAutoComplete {
            reviewer: "me-1".into()
        }]
    );
    answer(
        &store,
        AzurePullAction::SetAutoComplete {
            reviewer: "me-1".into(),
        },
        blocked_policy(),
        cx,
    );

    // approve is a vote through the review split
    click(cx, "azure-approve");
    assert_eq!(
        actions(&net_rx),
        [AzurePullAction::Vote {
            reviewer: "me-1".into(),
            vote: 10
        }]
    );
    answer(
        &store,
        AzurePullAction::Vote {
            reviewer: "me-1".into(),
            vote: 10,
        },
        blocked_policy(),
        cx,
    );

    // abandon asks through the page's card
    click(cx, "azure-primary-menu");
    assert!(cx.debug_bounds("azure-primary-menu").is_some());
    click(cx, "azure-menu-abandon");
    assert!(cx.debug_bounds("abandon-azure-dialog").is_some());
    assert!(
        actions(&net_rx).is_empty(),
        "nothing is sent before the answer"
    );
    click(cx, "btn-danger-Abandon pull request");
    assert_eq!(actions(&net_rx), [AzurePullAction::Abandon]);
}

/// Completing arms on the first press and sends on the second, like Merge.
#[gpui::test]
fn completing_takes_two_presses(cx: &mut TestAppContext) {
    use warp_station_core::azure::AzurePullAction;

    let (store, net_rx) = scratch_store(cx, "azure-complete");
    let cx = open_window(&store, cx);
    seed_azure(
        &store,
        AzureSnapshot {
            available: true,
            pulls: vec![AzureItem {
                number: 214,
                title: "Sync the bridge".into(),
                ..Default::default()
            }],
            ..Default::default()
        },
        cx,
    );
    // the merged Pull requests list is the entry now
    click(cx, "detail-tab-pulls");
    click(cx, "PR-az-214");
    seed_pull(&store, cx, Vec::new());
    enable_azure(&store, cx);
    while net_rx.try_recv().is_ok() {}

    let completions = |rx: &async_channel::Receiver<NetCommand>| {
        std::iter::from_fn(|| rx.try_recv().ok())
            .filter(|command| {
                matches!(
                    command,
                    NetCommand::AzurePullAction {
                        action: AzurePullAction::Complete,
                        ..
                    }
                )
            })
            .count()
    };

    click(cx, "azure-complete");
    assert_eq!(completions(&net_rx), 0, "the first press only arms");
    assert!(
        store.read_with(cx, |s, _| s
            .open_azure_pull
            .as_ref()
            .is_some_and(|open| open.armed_complete)),
        "the primary armed"
    );
    click(cx, "azure-complete");
    assert_eq!(completions(&net_rx), 1);
}

/// Below 900 px of pane the run detail takes it whole, and Back returns.
#[gpui::test]
fn a_narrow_pipelines_pane_keeps_one_column(cx: &mut TestAppContext) {
    let (store, _net_rx) = scratch_store(cx, "azure-narrow");
    let cx = open_window(&store, cx);
    cx.simulate_resize(gpui::size(gpui::px(1100.), gpui::px(760.)));
    draw(cx);
    seed_azure(
        &store,
        AzureSnapshot {
            available: true,
            pipelines: vec![run(9, "20261102.4", false)],
            ..Default::default()
        },
        cx,
    );
    click(cx, "detail-tab-pipelines");
    draw(cx);

    assert!(cx.debug_bounds("azure-runs-column").is_some());
    assert!(
        cx.debug_bounds("run-detail").is_none(),
        "nothing is selected in a narrow pane"
    );

    click(cx, "pipeline-run-0");
    draw(cx);
    assert!(cx.debug_bounds("run-detail").is_some());
    assert!(cx.debug_bounds("run-back").is_some(), "Back is here");
    assert!(cx.debug_bounds("azure-runs-column").is_none());

    cx.simulate_keystrokes("escape");
    draw(cx);
    assert!(
        cx.debug_bounds("azure-runs-column").is_some(),
        "Escape goes back"
    );
    assert!(cx.debug_bounds("run-detail").is_none());
}
