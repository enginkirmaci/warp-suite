//! The list head carries the focus cue: clicking a list that owns its keys
//! raises its label and its bottom rule to the accent, and focusing another
//! pane returns the head to its idle paint. The state selectors encode which
//! paint a real frame drew.

use gpui::TestAppContext;
use gpui::prelude::*;
use std::path::PathBuf;
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
        "warp-station-probe-focus-cue-{name}-{}-{seq}.json",
        std::process::id()
    ));
    std::fs::write(&path, "{}").expect("scratch settings");
    std::sync::Arc::new(warp_station_core::settings::SettingsService::at(path))
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

/// The head draws exactly one of the two state selectors.
fn assert_head(
    cx: &mut gpui::VisualTestContext,
    focused: &'static str,
    idle: &'static str,
    expect_focused: bool,
    why: &str,
) {
    let (shown, hidden) = if expect_focused {
        (focused, idle)
    } else {
        (idle, focused)
    };
    assert!(
        cx.debug_bounds(shown).is_some(),
        "{why}: {shown} should draw"
    );
    assert!(
        cx.debug_bounds(hidden).is_none(),
        "{why}: {hidden} should not draw"
    );
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

fn history() -> Vec<GitCommitInfo> {
    ["aaaaaaa", "bbbbbbb", "ccccccc"]
        .into_iter()
        .enumerate()
        .map(|(ix, hash)| GitCommitInfo {
            hash: hash.into(),
            subject: format!("commit {ix}"),
            author: Some("Probe Author".into()),
            date: chrono::Local::now(),
        })
        .collect()
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
        is_pull_request: false,
        ..pull(number)
    }
}

fn github(pulls: Vec<GitHubItem>, issues: Vec<GitHubItem>) -> GitHubSnapshot {
    GitHubSnapshot {
        url: Some("https://github.com/probe/alpha".into()),
        pulls,
        issues,
        runs: Vec::new(),
        available: true,
    }
}

#[gpui::test]
fn repo_list_head_cue_follows_the_focus(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| {
        let mut store = Store::with_settings(
            net_tx,
            scratch_service("repo_list_head_cue_follows_the_focus"),
        );
        store.group_by_root = false;
        store.replace_repos(vec![
            entry("alpha", "/probe/alpha"),
            entry("beta", "/probe/beta"),
        ]);
        store
    });
    let handle = store.clone();
    let (_window, cx) = cx.add_window_view(|window, cx| MainWindow::new(handle, window, cx));
    draw(cx);

    assert_head(
        cx,
        "repos-head-focused",
        "repos-head-idle",
        false,
        "the window owns the keys first",
    );

    click(cx, "repo-row-0");
    assert_head(
        cx,
        "repos-head-focused",
        "repos-head-idle",
        true,
        "a press on a repo row gives the list the keys",
    );

    // Ctrl+K moves the keyboard into the search field, another pane's input.
    cx.simulate_keystrokes("ctrl-k");
    draw(cx);
    assert_head(
        cx,
        "repos-head-focused",
        "repos-head-idle",
        false,
        "the search field takes the keys away",
    );
}

#[gpui::test]
fn files_list_head_cue_follows_the_focus(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| {
        let mut store = Store::with_settings(
            net_tx,
            scratch_service("files_list_head_cue_follows_the_focus"),
        );
        store.group_by_root = false;
        store.replace_repos(vec![
            entry("alpha", "/probe/alpha"),
            entry("beta", "/probe/beta"),
        ]);
        store.select_repo("/probe/alpha", cx);
        store.open_detail_tab(warp_station::store::WorkspaceTab::Changes, cx);
        store.changes_loaded_for = Some("/probe/alpha".into());
        store.set_changes(Some(changes(&["a.txt", "b.txt"])));
        store
    });
    let handle = store.clone();
    let (_window, cx) = cx.add_window_view(|window, cx| MainWindow::new(handle, window, cx));
    draw(cx);

    assert_head(
        cx,
        "files-head-focused",
        "files-head-idle",
        false,
        "the file list starts idle",
    );

    click(cx, "u-a.txt");
    assert_head(
        cx,
        "files-head-focused",
        "files-head-idle",
        true,
        "a press on a file row gives the file list the keys",
    );

    click(cx, "repo-row-0");
    assert_head(
        cx,
        "files-head-focused",
        "files-head-idle",
        false,
        "the repo list takes the keys back",
    );
}

#[gpui::test]
fn history_list_head_cue_follows_the_focus(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| {
        let mut store = Store::with_settings(
            net_tx,
            scratch_service("history_list_head_cue_follows_the_focus"),
        );
        store.group_by_root = false;
        store.replace_repos(vec![
            entry("alpha", "/probe/alpha"),
            entry("beta", "/probe/beta"),
        ]);
        store.select_repo("/probe/alpha", cx);
        store.set_history(history());
        store.open_detail_tab(warp_station::store::WorkspaceTab::History, cx);
        store
    });
    let handle = store.clone();
    let (_window, cx) = cx.add_window_view(|window, cx| MainWindow::new(handle, window, cx));
    draw(cx);

    assert_head(
        cx,
        "history-head-focused",
        "history-head-idle",
        false,
        "the commit list starts idle",
    );

    click(cx, "commit-aaaaaaa");
    assert_head(
        cx,
        "history-head-focused",
        "history-head-idle",
        true,
        "a press on a commit row gives the commit list the keys",
    );

    click(cx, "repo-row-0");
    assert_head(
        cx,
        "history-head-focused",
        "history-head-idle",
        false,
        "the repo list takes the keys back",
    );
}

#[gpui::test]
fn pull_request_list_head_cue_follows_the_focus(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| {
        let mut store = Store::with_settings(
            net_tx,
            scratch_service("pull_request_list_head_cue_follows_the_focus"),
        );
        store.group_by_root = false;
        store.pane_width = 1400.;
        store.replace_repos(vec![
            entry("alpha", "/probe/alpha"),
            entry("beta", "/probe/beta"),
        ]);
        store.github_by_folder.insert(
            "/probe/alpha".into(),
            std::sync::Arc::new(github(vec![pull(1), pull(2), pull(3)], Vec::new())),
        );
        store.select_repo("/probe/alpha", cx);
        store.open_detail_tab(warp_station::store::WorkspaceTab::PullRequests, cx);
        store
    });
    let handle = store.clone();
    let (_window, cx) = cx.add_window_view(|window, cx| MainWindow::new(handle, window, cx));
    draw(cx);

    assert_head(
        cx,
        "pr-head-focused",
        "pr-head-idle",
        false,
        "the pull request list starts idle",
    );

    click(cx, "PR-gh-1");
    assert_head(
        cx,
        "pr-head-focused",
        "pr-head-idle",
        true,
        "a press on a pull request row gives the list the keys",
    );

    click(cx, "repo-row-0");
    assert_head(
        cx,
        "pr-head-focused",
        "pr-head-idle",
        false,
        "the repo list takes the keys back",
    );
}

#[gpui::test]
fn issue_list_head_cue_follows_the_focus(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| {
        let mut store = Store::with_settings(
            net_tx,
            scratch_service("issue_list_head_cue_follows_the_focus"),
        );
        store.group_by_root = false;
        store.pane_width = 1400.;
        store.replace_repos(vec![
            entry("alpha", "/probe/alpha"),
            entry("beta", "/probe/beta"),
        ]);
        store.github_by_folder.insert(
            "/probe/alpha".into(),
            std::sync::Arc::new(github(Vec::new(), vec![issue(1), issue(2), issue(3)])),
        );
        store.select_repo("/probe/alpha", cx);
        store.open_detail_tab(warp_station::store::WorkspaceTab::Issues, cx);
        store
    });
    let handle = store.clone();
    let (_window, cx) = cx.add_window_view(|window, cx| MainWindow::new(handle, window, cx));
    draw(cx);

    assert_head(
        cx,
        "issues-head-focused",
        "issues-head-idle",
        false,
        "the issue list starts idle",
    );

    click(cx, "Issue-gh-1");
    assert_head(
        cx,
        "issues-head-focused",
        "issues-head-idle",
        true,
        "a press on an issue row gives the list the keys",
    );

    click(cx, "repo-row-0");
    assert_head(
        cx,
        "issues-head-focused",
        "issues-head-idle",
        false,
        "the repo list takes the keys back",
    );
}
