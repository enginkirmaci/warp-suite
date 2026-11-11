//! The repo list's row keys follow the focused surface: the window delegates
//! while it owns the focus, the list takes over when clicked, and a field
//! takes them away again.

use gpui::TestAppContext;
use gpui::prelude::*;
use std::path::PathBuf;
use warp_station::net::NetCommand;
use warp_station::store::Store;
use warp_station::ui::window::MainWindow;
use warp_station_core::github::{GitHubItem, GitHubSnapshot};
use warp_station_core::model::{GitChangeSnapshot, GitChangedFile, GitCommitInfo};
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

fn scratch_service(name: &str) -> std::sync::Arc<warp_station_core::settings::SettingsService> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!(
        "warp-station-probe-list-keys-{name}-{}-{seq}.json",
        std::process::id()
    ));
    std::fs::write(&path, "{}").expect("scratch settings");
    std::sync::Arc::new(warp_station_core::settings::SettingsService::at(path))
}

fn draw(cx: &mut gpui::VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
}

fn changes(paths: &[&str]) -> GitChangeSnapshot {
    let unstaged: Vec<GitChangedFile> = paths
        .iter()
        .map(|path| GitChangedFile {
            path: (*path).to_string(),
            status_code: "M".into(),
            additions: Some(1),
            deletions: Some(0),
        })
        .collect();
    GitChangeSnapshot {
        status_files: unstaged.clone(),
        staged_files: Vec::new(),
        unstaged_files: unstaged,
        worktree_counts: Default::default(),
        cached_counts: Default::default(),
    }
}

fn pull(number: u64) -> GitHubItem {
    GitHubItem {
        number,
        title: format!("PR {number}"),
        url: format!("https://example.test/pull/{number}"),
        author: Some("Probe Author".into()),
        labels: Vec::new(),
        is_draft: false,
        review_decision: None,
        updated_at: None,
        created_at: None,
        comments: 0,
        head_ref_name: Some("feature".into()),
        base_ref_name: Some("main".into()),
        state: Some("OPEN".into()),
        is_pull_request: true,
    }
}

fn issue(number: u64) -> GitHubItem {
    GitHubItem {
        number,
        title: format!("Issue {number}"),
        url: format!("https://example.test/pull/{number}"),
        author: Some("Probe Author".into()),
        labels: Vec::new(),
        is_draft: false,
        review_decision: None,
        updated_at: None,
        created_at: None,
        comments: 0,
        head_ref_name: Some("feature".into()),
        base_ref_name: Some("main".into()),
        state: Some("OPEN".into()),
        is_pull_request: false,
    }
}

fn click(cx: &mut gpui::VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} renders"));
    cx.simulate_click(bounds.center(), gpui::Modifiers::default());
    draw(cx);
}

fn selection(cx: &gpui::VisualTestContext, store: &gpui::Entity<Store>) -> Option<String> {
    cx.read_entity(store, |store, _| store.selected_repo.clone())
}

#[gpui::test]
fn row_keys_follow_the_focused_list(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| {
        let mut store =
            Store::with_settings(net_tx, scratch_service("row_keys_follow_the_focused_list"));
        store.group_by_root = false;
        store.replace_repos(vec![
            entry("alpha", "/probe/alpha"),
            entry("beta", "/probe/beta"),
            entry("gamma", "/probe/gamma"),
        ]);
        store
    });
    let handle = store.clone();
    let (_window, cx) = cx.add_window_view(|window, cx| MainWindow::new(handle, window, cx));
    draw(cx);

    // the window owns the focus first: the delegation still steps the list
    cx.simulate_keystrokes("down");
    cx.run_until_parked();
    assert_eq!(
        selection(cx, &store).as_deref(),
        Some("/probe/beta"),
        "the window delegates ↓ to the repo list"
    );

    // a press on a row gives the list the keys
    click(cx, "repo-row-0");
    assert_eq!(
        selection(cx, &store).as_deref(),
        Some("/probe/alpha"),
        "the click selects the row"
    );
    cx.simulate_keystrokes("down");
    cx.run_until_parked();
    assert_eq!(
        selection(cx, &store).as_deref(),
        Some("/probe/beta"),
        "the focused list takes ↓"
    );

    // Ctrl+K focuses the search field; the list must not move under it
    cx.simulate_keystrokes("ctrl-k");
    draw(cx);
    let before = selection(cx, &store);
    cx.simulate_keystrokes("down");
    cx.run_until_parked();
    assert_eq!(
        selection(cx, &store),
        before,
        "↓ in the search field does not move the repo list"
    );
}

#[gpui::test]
fn home_end_and_page_keys_step_the_list(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| {
        let mut store = Store::with_settings(
            net_tx,
            scratch_service("home_end_and_page_keys_step_the_list"),
        );
        store.group_by_root = false;
        store.replace_repos(vec![
            entry("alpha", "/probe/alpha"),
            entry("beta", "/probe/beta"),
            entry("gamma", "/probe/gamma"),
        ]);
        store
    });
    let handle = store.clone();
    let (_window, cx) = cx.add_window_view(|window, cx| MainWindow::new(handle, window, cx));
    draw(cx);
    click(cx, "repo-row-0");

    cx.simulate_keystrokes("end");
    cx.run_until_parked();
    assert_eq!(
        selection(cx, &store).as_deref(),
        Some("/probe/gamma"),
        "End goes to the last row"
    );

    cx.simulate_keystrokes("home");
    cx.run_until_parked();
    assert_eq!(
        selection(cx, &store).as_deref(),
        Some("/probe/alpha"),
        "Home goes to the first row"
    );

    cx.simulate_keystrokes("pagedown");
    cx.run_until_parked();
    assert_eq!(
        selection(cx, &store).as_deref(),
        Some("/probe/gamma"),
        "PageDown clamps at the last row"
    );
}

#[gpui::test]
fn file_keys_follow_the_focused_changes_list(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let (net_tx, net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| {
        let mut store = Store::with_settings(
            net_tx,
            scratch_service("file_keys_follow_the_focused_changes_list"),
        );
        store.group_by_root = false;
        store.replace_repos(vec![
            entry("alpha", "/probe/alpha"),
            entry("beta", "/probe/beta"),
        ]);
        store.select_repo("/probe/alpha", cx);
        store.open_detail_tab(warp_station::store::WorkspaceTab::Changes, cx);
        store.changes_loaded_for = Some("/probe/alpha".into());
        store.set_changes(Some(changes(&["a.txt", "b.txt", "c.txt"])));
        store
    });
    let handle = store.clone();
    let (_window, cx) = cx.add_window_view(|window, cx| MainWindow::new(handle, window, cx));
    draw(cx);

    click(cx, "u-a.txt");
    // the click may already have asked for a's patch; the step's request is
    // what this test reads
    while net_rx.try_recv().is_ok() {}

    cx.simulate_keystrokes("down");
    cx.run_until_parked();
    let mut asked = None;
    while let Ok(command) = net_rx.try_recv() {
        if let NetCommand::GetChangePatch { path, staged, .. } = command {
            asked = Some((path, staged));
        }
    }
    assert_eq!(
        asked,
        Some(("b.txt".to_string(), false)),
        "↓ asks for the next file's patch"
    );
    assert_eq!(
        selection(cx, &store).as_deref(),
        Some("/probe/alpha"),
        "the focused file list does not move the repo list"
    );
}

#[gpui::test]
fn history_keys_follow_the_focused_commit_list(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let (net_tx, net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| {
        let mut store = Store::with_settings(
            net_tx,
            scratch_service("history_keys_follow_the_focused_commit_list"),
        );
        store.group_by_root = false;
        store.replace_repos(vec![
            entry("alpha", "/probe/alpha"),
            entry("beta", "/probe/beta"),
        ]);
        store.select_repo("/probe/alpha", cx);
        store.set_history(vec![
            GitCommitInfo {
                hash: "aaaaaaa".into(),
                subject: "first".into(),
                author: Some("Probe Author".into()),
                date: chrono::Local::now(),
            },
            GitCommitInfo {
                hash: "bbbbbbb".into(),
                subject: "second".into(),
                author: Some("Probe Author".into()),
                date: chrono::Local::now(),
            },
            GitCommitInfo {
                hash: "ccccccc".into(),
                subject: "third".into(),
                author: Some("Probe Author".into()),
                date: chrono::Local::now(),
            },
        ]);
        store.open_detail_tab(warp_station::store::WorkspaceTab::History, cx);
        store
    });
    let handle = store.clone();
    let (_window, cx) = cx.add_window_view(|window, cx| MainWindow::new(handle, window, cx));
    draw(cx);

    click(cx, "commit-aaaaaaa");
    while net_rx.try_recv().is_ok() {}

    cx.simulate_keystrokes("down");
    cx.run_until_parked();
    let mut asked = None;
    while let Ok(command) = net_rx.try_recv() {
        if let NetCommand::GetCommitDetails { hash, .. } = command {
            asked = Some(hash);
        }
    }
    assert_eq!(
        asked.as_deref(),
        Some("bbbbbbb"),
        "↓ drills the next commit"
    );
    assert_eq!(
        selection(cx, &store).as_deref(),
        Some("/probe/alpha"),
        "the focused commit list does not move the repo list"
    );
}

#[gpui::test]
fn pull_request_keys_follow_the_focused_list(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| {
        let mut store = Store::with_settings(
            net_tx,
            scratch_service("pull_request_keys_follow_the_focused_list"),
        );
        store.group_by_root = false;
        store.pane_width = 1400.;
        store.replace_repos(vec![
            entry("alpha", "/probe/alpha"),
            entry("beta", "/probe/beta"),
        ]);
        store.github_by_folder.insert(
            "/probe/alpha".into(),
            std::sync::Arc::new(GitHubSnapshot {
                url: Some("https://github.com/probe/alpha".into()),
                pulls: vec![pull(1), pull(2), pull(3)],
                issues: Vec::new(),
                runs: Vec::new(),
                available: true,
            }),
        );
        store.select_repo("/probe/alpha", cx);
        store.open_detail_tab(warp_station::store::WorkspaceTab::PullRequests, cx);
        store
    });
    let handle = store.clone();
    let (_window, cx) = cx.add_window_view(|window, cx| MainWindow::new(handle, window, cx));
    draw(cx);

    click(cx, "PR-gh-1");
    cx.simulate_keystrokes("down");
    cx.run_until_parked();
    let open = cx.read_entity(&store, |store, _| {
        store.open_pull.as_ref().map(|open| open.item.number)
    });
    assert_eq!(open, Some(2), "↓ opens the next pull request");
    assert_eq!(
        selection(cx, &store).as_deref(),
        Some("/probe/alpha"),
        "the focused pull request list does not move the repo list"
    );
}

#[gpui::test]
fn issue_keys_follow_the_focused_list(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| {
        let mut store = Store::with_settings(
            net_tx,
            scratch_service("issue_keys_follow_the_focused_list"),
        );
        store.group_by_root = false;
        store.pane_width = 1400.;
        store.replace_repos(vec![
            entry("alpha", "/probe/alpha"),
            entry("beta", "/probe/beta"),
        ]);
        store.github_by_folder.insert(
            "/probe/alpha".into(),
            std::sync::Arc::new(GitHubSnapshot {
                url: Some("https://github.com/probe/alpha".into()),
                pulls: Vec::new(),
                issues: vec![issue(1), issue(2), issue(3)],
                runs: Vec::new(),
                available: true,
            }),
        );
        store.select_repo("/probe/alpha", cx);
        store.open_detail_tab(warp_station::store::WorkspaceTab::Issues, cx);
        store
    });
    let handle = store.clone();
    let (_window, cx) = cx.add_window_view(|window, cx| MainWindow::new(handle, window, cx));
    draw(cx);

    click(cx, "Issue-gh-1");
    cx.simulate_keystrokes("down");
    cx.run_until_parked();
    let open = cx.read_entity(&store, |store, _| {
        store.open_activity(false).map(|open| open.number)
    });
    assert_eq!(open, Some(2), "↓ opens the next issue");
    assert_eq!(
        selection(cx, &store).as_deref(),
        Some("/probe/alpha"),
        "the focused issue list does not move the repo list"
    );
}
