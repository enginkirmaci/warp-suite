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
        let mut store =
            Store::with_settings(net_tx, scratch_service("changes-changes_view_draws_over_real_data"));
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
        let mut store =
            Store::with_settings(net_tx, scratch_service("changes-history_and_drill_down_draw"));
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

#[gpui_kit::gpui::test]
fn branch_delete_gates_on_kind_current_and_two_clicks(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    // the receiver stays bound: the confirmed delete must arrive on it
    let (net_tx, net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| {
        let mut store =
            Store::with_settings(net_tx, scratch_service("changes-branch_delete_gates_on_kind_current_and_two_clicks"));
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
        let mut store =
            Store::with_settings(net_tx, scratch_service("changes-the_history_list_lays_out_only_its_visible_rows"));
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
