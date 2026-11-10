//! Headless draw probe for the Changes workspace: a real window over a store
//! loaded with the live repository's actual snapshot, branches and history —
//! two draws without a panic means the view's element tree is sound.

use gpui::prelude::*;
use gpui::{Modifiers, TestAppContext, VisualTestContext, point, px};
use warp_station::store::Store;
use warp_station::ui::window::MainWindow;

/// Serializes the HOME-swapping new-branch probe against its siblings.
static HOME_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Scratch settings for one probe, keyed by the test's name, the pid and a
/// per-call counter so parallel probes never share a file. Arrangement state
/// (selection, open tabs, the archive toggle) persists now, so an unisolated
/// store would read the developer's live arrangement — the first-row probe
/// flaked on exactly that — and write test choices into the real
/// ~/.warp/settings.json.
fn scratch_service(name: &str) -> std::sync::Arc<warp_station_core::settings::SettingsService> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!(
        "warp-station-probe-changes-{name}-{}-{seq}.json",
        std::process::id()
    ));
    std::fs::write(&path, "{}").expect("scratch settings");
    std::sync::Arc::new(warp_station_core::settings::SettingsService::at(path))
}

#[gpui_kit::gpui::test]
fn changes_view_draws_over_real_data(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let here = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
        .to_path_buf();

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| {
        let mut store = Store::with_settings(
            net_tx,
            scratch_service("changes-changes_view_draws_over_real_data"),
        );
        // prime the bottom bar with a REAL snapshot of this repository
        let snapshot = warp_station_core::git::read::change_snapshot(&here);
        let branches = warp_station_core::git::read::branches(&here).unwrap_or_default();
        let recent = warp_station_core::git::read::recent_commits(&here).unwrap_or_default();
        let unpushed = warp_station_core::git::read::unpushed_hashes(&here);
        store.replace_repos(vec![warp_station_core::repo_store::RepoEntry {
            name: "probe-repo".into(),
            folder_path: here.clone(),
            solution_path: None,
            tags: vec![],
            git_branch_name: Some("main".into()),
            git_modified_count: snapshot
                .as_ref()
                .map(|s| s.status_files.len() as u32)
                .unwrap_or(0),
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
        }]);
        store.select_repo(&here.to_string_lossy(), cx);
        store.set_changes(snapshot);
        store.changes_loaded_for = Some(here.to_string_lossy().to_string());
        store.branches = branches;
        store.recent_commits = recent;
        store.unpushed = unpushed;
        store
    });

    let (_window, cx) = cx.add_window_view(|window, cx| MainWindow::new(store.clone(), window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    // a second draw after an expand toggle would need input; two consecutive
    // draws still exercise the full tree including the rail and both sections
    cx.update(|window, cx| drop(window.draw(cx)));

    let loaded = cx.read_entity(&store, |s, _| s.changes_loaded_for.is_some());
    assert!(loaded, "changes should be loaded for the selected repo");
}

#[gpui_kit::gpui::test]
fn history_and_drill_down_draw(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let here = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
        .to_path_buf();

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| {
        let mut store = Store::with_settings(
            net_tx,
            scratch_service("changes-history_and_drill_down_draw"),
        );
        store.replace_repos(vec![warp_station_core::repo_store::RepoEntry {
            name: "probe-repo".into(),
            folder_path: here.clone(),
            solution_path: None,
            tags: vec![],
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
        }]);
        store.select_repo(&here.to_string_lossy(), cx);
        store.set_changes(warp_station_core::git::read::change_snapshot(&here));
        store.changes_loaded_for = Some(here.to_string_lossy().to_string());
        store.set_history(
            warp_station_core::git::read::history_commits(&here, 30).unwrap_or_default(),
        );
        store.unpushed = warp_station_core::git::read::unpushed_hashes(&here);
        store
    });
    assert!(
        cx.read_entity(&store, |s, _| s.history().len()) > 5,
        "probe repo should have history"
    );

    let (view_handle, cx) = cx.add_window_view(|window, cx| {
        warp_station::ui::changes::ChangesView::new(store.clone(), window, cx)
    });
    cx.run_until_parked();

    // list pane
    store.update(cx, |s: &mut Store, cx| {
        s.open_detail_tab(warp_station::store::WorkspaceTab::History, cx)
    });
    cx.run_until_parked();
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    // drill into the newest commit (details pre-seeded the way the net would)
    let head_hash = cx.read_entity(&store, |s, _| {
        s.history()
            .first()
            .map(|c| c.hash.clone())
            .unwrap_or_default()
    });
    let details = warp_station_core::git::read::commit_details(&here, &head_hash);
    view_handle.update(cx, |view, cx| view.drill_commit(&head_hash, cx));
    store.update(cx, |s: &mut Store, _| {
        s.commit_details = details;
    });
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    cx.update(|window, cx| drop(window.draw(cx)));
    assert!(
        cx.debug_bounds("commit-subject").is_some(),
        "the detail names the commit it shows"
    );
}

/// The Changes rail only offers verbs that can act right now: no fetch without
/// a remote, no pull with nothing behind, no push with nothing ahead, no
/// publish for a published branch — and no commit box until something is
/// staged (the commit prompt reads the index, so an unstaged-only tree has
/// nothing to commit).
#[gpui_kit::gpui::test]
fn git_verbs_and_commit_follow_the_repo_state(cx: &mut TestAppContext) {
    use warp_station_core::model::{
        GitBranchKind, GitBranchRef, GitChangeSnapshot, GitChangedFile,
    };
    use warp_station_core::repo_store::RepoEntry;

    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    fn file(path: &str) -> GitChangedFile {
        GitChangedFile {
            path: path.into(),
            status_code: "M".into(),
            additions: Some(1),
            deletions: None,
        }
    }
    fn snapshot(staged: bool, unstaged: bool) -> GitChangeSnapshot {
        let staged_files = if staged {
            vec![file("staged.txt")]
        } else {
            Vec::new()
        };
        let unstaged_files = if unstaged {
            vec![file("unstaged.txt")]
        } else {
            Vec::new()
        };
        let status_files = staged_files
            .iter()
            .chain(unstaged_files.iter())
            .cloned()
            .collect();
        GitChangeSnapshot {
            status_files,
            staged_files,
            unstaged_files,
            worktree_counts: Default::default(),
            cached_counts: Default::default(),
        }
    }

    // the receiver stays bound: the footer's Stage all must reach the net
    let (net_tx, net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| {
        let mut store = Store::with_settings(net_tx, scratch_service("changes-git-verbs-relevant"));
        store.replace_repos(vec![RepoEntry {
            name: "alpha".into(),
            folder_path: std::path::PathBuf::from("/probe/alpha"),
            solution_path: None,
            tags: Vec::new(),
            git_branch_name: Some("main".into()),
            git_modified_count: 1,
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
        }]);
        store.select_repo("/probe/alpha", cx);
        store.open_detail_tab(warp_station::store::WorkspaceTab::Changes, cx);
        store.changes_loaded_for = Some("/probe/alpha".into());
        store.branches = vec![GitBranchRef {
            name: "main".into(),
            kind: GitBranchKind::Local,
        }];
        store.set_changes(Some(snapshot(true, false)));
        store
    });
    let (_window, cx) = cx.add_window_view(|window, cx| MainWindow::new(store.clone(), window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    // published, in sync, one staged file: fetch and the commit box only
    assert!(
        cx.debug_bounds("rail-Fetch").is_some(),
        "a remote gets Fetch"
    );
    assert!(
        cx.debug_bounds("rail-Pull").is_none(),
        "nothing behind, no Pull"
    );
    assert!(
        cx.debug_bounds("rail-Push").is_none(),
        "nothing ahead, no Push"
    );
    assert!(
        cx.debug_bounds("rail-Publish").is_none(),
        "published, no Publish"
    );
    assert!(cx.debug_bounds("rail-Sync").is_none(), "in sync, no Sync");
    assert!(
        cx.debug_bounds("commit-message").is_some(),
        "staged: commit box"
    );
    assert!(
        cx.debug_bounds("stage-all-footer").is_none(),
        "a staged tree shows the commit box, not the stage footer"
    );
    assert!(
        cx.debug_bounds("stage-all-clear").is_none(),
        "the staged header's verb waits for hover"
    );
    hover(cx, "changes-section-staged");
    assert!(
        cx.debug_bounds("stage-all-clear").is_some(),
        "staged: the unstage-all verb is a named button"
    );
    assert!(
        cx.debug_bounds("discard-all-s").is_none(),
        "staged: discarding the whole index is not offered"
    );
    click(cx, "stage-all-clear");
    let mut unstaged_all = false;
    while let Ok(command) = net_rx.try_recv() {
        unstaged_all |= matches!(
            command,
            warp_station::net::NetCommand::Git {
                action: warp_station::net::GitAction::UnstageAll,
                ..
            }
        );
    }
    assert!(unstaged_all, "the header button unstages everything");

    // both sides present: the staged section sits above the unstaged one, so
    // the index reads first as the thing the commit box will take
    cx.update_entity(&store, |s: &mut Store, _| {
        s.set_changes(Some(snapshot(true, true)));
    });
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    let staged_header = cx
        .debug_bounds("changes-section-staged")
        .expect("the staged header draws");
    let unstaged_header = cx
        .debug_bounds("changes-section-unstaged")
        .expect("the unstaged header draws");
    assert!(
        staged_header.top() < unstaged_header.top(),
        "staged first: staged header {staged_header:?}, unstaged header {unstaged_header:?}"
    );

    cx.simulate_mouse_move(
        gpui::point(gpui::px(5.), gpui::px(5.)),
        None,
        Modifiers::default(),
    );
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    assert!(
        cx.debug_bounds("stage-all").is_none() && cx.debug_bounds("discard-all-u").is_none(),
        "the unstaged header's verbs wait for hover"
    );
    hover(cx, "changes-section-unstaged");
    let stage = cx.debug_bounds("stage-all").expect("the stage icon draws");
    let discard = cx
        .debug_bounds("discard-all-u")
        .expect("the discard icon draws");
    assert!(
        discard.origin.x < stage.origin.x,
        "discard sits left of stage: discard {discard:?}, stage {stage:?}"
    );
    // the row's hover verbs follow the same order
    for (staged, row_selector, path) in [
        (false, "u-unstaged.txt", "unstaged.txt"),
        (true, "s-staged.txt", "staged.txt"),
    ] {
        let row = cx
            .debug_bounds(row_selector)
            .unwrap_or_else(|| panic!("the {path} row draws"));
        cx.simulate_mouse_move(
            gpui::point(
                row.origin.x + row.size.width / 2.,
                row.origin.y + row.size.height / 2.,
            ),
            None,
            Modifiers::default(),
        );
        cx.run_until_parked();
        cx.update(|window, cx| drop(window.draw(cx)));
        let discard = cx
            .debug_bounds(if staged {
                "discard-true-staged.txt"
            } else {
                "discard-false-unstaged.txt"
            })
            .unwrap_or_else(|| panic!("the {path} row discard draws"));
        let toggle = cx
            .debug_bounds(if staged {
                "toggle-true-staged.txt"
            } else {
                "toggle-false-unstaged.txt"
            })
            .unwrap_or_else(|| panic!("the {path} row toggle draws"));
        assert!(
            discard.origin.x < toggle.origin.x,
            "{path}: discard sits left of the stage toggle: discard {discard:?}, toggle {toggle:?}"
        );
    }

    // a draft longer than the well scrolls inside it; it must not grow the
    // box past its row and paint under the Commit button
    cx.update_entity(&store, |s: &mut Store, _| {
        s.commit_message_draft = Some(
            "docs(agents): add suite-wide agent instructions and app guides\n\n\
             Add a root AGENTS.md as the entry point for agents working in the monorepo:\n\n\
             - read the suite guide first\n\
             - then the app guide\n\
             - keep the shared contracts in step\n\n\
             The app guides describe the store, the net thread and the UI."
                .into(),
        );
    });
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    let well = cx
        .debug_bounds("commit-message")
        .expect("the commit well draws");
    let button = cx
        .debug_bounds("commit-button")
        .expect("the commit button draws");
    assert!(
        well.bottom() <= button.top(),
        "a long draft stays above the button: well {well:?}, button {button:?}"
    );

    // both headers fold their rows; their verbs and the commit box stay
    assert!(cx.debug_bounds("u-unstaged.txt").is_some());
    click(cx, "changes-section-unstaged");
    assert!(
        cx.debug_bounds("u-unstaged.txt").is_none(),
        "the folded rows are gone"
    );
    hover(cx, "changes-section-unstaged");
    assert!(
        cx.debug_bounds("stage-all").is_some(),
        "the header verb survives the fold"
    );
    click(cx, "changes-section-unstaged");
    assert!(
        cx.debug_bounds("u-unstaged.txt").is_some(),
        "the rows come back"
    );

    click(cx, "changes-section-staged");
    assert!(
        cx.debug_bounds("s-staged.txt").is_none(),
        "the staged rows fold too"
    );
    assert!(
        cx.debug_bounds("commit-message").is_some(),
        "the commit box survives a folded staged section"
    );
    click(cx, "changes-section-staged");
    assert!(
        cx.debug_bounds("s-staged.txt").is_some(),
        "the staged rows come back"
    );

    // behind and ahead: Pull, Push and Sync appear
    cx.update_entity(&store, |s: &mut Store, _| {
        let entry = s.repos.iter_mut().find(|r| r.name == "alpha").unwrap();
        entry.git_to_pull_count = 2;
        entry.git_to_push_count = 3;
    });
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    assert!(cx.debug_bounds("rail-Pull").is_some());
    assert!(cx.debug_bounds("rail-Push").is_some());
    assert!(cx.debug_bounds("rail-Sync").is_some());

    // unstaged only: the commit box steps aside for the stage hint
    cx.update_entity(&store, |s: &mut Store, _| {
        s.set_changes(Some(snapshot(false, true)));
        let entry = s.repos.iter_mut().find(|r| r.name == "alpha").unwrap();
        entry.git_to_pull_count = 0;
        entry.git_to_push_count = 0;
    });
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    assert!(
        cx.debug_bounds("commit-message").is_none(),
        "nothing staged"
    );
    assert!(cx.debug_bounds("commit-button").is_none());
    assert!(cx.debug_bounds("changes-stage-hint").is_some());
    cx.simulate_mouse_move(
        gpui::point(gpui::px(5.), gpui::px(5.)),
        None,
        Modifiers::default(),
    );
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    assert!(
        cx.debug_bounds("discard-all-u").is_none(),
        "the unstaged header's verbs wait for hover"
    );
    hover(cx, "changes-section-unstaged");
    assert!(
        cx.debug_bounds("discard-all-u").is_some(),
        "unstaged still offers its discard-all"
    );
    assert!(
        cx.debug_bounds("stage-all").is_some(),
        "the unstaged header names its stage-all action"
    );
    // the trash arms first: one click must not throw the working tree away
    click(cx, "discard-all-u");
    assert!(
        !drain_discard(&net_rx),
        "one click only arms the discard-all"
    );
    assert!(
        cx.debug_bounds("discard-all-u").is_some(),
        "the armed button is still there to confirm"
    );
    click(cx, "discard-all-u");
    assert!(
        drain_discard(&net_rx),
        "the second click discards every unstaged file"
    );
    // a fresh snapshot disarms it again: the tree the arm was given for is gone
    click(cx, "discard-all-u");
    cx.update_entity(&store, |s: &mut Store, _| {
        s.set_changes(Some(snapshot(false, true)))
    });
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    hover(cx, "changes-section-unstaged");
    click(cx, "discard-all-u");
    assert!(
        !drain_discard(&net_rx),
        "a fresh snapshot disarms the discard-all"
    );
    click(cx, "discard-all-u");
    assert!(
        drain_discard(&net_rx),
        "the re-armed click still needs its confirmation"
    );
    assert!(
        cx.debug_bounds("stage-all-footer").is_some(),
        "the footer keeps the stage-all action"
    );
    click(cx, "stage-all-footer");
    let mut staged_all = false;
    while let Ok(command) = net_rx.try_recv() {
        staged_all |= matches!(
            command,
            warp_station::net::NetCommand::Git {
                action: warp_station::net::GitAction::StageAll,
                ..
            }
        );
    }
    assert!(staged_all, "the footer button stages everything");
    assert!(cx.debug_bounds("rail-Pull").is_none());
    assert!(cx.debug_bounds("rail-Push").is_none());
    assert!(cx.debug_bounds("rail-Sync").is_none());

    // an unpublished local branch publishes; it is not pushed
    cx.update_entity(&store, |s: &mut Store, _| {
        let entry = s.repos.iter_mut().find(|r| r.name == "alpha").unwrap();
        entry.git_branch_name = Some("feature".into());
        entry.git_branch_published = false;
        s.branches.push(GitBranchRef {
            name: "feature".into(),
            kind: GitBranchKind::Local,
        });
    });
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    assert!(
        cx.debug_bounds("rail-Publish").is_some(),
        "unpublished branch"
    );
    assert!(cx.debug_bounds("rail-Push").is_none());

    // a detached HEAD keeps Fetch but offers no branch verbs
    cx.update_entity(&store, |s: &mut Store, _| {
        let entry = s.repos.iter_mut().find(|r| r.name == "alpha").unwrap();
        entry.git_branch_name = Some("(detached)".into());
        entry.git_to_pull_count = 2;
        entry.git_branch_published = false;
    });
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    assert!(
        cx.debug_bounds("rail-Fetch").is_some(),
        "fetch stays useful"
    );
    assert!(cx.debug_bounds("rail-Pull").is_none());
    assert!(cx.debug_bounds("rail-Push").is_none());
    assert!(cx.debug_bounds("rail-Publish").is_none());
    assert!(cx.debug_bounds("rail-Sync").is_none());

    // no remote: even Fetch steps aside
    cx.update_entity(&store, |s: &mut Store, _| {
        let entry = s.repos.iter_mut().find(|r| r.name == "alpha").unwrap();
        entry.git_has_remote = false;
    });
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    assert!(cx.debug_bounds("rail-Fetch").is_none());
}

/// The commit wand can never leave "Generate & Commit" stuck: the progress
/// chip's Cancel stops the live run, a cancel is not an error, and a draft
/// that lands after a failed/cancelled run is not armed to commit itself.
#[gpui_kit::gpui::test]
fn a_failed_generation_disarms_generate_and_commit(cx: &mut TestAppContext) {
    use warp_station::net::{GitAction, NetCommand, UiEvent, WandKind};
    use warp_station_core::model::{GitChangeSnapshot, GitChangedFile};
    use warp_station_core::repo_store::RepoEntry;

    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    fn file(path: &str) -> GitChangedFile {
        GitChangedFile {
            path: path.into(),
            status_code: "M".into(),
            additions: Some(1),
            deletions: None,
        }
    }

    // `git` is a real executable on PATH, so the wand gate opens without
    // requiring a warp-agent install in the test environment
    let settings =
        std::env::temp_dir().join(format!("warp-station-probe-wand-{}", std::process::id()));
    std::fs::write(
        &settings,
        r#"{"WarpAgent":{"EnableWarpAgent":true,"Executable":"git"},"OpenCode":{"DefaultModel":"probe/model"}}"#,
    )
    .expect("scratch settings");

    let (net_tx, net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| {
        let mut store = Store::with_settings(
            net_tx,
            std::sync::Arc::new(warp_station_core::settings::SettingsService::at(
                settings.clone(),
            )),
        );
        store.replace_repos(vec![RepoEntry {
            name: "alpha".into(),
            folder_path: std::path::PathBuf::from("/probe/alpha"),
            solution_path: None,
            tags: Vec::new(),
            git_branch_name: Some("main".into()),
            git_modified_count: 1,
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
        }]);
        store.select_repo("/probe/alpha", cx);
        store.open_detail_tab(warp_station::store::WorkspaceTab::Changes, cx);
        store.changes_loaded_for = Some("/probe/alpha".into());
        let staged = vec![file("staged.txt")];
        store.set_changes(Some(GitChangeSnapshot {
            status_files: staged.clone(),
            staged_files: staged,
            unstaged_files: Vec::new(),
            worktree_counts: Default::default(),
            cached_counts: Default::default(),
        }));
        store
    });
    let (_window, cx) = cx.add_window_view(|window, cx| MainWindow::new(store.clone(), window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    // an empty box over a staged file starts the wand
    click(cx, "commit-button");
    let mut started = false;
    while let Ok(command) = net_rx.try_recv() {
        if matches!(command, NetCommand::Wand { .. }) {
            started = true;
        }
    }
    assert!(started, "the click starts a wand run");
    let run_id = cx
        .read_entity(&store, |s, _| s.wand_id.clone())
        .expect("the live run has an id");
    cx.read_entity(&store, |s, _| {
        assert_eq!(s.wand_busy, Some(WandKind::Commit))
    });

    // the progress chip offers Cancel, and it targets the live run
    assert!(
        cx.debug_bounds("wand-cancel").is_some(),
        "the working chip carries a Cancel"
    );
    assert!(
        cx.debug_bounds("activity-cancel").is_some(),
        "the title bar offers Cancel while a wand runs"
    );
    click(cx, "wand-cancel");
    let mut cancelled = None;
    while let Ok(command) = net_rx.try_recv() {
        if let NetCommand::CancelWand { id } = command {
            cancelled = Some(id);
        }
    }
    assert_eq!(
        cancelled.as_deref(),
        Some(run_id.as_str()),
        "Cancel stops exactly the live run"
    );
    // the title bar's stop is the one that stays reachable after a repo
    // switch; it targets the same run
    click(cx, "activity-cancel");
    let mut cancelled_again = None;
    while let Ok(command) = net_rx.try_recv() {
        if let NetCommand::CancelWand { id } = command {
            cancelled_again = Some(id);
        }
    }
    assert_eq!(cancelled_again.as_deref(), Some(run_id.as_str()));

    // the stop answers like any run: busy clears, and it is not an error
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.on_net_event(
            UiEvent::WandDone {
                wand_id: run_id.clone(),
                kind: WandKind::Commit,
                result: Err("cancelled".into()),
            },
            cx,
        );
    });
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    cx.read_entity(&store, |s, _| {
        assert!(s.wand_busy.is_none(), "cancelling clears busy");
        assert!(s.wand_error.is_none(), "a cancel is not a wand error");
    });

    // a genuine failure also clears busy (the button returns to Commit
    // instead of spinning forever)
    click(cx, "commit-button");
    while net_rx.try_recv().is_ok() {}
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.on_net_event(
            UiEvent::WandDone {
                wand_id: "w".into(),
                kind: WandKind::Commit,
                result: Err("the model returned no text".into()),
            },
            cx,
        );
    });
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    cx.read_entity(&store, |s, _| {
        assert!(
            s.wand_busy.is_none(),
            "the failed run cleared the busy flag"
        )
    });
    assert!(
        cx.debug_bounds("commit-button").is_some(),
        "the commit control is back"
    );

    // a late draft is just text: the failed click was disarmed
    cx.update_entity(&store, |s: &mut Store, _| {
        s.commit_message_draft = Some("late draft".into());
    });
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    assert!(
        git_commands(&net_rx).is_empty(),
        "no commit fires from a failed generation"
    );

    // a successful Generate & Commit commits by itself: the store owns the
    // follow-up, not this view
    cx.update_entity(&store, |s: &mut Store, _| s.commit_message_clear = true);
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    click(cx, "commit-button");
    while net_rx.try_recv().is_ok() {}
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.on_net_event(
            UiEvent::WandDone {
                wand_id: "w".into(),
                kind: WandKind::Commit,
                result: Ok("generated".into()),
            },
            cx,
        );
    });
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    let committed: Vec<String> = git_commands(&net_rx)
        .into_iter()
        .filter_map(|command| match command {
            NetCommand::Git {
                action: GitAction::Commit(message),
                ..
            } => Some(message),
            _ => None,
        })
        .collect();
    assert_eq!(committed, vec!["generated".to_string()]);
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let b = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} missing from the frame"));
    cx.simulate_click(
        gpui::Point {
            x: b.origin.x + b.size.width / 2.,
            y: b.origin.y + b.size.height / 2.,
        },
        Modifiers::default(),
    );
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
}

/// Parks the pointer on an element so its group-hover children (the section
/// headers' verbs, a row's actions) are laid out and measurable.
fn hover(cx: &mut VisualTestContext, selector: &'static str) {
    let b = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} missing from the frame"));
    cx.simulate_mouse_move(
        gpui::point(
            b.origin.x + b.size.width / 2.,
            b.origin.y + b.size.height / 2.,
        ),
        None,
        Modifiers::default(),
    );
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
}

fn git_commands(
    net_rx: &async_channel::Receiver<warp_station::net::NetCommand>,
) -> Vec<warp_station::net::NetCommand> {
    let mut git = Vec::new();
    while let Ok(cmd) = net_rx.try_recv() {
        if matches!(cmd, warp_station::net::NetCommand::Git { .. }) {
            git.push(cmd);
        }
    }
    git
}

/// Drains the channel, reporting whether it carried a whole-tree discard.
fn drain_discard(net_rx: &async_channel::Receiver<warp_station::net::NetCommand>) -> bool {
    let mut found = false;
    while let Ok(command) = net_rx.try_recv() {
        found |= matches!(
            command,
            warp_station::net::NetCommand::Git {
                action: warp_station::net::GitAction::DiscardUnstaged(_),
                ..
            }
        );
    }
    found
}

#[gpui_kit::gpui::test]
fn branch_delete_gates_on_kind_current_and_two_clicks(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    // the receiver stays bound: the confirmed delete must arrive on it
    let (net_tx, net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| {
        let mut store = Store::with_settings(
            net_tx,
            scratch_service("changes-branch_delete_gates_on_kind_current_and_two_clicks"),
        );
        store.replace_repos(vec![warp_station_core::repo_store::RepoEntry {
            name: "probe-repo".into(),
            folder_path: "/probe/alpha".into(),
            solution_path: None,
            tags: vec![],
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
        }]);
        store.select_repo("/probe/alpha", cx);
        store.open_detail_tab(warp_station::store::WorkspaceTab::Changes, cx);
        store.changes_loaded_for = Some("/probe/alpha".into());
        store.branches = vec![
            warp_station_core::model::GitBranchRef {
                name: "main".into(),
                kind: warp_station_core::model::GitBranchKind::Local,
            },
            warp_station_core::model::GitBranchRef {
                name: "feature/x".into(),
                kind: warp_station_core::model::GitBranchKind::Local,
            },
            warp_station_core::model::GitBranchRef {
                name: "feature/keep".into(),
                kind: warp_station_core::model::GitBranchKind::Local,
            },
            warp_station_core::model::GitBranchRef {
                name: "origin/feature/x".into(),
                kind: warp_station_core::model::GitBranchKind::Remote,
            },
        ];
        store
    });

    let (_window, cx) = cx.add_window_view(|window, cx| MainWindow::new(store.clone(), window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    click(cx, "branch-button");
    assert!(
        cx.debug_bounds("branch-menu").is_some(),
        "the branch menu opens"
    );

    // the trash is offered on deletable locals only
    assert!(
        cx.debug_bounds("branch-delete-main").is_none(),
        "never on the checked-out branch"
    );
    assert!(
        cx.debug_bounds("branch-delete-origin/feature/x").is_none(),
        "never on remote rows"
    );
    assert!(cx.debug_bounds("branch-delete-feature/x").is_some());
    assert!(cx.debug_bounds("branch-delete-feature/keep").is_some());

    // first click arms: the menu stays open and nothing runs
    click(cx, "branch-delete-feature/x");
    assert!(
        cx.debug_bounds("branch-menu").is_some(),
        "arming keeps the menu open"
    );
    assert!(
        git_commands(&net_rx).is_empty(),
        "the arm click must not run git"
    );

    // a press elsewhere in the tab closes the menu and disarms — the reopened
    // menu starts from a single-click arm again
    let sheet = cx
        .debug_bounds("changes-clean")
        .expect("the clean working tree renders");
    cx.simulate_click(sheet.origin + point(px(4.), px(20.)), Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    assert!(
        cx.debug_bounds("branch-menu").is_none(),
        "the outside press closed the menu"
    );
    click(cx, "branch-button");
    assert!(cx.debug_bounds("branch-menu").is_some(), "the menu reopens");
    click(cx, "branch-delete-feature/x");
    assert!(
        git_commands(&net_rx).is_empty(),
        "reopening disarmed the trash"
    );

    // arm, then confirm: exactly one delete reaches the net, never a checkout
    click(cx, "branch-delete-feature/x");
    assert!(
        cx.debug_bounds("branch-menu").is_none(),
        "confirming closes the menu"
    );
    let sent = git_commands(&net_rx);
    assert_eq!(sent.len(), 1, "one git command — the delete");
    let warp_station::net::NetCommand::Git { folder, action } = &sent[0] else {
        unreachable!("git_commands only collects Git commands");
    };
    assert_eq!(folder, std::path::Path::new("/probe/alpha"));
    assert!(
        matches!(action, warp_station::net::GitAction::DeleteBranch(name) if name == "feature/x"),
        "the delete fires, not a checkout: {action:?}"
    );
    assert!(
        cx.read_entity(&store, |s, _| s.git_busy()),
        "run_git marked the rail busy"
    );
}

/// The branch picker's new-branch form: it replaces the rows list, Cancel
/// returns to the list without sending anything, and Create sends exactly one
/// CreateBranch (checked out) and closes the dropdown. Runs under a scratch
/// HOME so the live settings' branch-name prefix never leaks into the name.
#[gpui_kit::gpui::test]
fn branch_new_branch_form_creates_and_closes(cx: &mut TestAppContext) {
    let _home_guard = HOME_ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let home = std::env::temp_dir().join(format!(
        "warp-station-new-branch-probe-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join(".warp")).unwrap();
    // SAFETY: process-global env; serialized against the siblings by HOME_ENV_LOCK.
    // USERPROFILE too on Windows: the data root resolves from it first, and a
    // HOME-only scratch leaves the real ~/.warp live for a save.
    unsafe {
        std::env::set_var("HOME", &home);
        #[cfg(windows)]
        std::env::set_var("USERPROFILE", &home);
        std::env::set_var("XDG_CONFIG_HOME", home.join(".config"));
    }
    let scratch = home.join(".warp").join("settings.json");
    // the settings default bakes in a personal branch-name prefix; the probe
    // pins an empty one so the typed name is the whole branch name
    std::fs::write(&scratch, r#"{"Repos": {"BranchNamePrefix": ""}}"#).expect("scratch settings");

    // the receiver stays bound: only the confirmed create must arrive on it
    let (net_tx, net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| {
        let mut store = Store::with_settings(
            net_tx,
            std::sync::Arc::new(warp_station_core::settings::SettingsService::at(
                scratch.clone(),
            )),
        );
        store.replace_repos(vec![warp_station_core::repo_store::RepoEntry {
            name: "probe-repo".into(),
            folder_path: "/probe/alpha".into(),
            solution_path: None,
            tags: vec![],
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
        }]);
        store.select_repo("/probe/alpha", cx);
        store.open_detail_tab(warp_station::store::WorkspaceTab::Changes, cx);
        store.changes_loaded_for = Some("/probe/alpha".into());
        store.branches = vec![warp_station_core::model::GitBranchRef {
            name: "main".into(),
            kind: warp_station_core::model::GitBranchKind::Local,
        }];
        store
    });

    let (_window, cx) = cx.add_window_view(|window, cx| MainWindow::new(store.clone(), window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    click(cx, "branch-button");
    assert!(
        cx.debug_bounds("branch-row-main").is_some(),
        "the list renders"
    );

    // the form swaps in for the list, seeded with the settings prefix (empty
    // here) and focused; nothing is sent by opening it
    click(cx, "new-branch-row");
    assert!(
        cx.debug_bounds("new-branch-name").is_some(),
        "the form renders"
    );
    assert!(
        cx.debug_bounds("branch-row-main").is_none(),
        "the form replaces the rows list"
    );
    assert!(
        git_commands(&net_rx).is_empty(),
        "opening the form sends nothing"
    );

    // Cancel returns to the list, still without sending anything
    click(cx, "new-branch-cancel");
    assert!(
        cx.debug_bounds("branch-row-main").is_some(),
        "cancel restores the list"
    );
    assert!(git_commands(&net_rx).is_empty(), "cancel sends nothing");

    // reopen: the form field is focused, so typing lands straight in it
    click(cx, "new-branch-row");
    cx.simulate_input("feat/probe");
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    click(cx, "new-branch-create");
    let sent = git_commands(&net_rx);
    assert_eq!(sent.len(), 1, "one git command — the create");
    let warp_station::net::NetCommand::Git { folder, action } = &sent[0] else {
        unreachable!("git_commands only collects Git commands");
    };
    assert_eq!(folder, std::path::Path::new("/probe/alpha"));
    assert!(
        matches!(
            action,
            warp_station::net::GitAction::CreateBranch { name, checkout: true, .. } if name == "feat/probe"
        ),
        "the create fires checked out: {action:?}"
    );
    assert!(
        cx.debug_bounds("branch-menu").is_none(),
        "creating closes the dropdown"
    );

    let _ = std::fs::remove_dir_all(&home);
}

/// The History list is virtualized: the store keeps 200 commits and every row
/// carries an avatar, a subject, an unpushed mark and a copy-SHA chip with its
/// own listener, so building all of them per repaint was the tab's cost. A
/// tall list in a short window lays out a screenful.
#[gpui_kit::gpui::test]
fn the_history_list_lays_out_only_its_visible_rows(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let here = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
        .to_path_buf();

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| {
        let mut store = Store::with_settings(
            net_tx,
            scratch_service("changes-the_history_list_lays_out_only_its_visible_rows"),
        );
        store.replace_repos(vec![warp_station_core::repo_store::RepoEntry {
            name: "probe-repo".into(),
            folder_path: here.clone(),
            solution_path: None,
            tags: vec![],
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
        }]);
        store.select_repo(&here.to_string_lossy(), cx);
        store.set_changes(warp_station_core::git::read::change_snapshot(&here));
        store.changes_loaded_for = Some(here.to_string_lossy().to_string());
        store.set_history(
            warp_station_core::git::read::history_commits(&here, 200).unwrap_or_default(),
        );
        store.unpushed = warp_station_core::git::read::unpushed_hashes(&here);
        store
    });
    let commits = cx.read_entity(&store, |s, _| s.history().len());
    assert!(
        commits > 60,
        "the probe repo needs a history longer than a screenful ({commits})",
    );

    let store_handle = store.clone();
    let (_window_handle, cx) =
        cx.add_window_view(|window, cx| MainWindow::new(store_handle.clone(), window, cx));
    cx.run_until_parked();
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.select_repo(&here.to_string_lossy(), cx);
        s.open_repo_tab(&here.to_string_lossy(), cx);
        s.open_detail_tab(warp_station::store::WorkspaceTab::History, cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    assert!(
        cx.debug_bounds("history-list").is_some(),
        "the history list renders",
    );
    // count the commit rows actually in the frame: each row carries its own
    // selector, so what the frame holds is what the list built
    let laid_out = commits;
    let mut built = 0usize;
    for commit in cx.read_entity(&store, |s, _| {
        s.history()
            .iter()
            .map(|c| c.short_hash().to_string())
            .collect::<Vec<_>>()
    }) {
        let selector: &'static str = Box::leak(format!("commit-{commit}").into_boxed_str());
        if cx.debug_bounds(selector).is_some() {
            built += 1;
        }
    }
    assert!(built > 0, "the top of the history is on screen",);
    assert!(
        built < laid_out,
        "a windowed list builds a screenful, not all {laid_out} commits ({built})",
    );
}

/// A request waiting behind another repo's run renders as queued on its own
/// repo, and its Cancel drops just that entry: the live run keeps the slot.
#[gpui_kit::gpui::test]
fn a_queued_generation_offers_its_own_cancel(cx: &mut TestAppContext) {
    use warp_station::net::{NetCommand, WandKind};
    use warp_station::store::WandRequest;
    use warp_station_core::model::{GitChangeSnapshot, GitChangedFile};
    use warp_station_core::repo_store::RepoEntry;

    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    fn repo(name: &str, folder: &str) -> RepoEntry {
        RepoEntry {
            name: name.into(),
            folder_path: std::path::PathBuf::from(folder),
            solution_path: None,
            tags: Vec::new(),
            git_branch_name: Some("main".into()),
            git_modified_count: 1,
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

    let settings =
        std::env::temp_dir().join(format!("warp-station-probe-queue-{}", std::process::id()));
    std::fs::write(
        &settings,
        r#"{"WarpAgent":{"EnableWarpAgent":true,"Executable":"git"},"OpenCode":{"DefaultModel":"probe/model"}}"#,
    )
    .expect("scratch settings");

    let (net_tx, net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| {
        let mut store = Store::with_settings(
            net_tx,
            std::sync::Arc::new(warp_station_core::settings::SettingsService::at(
                settings.clone(),
            )),
        );
        store.replace_repos(vec![
            repo("alpha", "/probe/alpha"),
            repo("beta", "/probe/beta"),
            repo("gamma", "/probe/gamma"),
        ]);
        store.select_repo("/probe/alpha", cx);
        store.open_detail_tab(warp_station::store::WorkspaceTab::Changes, cx);
        store.changes_loaded_for = Some("/probe/alpha".into());
        let staged = vec![GitChangedFile {
            path: "staged.txt".into(),
            status_code: "M".into(),
            additions: Some(1),
            deletions: None,
        }];
        store.set_changes(Some(GitChangeSnapshot {
            status_files: staged.clone(),
            staged_files: staged,
            unstaged_files: Vec::new(),
            worktree_counts: Default::default(),
            cached_counts: Default::default(),
        }));
        // beta's run owns the slot; alpha's request is waiting for it
        store.wand_busy = Some(WandKind::Commit);
        store.wand_id = Some("wand-live".into());
        store.wand_repo = Some("/probe/beta".into());
        store.wand_queue.push_back(WandRequest {
            kind: WandKind::Commit,
            folder: "/probe/alpha".into(),
            auto_commit: false,
        });
        store.wand_queue.push_back(WandRequest {
            kind: WandKind::Commit,
            folder: "/probe/gamma".into(),
            auto_commit: false,
        });
        store
    });
    let (_window, cx) = cx.add_window_view(|window, cx| MainWindow::new(store.clone(), window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    assert!(cx.debug_bounds("commit-button").is_some());
    assert!(
        cx.debug_bounds("wand-dequeue").is_some(),
        "the queued chip renders on the waiting repo"
    );
    assert!(
        cx.debug_bounds("wand-queue-toggle").is_some(),
        "the title bar counts the queue"
    );

    // the panel lists the queue in run order: alpha is next, gamma after it
    click(cx, "wand-queue-toggle");
    assert!(
        cx.debug_bounds("wand-queue-panel").is_some(),
        "the toggle opens the panel"
    );
    let first = cx
        .debug_bounds("wand-queue-row-0")
        .expect("the first queue row");
    let second = cx
        .debug_bounds("wand-queue-row-1")
        .expect("the second queue row");
    assert!(
        first.origin.y < second.origin.y,
        "rows follow the order the runs will take"
    );

    // the panel's own Cancel drops the row it names, not the live run
    click(cx, "wand-queue-cancel-1");
    cx.read_entity(&store, |s, _| {
        assert_eq!(s.wand_queue.len(), 1);
        assert!(s.wand_queued(WandKind::Commit, "/probe/alpha"));
        assert!(!s.wand_queued(WandKind::Commit, "/probe/gamma"));
        assert!(s.wand_busy.is_some(), "the live run keeps the slot");
    });
    assert!(
        cx.debug_bounds("wand-queue-row-1").is_none(),
        "gamma's row left"
    );

    click(cx, "wand-dequeue");
    cx.read_entity(&store, |s, _| {
        assert!(s.wand_queue.is_empty(), "the request left the queue");
        assert!(s.wand_busy.is_some(), "the live run keeps the slot");
        assert_eq!(s.wand_repo.as_deref(), Some("/probe/beta"));
        assert!(!s.wand_queued(WandKind::Commit, "/probe/alpha"));
    });
    assert!(
        cx.debug_bounds("wand-queue-panel").is_none(),
        "an empty queue closes the panel"
    );
    assert!(cx.debug_bounds("wand-queue-toggle").is_none());
    let mut cancelled = 0;
    while let Ok(command) = net_rx.try_recv() {
        if matches!(command, NetCommand::CancelWand { .. }) {
            cancelled += 1;
        }
    }
    assert_eq!(cancelled, 0, "dequeue never stops the live run");

    // the title bar's stop is the broad one: it drops the queue and stops
    // the run that owns the slot
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.wand_queue.push_back(WandRequest {
            kind: WandKind::Commit,
            folder: "/probe/gamma".into(),
            auto_commit: false,
        });
        s.cancel_all_wands(cx);
    });
    cx.read_entity(&store, |s, _| {
        assert!(s.wand_queue.is_empty(), "the queue was cleared");
    });
    let mut cancelled = 0;
    while let Ok(command) = net_rx.try_recv() {
        if matches!(command, NetCommand::CancelWand { .. }) {
            cancelled += 1;
        }
    }
    assert_eq!(cancelled, 1, "the live run was stopped too");
}
