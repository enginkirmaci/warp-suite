//! The split main screen's detail rail must dispatch, not just draw: the star
//! toggles the repo's favourite flag and the sync verbs reach the net thread.

use gpui::TestAppContext;
use gpui::prelude::*;
use std::path::PathBuf;
use warp_station::net::{GitAction, NetCommand, UiEvent};
use warp_station::store::{Store, WorkspaceTab};
use warp_station::ui::window::MainWindow;
use warp_station_core::repo_store::RepoEntry;
use warp_station_core::settings::RepoSortMode;

fn entry(name: &str, path: &str) -> RepoEntry {
    RepoEntry {
        name: name.into(),
        folder_path: PathBuf::from(path),
        solution_path: None,
        tags: Vec::new(),
        git_branch_name: Some("main".into()),
        git_modified_count: 3,
        git_to_push_count: 2,
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

/// A double click: the platform's second press and release carry
/// `click_count: 2`, so both clicks reach the row.
fn double_click(cx: &mut gpui::VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} renders"));
    let center = bounds.center();
    for click_count in 1..=2 {
        cx.simulate_event(gpui::MouseDownEvent {
            position: center,
            modifiers: gpui::Modifiers::default(),
            button: gpui::MouseButton::Left,
            click_count,
            first_mouse: false,
        });
        cx.simulate_event(gpui::MouseUpEvent {
            position: center,
            modifiers: gpui::Modifiers::default(),
            button: gpui::MouseButton::Left,
            click_count,
        });
        draw(cx);
    }
}

fn middle_click(cx: &mut gpui::VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} renders"));
    let center = bounds.center();
    cx.simulate_mouse_down(
        center,
        gpui::MouseButton::Middle,
        gpui::Modifiers::default(),
    );
    cx.simulate_mouse_up(
        center,
        gpui::MouseButton::Middle,
        gpui::Modifiers::default(),
    );
    draw(cx);
}

fn selected(store: &gpui::Entity<Store>, cx: &mut gpui::VisualTestContext) -> Option<String> {
    cx.read_entity(store, |s, _| s.selected_repo.clone())
}

/// A store on scratch settings: what a probe saves must not reach the
/// developer's file, and what it reads must not depend on it.
fn scratch_store(
    cx: &mut TestAppContext,
    name: &str,
    repos: Vec<RepoEntry>,
) -> (gpui::Entity<Store>, async_channel::Receiver<NetCommand>) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let scratch = std::env::temp_dir().join(format!(
        "warp-station-probe-split-{name}-{}.json",
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
        store.replace_repos(repos);
        store
    });
    (store, net_rx)
}

/// Two folder groups, for the probes that drive the bands.
fn grouped_store(cx: &mut TestAppContext, name: &str) -> gpui::Entity<Store> {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let scratch = std::env::temp_dir().join(format!(
        "warp-station-probe-split-{name}-{}.json",
        std::process::id()
    ));
    std::fs::write(
        &scratch,
        r#"{"Repos": {"RepoScanFolders": ["/roots/a", "/second/b"], "GroupReposByRoot": true}}"#,
    )
    .expect("scratch settings");
    let (net_tx, _net_rx) = async_channel::unbounded();
    cx.new(|_cx| {
        let mut store = Store::with_settings(
            net_tx,
            std::sync::Arc::new(warp_station_core::settings::SettingsService::at(scratch)),
        );
        store.replace_repos(vec![
            entry("alpha", "/roots/a/alpha"),
            entry("beta", "/roots/a/beta"),
            entry("gamma", "/second/b/gamma"),
        ]);
        store
    })
}

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
        "warp-station-probe-split-{name}-{}-{seq}.json",
        std::process::id()
    ));
    std::fs::write(&path, "{}").expect("scratch settings");
    std::sync::Arc::new(warp_station_core::settings::SettingsService::at(path))
}

/// The projected repo rows, top to bottom.
fn row_names(store: &gpui::Entity<Store>, cx: &mut gpui::VisualTestContext) -> Vec<String> {
    cx.read_entity(store, |s, _| {
        s.projected
            .iter()
            .filter_map(|row| row.repo_index())
            .map(|ix| s.repos[ix].name.clone())
            .collect()
    })
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

fn clipboard_text(cx: &gpui::VisualTestContext) -> Option<String> {
    cx.read_from_clipboard().and_then(|item| item.text())
}

fn scan_folders(store: &gpui::Entity<Store>, cx: &mut gpui::VisualTestContext) -> Vec<String> {
    cx.read_entity(store, |s, _| s.settings().repos.repo_scan_folders)
        .unwrap_or_default()
}

fn load_changes_folders(net_rx: &async_channel::Receiver<NetCommand>) -> Vec<String> {
    let mut folders = Vec::new();
    while let Ok(command) = net_rx.try_recv() {
        if let NetCommand::LoadChanges { folder } = command {
            folders.push(folder.to_string_lossy().to_string());
        }
    }
    folders
}

#[gpui::test]
fn rail_star_toggles_the_favourite(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| {
        let mut store = Store::with_settings(
            net_tx,
            scratch_service("split-rail_star_toggles_the_favourite"),
        );
        store.group_by_root = false;
        store.replace_repos(vec![entry("alpha", "/probe/alpha")]);
        store
    });
    let handle = store.clone();
    let (_window, cx) = cx.add_window_view(|window, cx| MainWindow::new(handle, window, cx));
    draw(cx);

    let star = cx.debug_bounds("insp-star").expect("the rail star renders");
    cx.simulate_click(star.center(), gpui::Modifiers::default());
    cx.run_until_parked();

    assert!(
        cx.read_entity(&store, |s, _| s.repos[0].is_favorite()),
        "clicking the rail star adds the favourites tag"
    );

    // and the same control takes it back off
    draw(cx);
    let star = cx.debug_bounds("insp-star").expect("the rail star renders");
    cx.simulate_click(star.center(), gpui::Modifiers::default());
    cx.run_until_parked();
    assert!(
        !cx.read_entity(&store, |s, _| s.repos[0].is_favorite()),
        "a second click removes it again"
    );
}

#[gpui::test]
fn rail_sync_verbs_reach_the_net_thread(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let (net_tx, net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| {
        let mut store = Store::with_settings(
            net_tx,
            scratch_service("split-rail_sync_verbs_reach_the_net_thread"),
        );
        store.group_by_root = false;
        // both directions waiting, so both Needs-you verbs render
        let mut repo = entry("alpha", "/probe/alpha");
        repo.git_to_pull_count = 2;
        store.replace_repos(vec![repo]);
        store
    });
    let handle = store.clone();
    let (_window, cx) = cx.add_window_view(|window, cx| MainWindow::new(handle, window, cx));
    draw(cx);

    for (selector, expected) in [
        ("insp-pull", GitAction::Pull),
        ("insp-push", GitAction::Push),
    ] {
        // the previous verb's job is over: one git action runs per repo
        cx.update_entity(&store, |s: &mut Store, cx| {
            s.git_jobs.clear();
            cx.notify();
        });
        draw(cx);
        let bounds = cx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("{selector} renders"));
        cx.simulate_click(bounds.center(), gpui::Modifiers::default());
        cx.run_until_parked();

        let mut seen = false;
        while let Ok(command) = net_rx.try_recv() {
            if let NetCommand::Git { action, .. } = command
                && std::mem::discriminant(&action) == std::mem::discriminant(&expected)
            {
                seen = true;
            }
        }
        assert!(seen, "{selector} dispatches its git action");
        draw(cx);
    }
}

#[gpui::test]
fn keymap_steps_rows_and_favorites(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| {
        let mut store = Store::with_settings(
            net_tx,
            scratch_service("split-keymap_steps_rows_and_favorites"),
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

    // the first row starts selected, so the first ↓ lands on the second
    cx.simulate_keystrokes("down");
    cx.run_until_parked();
    assert_eq!(
        cx.read_entity(&store, |s, _| s.selected_repo.clone())
            .as_deref(),
        Some("/probe/beta"),
        "↓ steps to the next row"
    );

    draw(cx);
    cx.simulate_keystrokes("up");
    cx.run_until_parked();
    assert_eq!(
        cx.read_entity(&store, |s, _| s.selected_repo.clone())
            .as_deref(),
        Some("/probe/alpha"),
        "↑ steps back"
    );

    draw(cx);
    cx.simulate_keystrokes("s");
    cx.run_until_parked();
    assert!(
        cx.read_entity(&store, |s, _| s.repos[0].is_favorite()),
        "S favorites the focused row"
    );
}

#[gpui::test]
fn commit_key_opens_the_changes_tab(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| {
        let mut store = Store::with_settings(
            net_tx,
            scratch_service("split-commit_key_opens_the_changes_tab"),
        );
        store.group_by_root = false;
        store.replace_repos(vec![entry("alpha", "/probe/alpha")]);
        store
    });
    let handle = store.clone();
    let (_window, cx) = cx.add_window_view(|window, cx| MainWindow::new(handle, window, cx));
    draw(cx);

    assert!(
        cx.debug_bounds("detail-tab-changes-panel").is_none(),
        "Overview comes first"
    );
    cx.simulate_keystrokes("c");
    cx.run_until_parked();
    draw(cx);
    assert!(
        cx.debug_bounds("detail-tab-changes-panel").is_some(),
        "C opens the Changes tab"
    );
}

#[gpui::test]
fn rail_menu_opens_and_the_tag_chip_switches_to_an_editor(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| {
        let mut store = Store::with_settings(
            net_tx,
            scratch_service("split-rail_menu_opens_and_the_tag_chip_switches_to_an_editor"),
        );
        store.group_by_root = false;
        store.replace_repos(vec![entry("alpha", "/probe/alpha")]);
        store
    });
    let handle = store.clone();
    let (_window, cx) = cx.add_window_view(|window, cx| MainWindow::new(handle, window, cx));
    draw(cx);

    // "+ tag" swaps the chip for the inline editor
    assert!(
        cx.debug_bounds("insp-add-tag").is_some(),
        "the add-tag chip renders"
    );
    let chip = cx
        .debug_bounds("insp-add-tag")
        .expect("the add-tag chip renders");
    cx.simulate_click(chip.center(), gpui::Modifiers::default());
    draw(cx);
    draw(cx);
    assert!(
        cx.debug_bounds("insp-add-tag").is_none(),
        "the chip is replaced by the tag editor"
    );

    // the "⋯" menu lists the repo verbs, over the rail rather than in it
    assert!(
        cx.debug_bounds("insp-repo-menu").is_none(),
        "the menu starts closed"
    );
    let primary = cx
        .debug_bounds("insp-primary")
        .expect("the primary action renders");
    let more = cx
        .debug_bounds("insp-more")
        .expect("the more button renders");
    cx.simulate_click(more.center(), gpui::Modifiers::default());
    draw(cx);
    assert!(
        cx.debug_bounds("insp-repo-menu").is_some(),
        "the ⋯ button opens the repo menu"
    );
    assert_eq!(
        cx.debug_bounds("insp-primary").map(|bounds| bounds.origin),
        Some(primary.origin),
        "opening the menu never reflows the rail"
    );

    // "Archive" tags the repo, which drops it from the live list
    let archive = cx
        .debug_bounds("insp-menu-archive")
        .expect("the archive row renders");
    cx.simulate_click(archive.center(), gpui::Modifiers::default());
    draw(cx);
    assert!(
        cx.read_entity(&store, |s, _| s.repos[0].is_archived()),
        "the menu's Archive row tags the repo"
    );
}

#[gpui::test]
fn scope_segments_filter_the_projected_list(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| {
        let mut store = Store::with_settings(
            net_tx,
            scratch_service("split-scope_segments_filter_the_projected_list"),
        );
        store.group_by_root = false;
        let mut clean = entry("beta", "/probe/beta");
        clean.git_modified_count = 0;
        clean.git_to_push_count = 0;
        store.replace_repos(vec![entry("alpha", "/probe/alpha"), clean]);
        store
    });
    let handle = store.clone();
    let (_window, cx) = cx.add_window_view(|window, cx| MainWindow::new(handle, window, cx));
    draw(cx);

    let rows = |cx: &mut gpui::VisualTestContext| cx.read_entity(&store, |s, _| s.projected.len());
    assert_eq!(rows(cx), 2, "both rows projected at the start");

    let attention = cx
        .debug_bounds("scope-attention")
        .expect("the needs-attention segment renders");
    cx.simulate_click(attention.center(), gpui::Modifiers::default());
    draw(cx);
    assert_eq!(
        rows(cx),
        1,
        "Needs attention keeps only the repo with work waiting"
    );

    let all = cx
        .debug_bounds("scope-all")
        .expect("the All segment renders");
    cx.simulate_click(all.center(), gpui::Modifiers::default());
    draw(cx);
    assert_eq!(rows(cx), 2, "All clears the scope again");
}

#[gpui::test]
fn tag_editor_commits_on_enter(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| {
        let mut store =
            Store::with_settings(net_tx, scratch_service("split-tag_editor_commits_on_enter"));
        store.group_by_root = false;
        store.replace_repos(vec![entry("alpha", "/probe/alpha")]);
        store
    });
    let handle = store.clone();
    let (_window, cx) = cx.add_window_view(|window, cx| MainWindow::new(handle, window, cx));
    draw(cx);

    let chip = cx
        .debug_bounds("insp-add-tag")
        .expect("the add-tag chip renders");
    cx.simulate_click(chip.center(), gpui::Modifiers::default());
    draw(cx);
    draw(cx);

    cx.simulate_keystrokes("fresh");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    draw(cx);

    assert!(
        cx.read_entity(&store, |s, _| s.repos[0]
            .tags
            .iter()
            .any(|t| t.eq_ignore_ascii_case("fresh"))),
        "typing a tag and pressing Enter writes it"
    );
}

#[gpui::test]
fn rail_file_rows_render_from_a_snapshot(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| {
        let mut store = Store::with_settings(
            net_tx,
            scratch_service("split-rail_file_rows_render_from_a_snapshot"),
        );
        store.group_by_root = false;
        store.replace_repos(vec![entry("alpha", "/probe/alpha")]);
        store.set_changes(Some(warp_station_core::model::GitChangeSnapshot {
            status_files: vec![warp_station_core::model::GitChangedFile {
                path: "src/main.rs".into(),
                status_code: "M".into(),
                additions: Some(3),
                deletions: Some(1),
            }],
            staged_files: Vec::new(),
            unstaged_files: Vec::new(),
            worktree_counts: Default::default(),
            cached_counts: Default::default(),
        }));
        store.selected_repo = Some("/probe/alpha".into());
        store.changes_loaded_for = Some("/probe/alpha".into());
        store
    });
    let handle = store.clone();
    let (_window, cx) = cx.add_window_view(|window, cx| MainWindow::new(handle, window, cx));
    draw(cx);

    assert!(
        cx.debug_bounds("insp-uncommitted").is_some(),
        "the uncommitted row renders from the snapshot's counts"
    );
    assert!(
        cx.debug_bounds("insp-commit").is_some(),
        "and its Review link is there to open Changes"
    );
}

#[gpui::test]
fn rail_issue_rows_render_from_a_github_snapshot(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| {
        let mut store = Store::with_settings(
            net_tx,
            scratch_service("split-rail_issue_rows_render_from_a_github_snapshot"),
        );
        store.group_by_root = false;
        store.replace_repos(vec![entry("alpha", "/probe/alpha")]);
        let snapshot = warp_station_core::github::GitHubSnapshot {
            available: true,
            issues: vec![warp_station_core::github::GitHubItem {
                number: 482,
                title: "Agent stream drops on reconnect".into(),
                url: "https://example.invalid/482".into(),
                author: None,
                labels: Vec::new(),
                is_draft: false,
                review_decision: None,
                state: None,
                updated_at: None,
                created_at: None,
                comments: 0,
                head_ref_name: None,
                base_ref_name: None,
                is_pull_request: false,
            }],
            // the snapshot's other fields default: no PRs, no URL
            ..Default::default()
        };
        store
            .github_by_folder
            .insert("/probe/alpha".to_string(), std::sync::Arc::new(snapshot));
        store
    });
    let handle = store.clone();
    let (_window, cx) = cx.add_window_view(|window, cx| MainWindow::new(handle, window, cx));
    draw(cx);

    assert!(
        cx.debug_bounds("insp-issue-482").is_some(),
        "the issue row renders from the GitHub snapshot"
    );
}

/// The press-again-to-close toggle belongs to the old table's bar. Here the
/// rail stays open, so pressing the selected row must leave it selected, not
/// drop the rail back to the first row.
#[gpui::test]
fn pressing_the_selected_row_keeps_it_selected(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| {
        let mut store = Store::with_settings(
            net_tx,
            scratch_service("split-pressing_the_selected_row_keeps_it_selected"),
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

    click(cx, "repo-row-1");
    assert_eq!(
        selected(&store, cx).as_deref(),
        Some("/probe/beta"),
        "a press selects the row"
    );

    click(cx, "repo-row-1");
    assert_eq!(
        selected(&store, cx).as_deref(),
        Some("/probe/beta"),
        "a second press changes nothing"
    );
}

/// ↑↓ clamp at the ends of the list; re-selecting the clamped row used to
/// count as a second press and clear the selection.
#[gpui::test]
fn stepping_past_the_list_ends_keeps_the_selection(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| {
        let mut store = Store::with_settings(
            net_tx,
            scratch_service("split-stepping_past_the_list_ends_keeps_the_selection"),
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

    cx.simulate_keystrokes("down");
    draw(cx);
    cx.simulate_keystrokes("down");
    draw(cx);
    assert_eq!(
        selected(&store, cx).as_deref(),
        Some("/probe/beta"),
        "↓ on the last row stays there"
    );

    cx.simulate_keystrokes("up");
    draw(cx);
    cx.simulate_keystrokes("up");
    draw(cx);
    assert_eq!(
        selected(&store, cx).as_deref(),
        Some("/probe/alpha"),
        "↑ on the first row stays there"
    );
}

/// The rail's sections load per selection, so the row it opens on has to be a
/// real selection: a rail that only previewed the first row sat on "loading
/// changes…" forever.
#[gpui::test]
fn the_first_row_is_selected_and_loaded_without_a_press(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let (net_tx, net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| {
        let mut store = Store::with_settings(
            net_tx,
            scratch_service("split-the_first_row_is_selected_and_loaded_without_a_press"),
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

    assert_eq!(selected(&store, cx).as_deref(), Some("/probe/alpha"));
    assert_eq!(
        load_changes_folders(&net_rx),
        vec!["/probe/alpha".to_string()],
        "the rail's file list loads for the row it opens on"
    );

    // the selected repo leaving the list hands the rail to the next row
    click(cx, "insp-more");
    click(cx, "insp-menu-archive");
    assert_eq!(selected(&store, cx).as_deref(), Some("/probe/beta"));
    assert_eq!(
        load_changes_folders(&net_rx),
        vec!["/probe/beta".to_string()],
        "the replacement row loads too"
    );

    // so does a rescan that no longer finds it
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.replace_repos(vec![entry("gamma", "/probe/gamma")]);
        cx.notify();
    });
    draw(cx);
    assert_eq!(selected(&store, cx).as_deref(), Some("/probe/gamma"));
    assert_eq!(
        load_changes_folders(&net_rx),
        vec!["/probe/gamma".to_string()]
    );
}

/// A first launch opens on an empty list: the rail picks up the first row as
/// soon as a scan delivers one.
#[gpui::test]
fn the_first_repo_to_arrive_is_selected(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let (net_tx, net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| {
        let mut store = Store::with_settings(
            net_tx,
            scratch_service("split-the_first_repo_to_arrive_is_selected"),
        );
        store.group_by_root = false;
        store.replace_repos(Vec::new());
        store
    });
    let handle = store.clone();
    let (_window, cx) = cx.add_window_view(|window, cx| MainWindow::new(handle, window, cx));
    draw(cx);
    assert_eq!(selected(&store, cx), None, "nothing to select yet");

    cx.update_entity(&store, |s: &mut Store, cx| {
        s.replace_repos(vec![entry("alpha", "/probe/alpha")]);
        cx.notify();
    });
    draw(cx);

    assert_eq!(selected(&store, cx).as_deref(), Some("/probe/alpha"));
    assert_eq!(
        load_changes_folders(&net_rx),
        vec!["/probe/alpha".to_string()]
    );
}

fn open_changes_tab(
    cx: &mut TestAppContext,
) -> (gpui::Entity<Store>, &mut gpui::VisualTestContext) {
    use warp_station_core::model::{GitBranchKind, GitBranchRef};

    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| {
        let mut store = Store::with_settings(net_tx, scratch_service("split-open_changes_tab"));
        store.group_by_root = false;
        store.replace_repos(vec![entry("alpha", "/probe/alpha")]);
        store.branches = vec![GitBranchRef {
            name: "main".into(),
            kind: GitBranchKind::Local,
        }];
        store
    });
    let handle = store.clone();
    let (_window, cx) = cx.add_window_view(|window, cx| MainWindow::new(handle, window, cx));
    draw(cx);

    cx.simulate_keystrokes("c");
    draw(cx);
    draw(cx);
    assert!(
        cx.debug_bounds("detail-tab-changes-panel").is_some(),
        "C opens it"
    );
    (store, cx)
}

/// The branch popup closes on Escape and on a press anywhere else, and leaves
/// the tab where it is.
#[gpui::test]
fn the_branch_menu_closes_on_escape_and_outside_presses(cx: &mut TestAppContext) {
    let (_store, cx) = open_changes_tab(cx);

    click(cx, "branch-button");
    assert!(cx.debug_bounds("branch-menu").is_some(), "it opens");
    cx.simulate_keystrokes("escape");
    draw(cx);
    assert!(cx.debug_bounds("branch-menu").is_none(), "Escape closes it");
    assert!(
        cx.debug_bounds("detail-tab-changes-panel").is_some(),
        "and leaves the tab open"
    );

    click(cx, "branch-button");
    click(cx, "changes-diff");
    assert!(
        cx.debug_bounds("branch-menu").is_none(),
        "a press elsewhere closes it"
    );
    assert!(cx.debug_bounds("detail-tab-changes-panel").is_some());
}

/// The rail's keycaps are a promise: a letter runs its row for the repo the
/// rail shows. warp-agent's row is the one a probe can press: it only messages
/// the net thread, where the others start real processes.
#[gpui::test]
fn a_letter_key_runs_its_launcher(cx: &mut TestAppContext) {
    let (store, net_rx) = scratch_store(cx, "letters", vec![entry("alpha", "/probe/alpha")]);
    let exe = std::env::current_exe().expect("test binary path");
    cx.read_entity(&store, |s, _| {
        s.update_settings(|settings| settings.warp_agent.executable = exe.display().to_string());
    });
    let cx = open_window(&store, cx);

    cx.simulate_keystrokes("a");
    cx.run_until_parked();

    let mut opened = None;
    while let Ok(command) = net_rx.try_recv() {
        if let NetCommand::WarpOpen { folder, .. } = command {
            opened = Some(folder);
        }
    }
    assert_eq!(
        opened,
        Some(PathBuf::from("/probe/alpha")),
        "A runs the warp-agent row"
    );
}

/// A held letter repeats; a launcher row must not start once per repeat.
#[gpui::test]
fn a_held_letter_does_not_repeat(cx: &mut TestAppContext) {
    let (store, _net_rx) = scratch_store(cx, "held", vec![entry("alpha", "/probe/alpha")]);
    let cx = open_window(&store, cx);

    cx.simulate_event(gpui::KeyDownEvent {
        keystroke: gpui::Keystroke::parse("s").expect("a keystroke"),
        is_held: true,
        prefer_character_input: false,
    });
    draw(cx);

    assert!(
        !cx.read_entity(&store, |s, _| s.repos[0].is_favorite()),
        "the repeat of a held S is ignored"
    );
}

/// The row keys act on the list, so nothing open on top of it may let them
/// through: they used to star repos and move the selection behind the sheet.
#[gpui::test]
fn the_keymap_stays_out_of_open_layers(cx: &mut TestAppContext) {
    let (store, _net_rx) = scratch_store(
        cx,
        "layers",
        vec![entry("alpha", "/probe/alpha"), entry("beta", "/probe/beta")],
    );
    let cx = open_window(&store, cx);

    let assert_rows_untouched = |cx: &mut gpui::VisualTestContext, layer: &str| {
        cx.simulate_keystrokes("s");
        draw(cx);
        cx.simulate_keystrokes("down");
        draw(cx);
        assert_eq!(
            selected(&store, cx).as_deref(),
            Some("/probe/alpha"),
            "↓ stays out of the {layer}"
        );
        assert!(
            !cx.read_entity(&store, |s, _| s.repos[0].is_favorite()),
            "S stays out of the {layer}"
        );
    };

    click(cx, "insp-more");
    assert_rows_untouched(cx, "repo menu");
    cx.simulate_keystrokes("escape");
    draw(cx);

    click(cx, "sort-button");
    assert_rows_untouched(cx, "sort menu");
    cx.simulate_keystrokes("escape");
    draw(cx);

    click(cx, "add-repo-button");
    assert_rows_untouched(cx, "Add repo dialog");
}

#[gpui::test]
fn pressing_a_band_folds_its_group_and_persists(cx: &mut TestAppContext) {
    let store = grouped_store(cx, "fold");
    let cx = open_window(&store, cx);
    let collapsed = |cx: &mut gpui::VisualTestContext| {
        cx.read_entity(&store, |s, _| s.settings().repos.collapsed_groups)
    };
    assert_eq!(row_names(&store, cx), ["alpha", "beta", "gamma"]);

    click(cx, "repo-band-/roots/a");
    assert_eq!(row_names(&store, cx), ["gamma"]);
    assert_eq!(collapsed(cx), Some(vec!["/roots/a".to_string()]));

    click(cx, "repo-band-/roots/a");
    assert_eq!(row_names(&store, cx), ["alpha", "beta", "gamma"]);
    assert_eq!(collapsed(cx), None);
}

/// The folder glyph sits inside the band, so its press must not fold too.
#[gpui::test]
fn the_folder_glyph_solos_its_group_without_folding_it(cx: &mut TestAppContext) {
    let store = grouped_store(cx, "solo");
    let cx = open_window(&store, cx);
    let soloed =
        |cx: &mut gpui::VisualTestContext| cx.read_entity(&store, |s, _| s.group_filter.clone());

    click(cx, "repo-band-solo-/roots/a");
    assert_eq!(soloed(cx).as_deref(), Some("/roots/a"));
    assert_eq!(
        row_names(&store, cx),
        ["alpha", "beta"],
        "the group keeps its rows and the others leave"
    );

    click(cx, "repo-band-solo-/roots/a");
    assert_eq!(soloed(cx), None, "the same glyph lifts the filter");

    click(cx, "repo-band-solo-/roots/a");
    click(cx, "clear-button");
    assert_eq!(soloed(cx), None, "and so does Clear");
    assert_eq!(row_names(&store, cx), ["alpha", "beta", "gamma"]);
}

#[gpui::test]
fn the_sort_menu_reorders_the_list_and_persists(cx: &mut TestAppContext) {
    let mut old = entry("alpha", "/probe/alpha");
    old.git_last_commit_at = Some(chrono::Local::now() - chrono::Duration::days(2));
    let mut new = entry("beta", "/probe/beta");
    new.git_last_commit_at = Some(chrono::Local::now());
    let (store, _net_rx) = scratch_store(cx, "sort", vec![old, new]);
    let cx = open_window(&store, cx);
    assert_eq!(row_names(&store, cx), ["alpha", "beta"]);

    click(cx, "sort-button");
    click(cx, "sort-mode-Last activity");
    assert_eq!(
        row_names(&store, cx),
        ["beta", "alpha"],
        "a new mode starts in its own default direction: newest first"
    );
    assert_eq!(
        cx.read_entity(&store, |s, _| s.settings().repos.sort_mode),
        RepoSortMode::LastActivity,
        "the choice is saved"
    );
    assert!(
        cx.debug_bounds("sort-menu").is_none(),
        "picking closes the menu"
    );

    click(cx, "sort-button");
    click(cx, "sort-mode-Last activity");
    assert_eq!(
        row_names(&store, cx),
        ["alpha", "beta"],
        "picking the active mode flips it"
    );
}

/// With GitHub off nothing ranks repos by its counts, so the menu drops it.
#[gpui::test]
fn the_sort_menu_follows_the_github_switch(cx: &mut TestAppContext) {
    let (store, _net_rx) = scratch_store(cx, "sort-github", vec![entry("alpha", "/probe/alpha")]);
    cx.read_entity(&store, |s, _| {
        s.update_settings(|settings| settings.repos.enable_github = false);
    });
    let cx = open_window(&store, cx);

    click(cx, "sort-button");

    assert!(cx.debug_bounds("sort-mode-GitHub").is_none());
    assert!(cx.debug_bounds("sort-mode-Attention first").is_some());
}

#[gpui::test]
fn the_sort_menu_closes_on_outside_presses_and_escape(cx: &mut TestAppContext) {
    let (store, _net_rx) = scratch_store(
        cx,
        "sort-close",
        vec![entry("alpha", "/probe/alpha"), entry("beta", "/probe/beta")],
    );
    let cx = open_window(&store, cx);

    click(cx, "sort-button");
    assert!(cx.debug_bounds("sort-menu").is_some(), "it opens");
    click(cx, "insp-star");
    assert!(
        cx.debug_bounds("sort-menu").is_none(),
        "a press elsewhere closes it"
    );
    assert!(
        cx.read_entity(&store, |s, _| s.repos[0].is_favorite()),
        "and still lands on what it hit"
    );

    click(cx, "sort-button");
    click(cx, "sort-button");
    assert!(
        cx.debug_bounds("sort-menu").is_none(),
        "the button toggles rather than closing and reopening"
    );

    click(cx, "sort-button");
    cx.simulate_keystrokes("escape");
    draw(cx);
    assert!(cx.debug_bounds("sort-menu").is_none());
}

/// The star sits on the row, but pressing it is not a press on the row.
#[gpui::test]
fn the_row_star_toggles_the_favorite_without_selecting(cx: &mut TestAppContext) {
    let (store, _net_rx) = scratch_store(
        cx,
        "row-star",
        vec![entry("alpha", "/probe/alpha"), entry("beta", "/probe/beta")],
    );
    let cx = open_window(&store, cx);

    click(cx, "repo-star-1");

    assert!(
        cx.read_entity(&store, |s, _| s.repos[1].is_favorite()),
        "beta's star marks beta"
    );
    assert_eq!(
        selected(&store, cx).as_deref(),
        Some("/probe/alpha"),
        "the selection stays where it was"
    );
}

#[gpui::test]
fn an_archived_row_has_no_star_to_press(cx: &mut TestAppContext) {
    let mut archived = entry("beta", "/probe/beta");
    archived.tags = vec![warp_station_core::repo_store::ARCHIVED_TAG.to_string()];
    let (store, _net_rx) = scratch_store(
        cx,
        "row-archived",
        vec![entry("alpha", "/probe/alpha"), archived],
    );
    cx.update_entity(&store, |s: &mut Store, cx| s.toggle_show_archived(cx));
    let cx = open_window(&store, cx);

    // the Archive filter shows the archived repo alone, so it is row 0
    click(cx, "repo-star-0");

    assert!(!cx.read_entity(&store, |s, _| s.repos[1].is_favorite()));
}

/// "clean" means nothing at all is waiting: a repo with only a PR open or
/// commits to pull used to read as clean, because the row never showed them.
#[gpui::test]
fn a_row_shows_all_that_waits_on_it(cx: &mut TestAppContext) {
    let quiet = |name: &str, path: &str| {
        let mut repo = entry(name, path);
        repo.git_modified_count = 0;
        repo.git_to_push_count = 0;
        repo
    };
    let mut pulled = quiet("alpha", "/probe/alpha");
    pulled.git_to_pull_count = 2;
    let mut reviewed = quiet("beta", "/probe/beta");
    reviewed.github_pr_count = 1;
    let (store, _net_rx) = scratch_store(
        cx,
        "row-meta",
        vec![pulled, reviewed, quiet("gamma", "/probe/gamma")],
    );
    let cx = open_window(&store, cx);

    assert!(cx.debug_bounds("repo-behind-0").is_some());
    assert!(cx.debug_bounds("repo-clean-0").is_none());
    assert!(cx.debug_bounds("repo-prs-1").is_some());
    assert!(
        cx.debug_bounds("repo-clean-1").is_some(),
        "an open PR leaves the working tree clean"
    );
    assert!(cx.debug_bounds("repo-clean-2").is_some());
}

#[gpui::test]
fn the_github_switch_takes_its_counts_off_the_row(cx: &mut TestAppContext) {
    let mut repo = entry("alpha", "/probe/alpha");
    repo.git_modified_count = 0;
    repo.git_to_push_count = 0;
    repo.github_pr_count = 1;
    let (store, _net_rx) = scratch_store(cx, "row-github", vec![repo]);
    cx.read_entity(&store, |s, _| {
        s.update_settings(|settings| settings.repos.enable_github = false);
    });
    let cx = open_window(&store, cx);

    assert!(cx.debug_bounds("repo-prs-0").is_none());
    assert!(cx.debug_bounds("repo-clean-0").is_some());
}

/// The Azure pass's counts land on the entry and sum with GitHub's: one row,
/// one count, whichever platform answered. The platform-specific chips are
/// gone now that the lists carry the rows.
#[gpui::test]
fn a_row_sums_both_platforms_counts(cx: &mut TestAppContext) {
    let mut repo = entry("alpha", "/probe/alpha");
    repo.git_modified_count = 0;
    repo.git_to_push_count = 0;
    repo.github_pr_count = 1;
    repo.github_issue_count = 1;
    let (store, _net_rx) = scratch_store(cx, "row-azure", vec![repo]);
    let cx = open_window(&store, cx);
    draw(cx);
    assert!(cx.debug_bounds("repo-prs-0").is_some());
    assert!(
        cx.debug_bounds("repo-azure-0").is_none(),
        "no separate chips"
    );

    // the Azure pass settles: its counts reach the entry and the row's counts
    // now read the union
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.on_net_event(
            UiEvent::AzureSettled {
                items: vec![(
                    "/probe/alpha".to_string(),
                    warp_station_core::azure::AzureSnapshot {
                        available: true,
                        pulls: vec![warp_station_core::azure::AzureItem::default()],
                        ..Default::default()
                    },
                )],
            },
            cx,
        );
    });
    draw(cx);
    let (pulls, issues) = store.read_with(cx, |s: &Store, _| {
        let repo = &s.repos[0];
        (repo.pull_total(), repo.issue_total())
    });
    assert_eq!(pulls, 2, "one GitHub and one Azure pull request");
    assert_eq!(issues, 1);

    cx.update_entity(&store, |s: &mut Store, cx| {
        s.update_settings(|settings| settings.repos.enable_azure_devops = false);
        cx.notify();
    });
    draw(cx);
    assert!(
        cx.debug_bounds("repo-azure-0").is_none(),
        "the switch still takes the platform's rows away"
    );
}

#[gpui::test]
fn a_row_marks_a_running_warp_agent_workspace(cx: &mut TestAppContext) {
    let (store, _net_rx) = scratch_store(cx, "row-warp", vec![entry("alpha", "/probe/alpha")]);
    let cx = open_window(&store, cx);
    assert!(cx.debug_bounds("repo-warp-0").is_none());

    cx.update_entity(&store, |s: &mut Store, cx| {
        s.warp_running.insert(
            "/probe/alpha".to_string(),
            warp_station::net::WarpWorkspaceStatus {
                connected: true,
                running: vec!["chat".into()],
            },
        );
        cx.notify();
    });
    draw(cx);

    assert!(cx.debug_bounds("repo-warp-0").is_some());
}

#[gpui::test]
fn the_repo_menu_copies_the_folder_path(cx: &mut TestAppContext) {
    let (store, _net_rx) = scratch_store(cx, "copy-path", vec![entry("alpha", "/probe/alpha")]);
    let cx = open_window(&store, cx);

    click(cx, "insp-more");
    click(cx, "insp-menu-copy-path");

    assert_eq!(clipboard_text(cx).as_deref(), Some("/probe/alpha"));
    assert!(
        cx.debug_bounds("insp-repo-menu").is_none(),
        "the row closes the menu behind its action"
    );
}

#[gpui::test]
fn the_repo_menu_refreshes_the_repo(cx: &mut TestAppContext) {
    let (store, net_rx) = scratch_store(cx, "refresh", vec![entry("alpha", "/probe/alpha")]);
    let cx = open_window(&store, cx);
    load_changes_folders(&net_rx);

    click(cx, "insp-more");
    click(cx, "insp-menu-refresh");

    assert_eq!(
        load_changes_folders(&net_rx),
        vec!["/probe/alpha".to_string()],
        "Refresh reloads the rail's changes"
    );
}

#[gpui::test]
fn the_head_refreshes_the_repo(cx: &mut TestAppContext) {
    let (store, net_rx) = scratch_store(cx, "head-refresh", vec![entry("alpha", "/probe/alpha")]);
    let cx = open_window(&store, cx);
    load_changes_folders(&net_rx);

    click(cx, "detail-refresh");

    assert_eq!(
        load_changes_folders(&net_rx),
        vec!["/probe/alpha".to_string()],
        "the head's refresh reloads the rail's changes"
    );
}

/// Nothing links to GitHub before a pass has named the repo's page, and either
/// pass can be the first to answer.
#[gpui::test]
fn the_github_rows_wait_for_a_url(cx: &mut TestAppContext) {
    let (store, _net_rx) = scratch_store(cx, "github-url", vec![entry("alpha", "/probe/alpha")]);
    let cx = open_window(&store, cx);

    click(cx, "insp-more");
    assert!(
        cx.debug_bounds("insp-menu-github").is_none(),
        "no URL yet, no GitHub row"
    );
    assert!(
        cx.debug_bounds("insp-menu-copy-url").is_none(),
        "no URL yet, no copy row"
    );

    cx.update_entity(&store, |s: &mut Store, cx| {
        let snapshot = warp_station_core::github::GitHubSnapshot {
            url: Some("https://example.invalid/alpha".into()),
            available: true,
            ..Default::default()
        };
        s.github_by_folder
            .insert("/probe/alpha".to_string(), std::sync::Arc::new(snapshot));
        cx.notify();
    });
    draw(cx);
    assert!(
        cx.debug_bounds("insp-menu-github").is_some(),
        "the activity pass names the page"
    );

    click(cx, "insp-menu-copy-url");
    assert_eq!(
        clipboard_text(cx).as_deref(),
        Some("https://example.invalid/alpha")
    );
}

/// The status probe's own read of `remote.origin.url` keeps both GitHub rows
/// on a repo with no snapshot: a plain GitHub remote and no `gh` must not
/// lose them.
#[gpui::test]
fn the_github_rows_fall_back_to_the_repo_remote(cx: &mut TestAppContext) {
    let mut alpha = entry("alpha", "/probe/alpha");
    alpha.github_web_url = Some("https://github.com/owner/repo".into());
    let (store, _net_rx) = scratch_store(cx, "github-remote", vec![alpha]);
    let cx = open_window(&store, cx);

    click(cx, "insp-more");
    assert!(
        cx.debug_bounds("insp-menu-github").is_some(),
        "the remote names the page without a snapshot"
    );
    assert!(
        cx.debug_bounds("insp-menu-copy-url").is_some(),
        "and the copy row follows it"
    );

    click(cx, "insp-menu-copy-url");
    assert_eq!(
        clipboard_text(cx).as_deref(),
        Some("https://github.com/owner/repo")
    );
}

#[gpui::test]
fn removing_a_repo_takes_two_clicks(cx: &mut TestAppContext) {
    let (store, net_rx) = scratch_store(
        cx,
        "remove",
        vec![entry("alpha", "/probe/alpha"), entry("beta", "/probe/beta")],
    );
    cx.read_entity(&store, |s, _| {
        s.update_settings(|settings| {
            settings.repos.repo_scan_folders =
                Some(vec!["/probe/alpha".into(), "/probe/beta".into()]);
        });
    });
    let cx = open_window(&store, cx);

    click(cx, "insp-more");
    click(cx, "insp-menu-remove");
    assert_eq!(
        scan_folders(&store, cx).len(),
        2,
        "the first click only arms the row"
    );
    assert!(
        cx.debug_bounds("insp-repo-menu").is_some(),
        "and keeps the menu open for the second"
    );

    click(cx, "insp-menu-remove");
    assert_eq!(scan_folders(&store, cx), ["/probe/beta"]);
    assert!(cx.debug_bounds("insp-repo-menu").is_none());

    let mut rescanned = false;
    while let Ok(command) = net_rx.try_recv() {
        rescanned |= matches!(command, NetCommand::Scan { .. });
    }
    assert!(rescanned, "the list rebuilds without the repo");
}

/// An armed Remove must never fire on a repo it wasn't armed for.
#[gpui::test]
fn a_moved_selection_closes_the_menu_and_disarms_it(cx: &mut TestAppContext) {
    let (store, _net_rx) = scratch_store(
        cx,
        "disarm",
        vec![entry("alpha", "/probe/alpha"), entry("beta", "/probe/beta")],
    );
    cx.read_entity(&store, |s, _| {
        s.update_settings(|settings| {
            settings.repos.repo_scan_folders =
                Some(vec!["/probe/alpha".into(), "/probe/beta".into()]);
        });
    });
    let cx = open_window(&store, cx);

    click(cx, "insp-more");
    click(cx, "insp-menu-remove");

    // a search hit, say: anything that moves the selection without a press
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.select_repo("/probe/beta", cx);
    });
    draw(cx);
    assert!(
        cx.debug_bounds("insp-repo-menu").is_none(),
        "the menu belongs to the repo it opened on"
    );

    click(cx, "insp-more");
    click(cx, "insp-menu-remove");
    assert_eq!(
        scan_folders(&store, cx).len(),
        2,
        "the reopened menu starts disarmed"
    );
}

#[gpui::test]
fn the_repo_menu_closes_on_outside_presses_and_escape(cx: &mut TestAppContext) {
    let (store, _net_rx) = scratch_store(
        cx,
        "menu-close",
        vec![entry("alpha", "/probe/alpha"), entry("beta", "/probe/beta")],
    );
    let cx = open_window(&store, cx);

    click(cx, "insp-more");
    click(cx, "repo-row-1");
    assert!(
        cx.debug_bounds("insp-repo-menu").is_none(),
        "a press elsewhere closes it"
    );
    assert_eq!(
        selected(&store, cx).as_deref(),
        Some("/probe/beta"),
        "and still lands on what it hit"
    );

    click(cx, "insp-more");
    click(cx, "insp-more");
    assert!(
        cx.debug_bounds("insp-repo-menu").is_none(),
        "the ⋯ button toggles rather than closing and reopening"
    );

    click(cx, "insp-more");
    cx.simulate_keystrokes("escape");
    draw(cx);
    assert!(cx.debug_bounds("insp-repo-menu").is_none());
}

#[gpui::test]
fn a_tag_chip_removes_its_tag(cx: &mut TestAppContext) {
    let mut tagged = entry("alpha", "/probe/alpha");
    tagged.tags = vec!["rust".into(), "llm".into()];
    let (store, _net_rx) = scratch_store(cx, "tags", vec![tagged]);
    let cx = open_window(&store, cx);

    click(cx, "insp-tag-remove-rust");

    assert_eq!(
        cx.read_entity(&store, |s, _| s.repos[0].tags.clone()),
        ["llm"]
    );
}

#[gpui::test]
fn the_branch_line_copies_the_branch(cx: &mut TestAppContext) {
    let (store, _net_rx) = scratch_store(cx, "branch", vec![entry("alpha", "/probe/alpha")]);
    let cx = open_window(&store, cx);

    click(cx, "insp-branch");

    assert_eq!(clipboard_text(cx).as_deref(), Some("main"));
}

/// The rail only shows what GitHub answered: nothing before the answer, and
/// nothing for a repo it knows no facts about.
#[gpui::test]
fn the_details_section_follows_the_github_answer(cx: &mut TestAppContext) {
    let (store, _net_rx) = scratch_store(cx, "details", vec![entry("alpha", "/probe/alpha")]);
    let cx = open_window(&store, cx);
    assert!(cx.debug_bounds("insp-details").is_none());

    let answer = |cx: &mut gpui::VisualTestContext, details| {
        cx.update_entity(&store, |s: &mut Store, cx| {
            s.github_details = Some(details);
            cx.notify();
        });
        draw(cx);
    };

    answer(
        cx,
        warp_station_core::github::GitHubRepoDetails {
            owner: Some("enginkirmaci".into()),
            primary_language: Some("Rust".into()),
            topics: vec!["gpui".into()],
            ..Default::default()
        },
    );
    assert!(cx.debug_bounds("insp-details").is_some());

    answer(cx, Default::default());
    assert!(cx.debug_bounds("insp-details").is_none());
}

/// The details used to be requested by the Overview tab's view, which only
/// lived on because the hidden workspace bar held it.
#[gpui::test]
fn selecting_a_row_asks_github_for_its_details(cx: &mut TestAppContext) {
    let (store, net_rx) = scratch_store(
        cx,
        "details-ask",
        vec![entry("alpha", "/probe/alpha"), entry("beta", "/probe/beta")],
    );
    let exe = std::env::current_exe().expect("test binary path");
    cx.read_entity(&store, |s, _| {
        s.update_settings(|settings| {
            settings.repos.github_executable = exe.display().to_string();
        });
    });
    let cx = open_window(&store, cx);

    click(cx, "repo-row-1");

    let mut asked = Vec::new();
    while let Ok(command) = net_rx.try_recv() {
        if let NetCommand::GitHubDetails { folder, .. } = command {
            asked.push(folder);
        }
    }
    assert_eq!(
        asked,
        [PathBuf::from("/probe/alpha"), PathBuf::from("/probe/beta")],
        "the row the rail opens on, then the pressed one"
    );
}

fn github_item(number: u64, is_pull_request: bool) -> warp_station_core::github::GitHubItem {
    warp_station_core::github::GitHubItem {
        number,
        title: format!("item {number}"),
        url: format!("https://example.invalid/{number}"),
        author: None,
        labels: Vec::new(),
        is_draft: false,
        review_decision: None,
        state: None,
        updated_at: None,
        created_at: None,
        comments: 0,
        head_ref_name: None,
        base_ref_name: None,
        is_pull_request,
    }
}

fn seed_github(
    store: &gpui::Entity<Store>,
    folder: &str,
    pulls: u64,
    issues: u64,
    cx: &mut TestAppContext,
) {
    let snapshot = warp_station_core::github::GitHubSnapshot {
        available: true,
        // a resolved repository: the tab's New verb needs the link
        url: Some("https://github.com/o/alpha".into()),
        runs: Vec::new(),
        pulls: (0..pulls).map(|n| github_item(n, true)).collect(),
        issues: (0..issues).map(|n| github_item(100 + n, false)).collect(),
    };
    cx.update_entity(store, |s: &mut Store, _| {
        s.github_by_folder
            .insert(folder.to_string(), std::sync::Arc::new(snapshot));
    });
}

/// A press in the tab strip swaps the panel; every other stays unmounted.
#[gpui::test]
fn the_tab_strip_switches_the_panel(cx: &mut TestAppContext) {
    let (store, _net_rx) = scratch_store(cx, "tabs", vec![entry("alpha", "/probe/alpha")]);
    seed_github(&store, "/probe/alpha", 1, 1, cx);
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.azure_by_folder.insert(
            "/probe/alpha".to_string(),
            std::sync::Arc::new(warp_station_core::azure::AzureSnapshot {
                available: true,
                ..Default::default()
            }),
        );
        cx.notify();
    });
    let cx = open_window(&store, cx);
    cx.simulate_keystrokes("c");
    draw(cx);
    draw(cx);
    assert!(cx.debug_bounds("detail-tab-changes-panel").is_some());

    click(cx, "detail-tab-pulls");
    assert!(cx.debug_bounds("detail-tab-pulls-panel").is_some());
    assert!(cx.debug_bounds("pr-list").is_some(), "the PR rows mount");
    assert!(cx.debug_bounds("detail-tab-changes-panel").is_none());

    click(cx, "detail-tab-issues");
    assert!(cx.debug_bounds("issue-list").is_some(), "so do the issues");

    click(cx, "detail-tab-overview");
    assert!(
        cx.debug_bounds("readme-card").is_some(),
        "the README card lives on Overview"
    );
    assert!(
        cx.debug_bounds("detail-tab-readme").is_none(),
        "there is no README tab"
    );

    click(cx, "detail-tab-pipelines");
    assert!(cx.debug_bounds("detail-tab-pipelines-panel").is_some());
}

fn ready_pull(number: u64) -> warp_station_core::github::GitHubPull {
    warp_station_core::github::GitHubPull {
        number,
        title: format!("item {number}"),
        state: "OPEN".into(),
        mergeable: "MERGEABLE".into(),
        body: "What it does".into(),
        ..Default::default()
    }
}

/// `gh` stands in as a binary every machine has: the channel nobody drains
/// keeps it from ever running.
fn use_a_located_gh(store: &gpui::Entity<Store>, cx: &mut TestAppContext) {
    cx.update_entity(store, |s: &mut Store, _| {
        s.update_settings(|settings| {
            settings.repos.enable_github = true;
            settings.repos.github_executable = std::env::current_exe()
                .unwrap()
                .to_string_lossy()
                .to_string();
        });
    });
}

/// The net thread's answer to one verb: without it the head stays busy, as it
/// would while `gh` runs.
fn settle(
    store: &gpui::Entity<Store>,
    action: warp_station_core::github::PullAction,
    cx: &mut gpui::VisualTestContext,
) {
    cx.update_entity(store, |s: &mut Store, cx| {
        s.on_net_event(
            UiEvent::GitHubPullActed {
                folder: "/probe/alpha".into(),
                number: 0,
                action,
                result: Ok(()),
            },
            cx,
        );
    });
    draw(cx);
}

fn sent_issue_asks(net_rx: &async_channel::Receiver<NetCommand>) -> Vec<u64> {
    let mut numbers = Vec::new();
    while let Ok(command) = net_rx.try_recv() {
        if let NetCommand::GitHubIssue { number, .. } = command {
            numbers.push(number);
        }
    }
    numbers
}

fn sent_creates(
    net_rx: &async_channel::Receiver<NetCommand>,
) -> Vec<warp_station_core::github::NewPullRequest> {
    let mut requests = Vec::new();
    while let Ok(command) = net_rx.try_recv() {
        if let NetCommand::GitHubPullCreate { request, .. } = command {
            requests.push(request);
        }
    }
    requests
}

fn sent_actions(
    net_rx: &async_channel::Receiver<NetCommand>,
) -> Vec<warp_station_core::github::PullAction> {
    let mut actions = Vec::new();
    while let Ok(command) = net_rx.try_recv() {
        if let NetCommand::GitHubPullAction { action, .. } = command {
            actions.push(action);
        }
    }
    actions
}

fn sent_merges(net_rx: &async_channel::Receiver<NetCommand>) -> Vec<u64> {
    let mut numbers = Vec::new();
    while let Ok(command) = net_rx.try_recv() {
        if let NetCommand::GitHubMerge { number, .. } = command {
            numbers.push(number);
        }
    }
    numbers
}

/// A failed closed load is not "still loading": the tab offers Retry, and
/// Retry clears the ask guard so the request goes out again (audit A2).
#[gpui::test]
fn a_failed_closed_load_offers_retry(cx: &mut TestAppContext) {
    let (store, net_rx) = scratch_store(cx, "closed-fail", vec![entry("alpha", "/probe/alpha")]);
    seed_github(&store, "/probe/alpha", 1, 0, cx);
    use_a_located_gh(&store, cx);
    let cx = open_window(&store, cx);
    draw(cx);
    click(cx, "detail-tab-pulls");
    while net_rx.try_recv().is_ok() {}

    click(cx, "pr-state-closed");
    draw(cx);
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.on_net_event(
            UiEvent::GitHubClosedLoaded {
                folder: "/probe/alpha".into(),
                pull_requests: true,
                items: None,
            },
            cx,
        );
    });
    draw(cx);
    assert!(
        cx.debug_bounds("closed-retry").is_some(),
        "a failed load must offer Retry, not spin forever"
    );

    while net_rx.try_recv().is_ok() {}
    click(cx, "closed-retry");
    draw(cx);
    let mut asked = 0;
    while let Ok(command) = net_rx.try_recv() {
        if let NetCommand::GitHubClosed { pull_requests, .. } = command {
            assert!(pull_requests);
            asked += 1;
        }
    }
    assert_eq!(asked, 1, "Retry asks once");
}

/// The Pull requests tab is the design's workbench: the 320 work list beside
/// the selected pull request, with the repository's own head still above it.
/// The first row is the selection where both columns fit.
#[gpui::test]
fn a_pull_request_opens_beside_the_list(cx: &mut TestAppContext) {
    let (store, _net_rx) = scratch_store(cx, "pull", vec![entry("alpha", "/probe/alpha")]);
    seed_github(&store, "/probe/alpha", 2, 0, cx);
    let cx = open_window(&store, cx);
    draw(cx);
    click(cx, "detail-tab-pulls");
    draw(cx);

    assert!(cx.debug_bounds("pr-list").is_some(), "the work list shows");
    assert!(
        cx.debug_bounds("pull-request").is_some(),
        "the first row opens beside it"
    );
    assert!(
        cx.debug_bounds("detail-tab-pulls").is_some(),
        "the repo head stays above the workbench"
    );
    assert!(
        cx.debug_bounds("pull-back").is_none(),
        "Back belongs to the narrow pane"
    );
    assert!(
        cx.debug_bounds("pull-merge").is_none(),
        "no verb until the full view says it is allowed"
    );

    click(cx, "PR-gh-1");
    assert!(cx.debug_bounds("pull-request").is_some());
    assert_eq!(
        store.read_with(cx, |s, _| s.open_pull.as_ref().map(|o| o.item.number)),
        Some(1),
        "a press selects that row"
    );

    cx.update_entity(&store, |s: &mut Store, cx| {
        s.open_pull.as_mut().unwrap().pull = Some(ready_pull(1));
        cx.notify();
    });
    draw(cx);
    assert!(cx.debug_bounds("pull-merge").is_some());
    assert!(cx.debug_bounds("pull-comments").is_some());
    assert!(cx.debug_bounds("pull-policies").is_some());
    assert!(cx.debug_bounds("pull-reviewers-card").is_some());
}

/// Below 900 px of pane the list and the pull request take turns: a row opens
/// the detail with Back, and Back and Escape both return to the list.
#[gpui::test]
fn a_narrow_pane_shows_one_column_and_back_returns(cx: &mut TestAppContext) {
    let (store, _net_rx) = scratch_store(cx, "pull-stacked", vec![entry("alpha", "/probe/alpha")]);
    seed_github(&store, "/probe/alpha", 2, 0, cx);
    let cx = open_window(&store, cx);
    cx.simulate_resize(gpui::size(gpui::px(1100.), gpui::px(760.)));
    draw(cx);
    click(cx, "detail-tab-pulls");
    draw(cx);

    assert!(cx.debug_bounds("pr-list").is_some(), "the list has the tab");
    assert!(
        cx.debug_bounds("pull-request").is_none(),
        "nothing is selected in a narrow pane"
    );

    click(cx, "PR-gh-1");
    draw(cx);
    assert!(cx.debug_bounds("pull-request").is_some());
    assert!(cx.debug_bounds("pr-list").is_none(), "the list steps aside");
    assert!(cx.debug_bounds("pull-back").is_some());

    cx.simulate_keystrokes("escape");
    draw(cx);
    assert!(cx.debug_bounds("pr-list").is_some(), "Escape goes back");
    assert!(cx.debug_bounds("pull-request").is_none());

    click(cx, "PR-gh-1");
    click(cx, "pull-back");
    draw(cx);
    assert!(cx.debug_bounds("pr-list").is_some(), "so does Back");
}

/// The pull request's own strip: Overview carries the policies, the description
/// and the comments; Files and Commits swap the body for their viewers.
#[gpui::test]
fn the_pull_request_tabs_swap_their_bodies(cx: &mut TestAppContext) {
    let (store, _net_rx) = scratch_store(cx, "pull-tabs", vec![entry("alpha", "/probe/alpha")]);
    seed_github(&store, "/probe/alpha", 1, 0, cx);
    let cx = open_window(&store, cx);
    draw(cx);
    click(cx, "detail-tab-pulls");

    let mut pull = ready_pull(0);
    pull.files = vec![warp_station_core::github::GitHubFilePatch {
        path: "src/main.rs".into(),
        status: "M".into(),
        additions: 2,
        deletions: 1,
        patch: String::new(),
    }];
    pull.commits = vec![warp_station_core::github::GitHubCommit {
        oid: "abc1234def".into(),
        headline: "Add the thing".into(),
        author: Some("engin".into()),
        committed_at: None,
    }];
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.open_pull.as_mut().unwrap().pull = Some(pull);
        cx.notify();
    });
    draw(cx);

    assert!(cx.debug_bounds("pull-policies").is_some(), "Overview first");
    assert!(cx.debug_bounds("pull-files").is_none());

    click(cx, "pull-tab-files");
    assert!(cx.debug_bounds("pull-files").is_some(), "the file rail");
    assert!(cx.debug_bounds("pull-file-0").is_some());
    assert!(cx.debug_bounds("pull-policies").is_none());

    click(cx, "pull-tab-commits");
    assert!(cx.debug_bounds("pull-commits").is_some(), "the commit list");
    assert!(cx.debug_bounds("pull-commit-0").is_some());
    assert!(cx.debug_bounds("pull-copy-sha").is_some());

    click(cx, "pull-tab-overview");
    assert!(cx.debug_bounds("pull-policies").is_some());
}

/// Every verb is one command: Approve, Mark as draft, and Abandon (which asks
/// first) all leave the view as a `GitHubPullAction`.
#[gpui::test]
fn the_pull_request_verbs_send_one_command(cx: &mut TestAppContext) {
    use warp_station_core::github::PullAction;

    let (store, net_rx) = scratch_store(cx, "pull-verbs", vec![entry("alpha", "/probe/alpha")]);
    seed_github(&store, "/probe/alpha", 1, 0, cx);
    use_a_located_gh(&store, cx);
    let cx = open_window(&store, cx);
    draw(cx);
    click(cx, "detail-tab-pulls");
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.open_pull.as_mut().unwrap().pull = Some(ready_pull(0));
        cx.notify();
    });
    draw(cx);
    while net_rx.try_recv().is_ok() {}

    click(cx, "pull-approve");
    assert_eq!(sent_actions(&net_rx), [PullAction::Approve]);
    settle(&store, PullAction::Approve, cx);

    // the primary's menu: draft, then abandon through its confirmation
    click(cx, "pull-primary-menu");
    assert!(cx.debug_bounds("pull-primary-menu").is_some());
    click(cx, "pull-menu-draft");
    assert_eq!(sent_actions(&net_rx), [PullAction::Draft], "Mark as draft");
    settle(&store, PullAction::Draft, cx);

    click(cx, "pull-primary-menu");
    click(cx, "pull-menu-abandon");
    assert!(
        cx.debug_bounds("abandon-dialog").is_some(),
        "Abandon asks first"
    );
    click(cx, "btn-danger-Abandon pull request");
    assert_eq!(sent_actions(&net_rx), [PullAction::Close]);
    assert!(
        cx.debug_bounds("abandon-dialog").is_none(),
        "the card closes with the answer"
    );
}

/// New pull request opens the form in the detail column — not an overlay, and
/// not the browser — and Cancel gives the selection back.
#[gpui::test]
fn the_new_pull_request_form_opens_in_the_pane(cx: &mut TestAppContext) {
    let (store, _net_rx) = scratch_store(cx, "new-pull", vec![entry("alpha", "/probe/alpha")]);
    seed_github(&store, "/probe/alpha", 1, 0, cx);
    let cx = open_window(&store, cx);
    draw(cx);
    click(cx, "detail-tab-pulls");
    click(cx, "list-new");

    assert!(cx.debug_bounds("new-pull").is_some(), "the form draws");
    assert!(
        cx.debug_bounds("pull-request").is_none(),
        "it takes the detail column"
    );
    assert!(
        cx.debug_bounds("pr-list").is_some(),
        "the list stays beside it"
    );
    for field in [
        "new-pull-head",
        "new-pull-base",
        "new-pull-title",
        "new-pull-description",
        "new-pull-reviewer",
        "new-pull-foot",
    ] {
        assert!(cx.debug_bounds(field).is_some(), "{field} draws");
    }
    assert!(
        cx.debug_bounds("new-pull-back").is_none(),
        "Back belongs to the narrow pane"
    );
    assert_eq!(
        store.read_with(cx, |s, _| s.new_pull.as_ref().map(|f| f.head.clone())),
        Some("main".into()),
        "the source starts on the checked out branch"
    );

    click(cx, "new-pull-cancel");
    assert!(cx.debug_bounds("new-pull").is_none());
    assert!(
        cx.debug_bounds("pull-request").is_some(),
        "Cancel gives the selection back"
    );
}

/// The form previews what the source branch would bring: files on Files,
/// commits on Commits, each tab counting its own.
#[gpui::test]
fn the_form_previews_the_branches(cx: &mut TestAppContext) {
    use warp_station_core::model::{CompareCommit, ComparePreview, GitChangedFile};

    let (store, _net_rx) =
        scratch_store(cx, "new-pull-preview", vec![entry("alpha", "/probe/alpha")]);
    seed_github(&store, "/probe/alpha", 1, 0, cx);
    let cx = open_window(&store, cx);
    draw(cx);
    click(cx, "detail-tab-pulls");
    click(cx, "list-new");

    let preview = ComparePreview {
        files: vec![GitChangedFile {
            path: "src/main.rs".into(),
            status_code: "M".into(),
            additions: Some(4),
            deletions: Some(1),
        }],
        commits: vec![CompareCommit {
            hash: "abc1234def".into(),
            subject: "Add the thing".into(),
            author: Some("engin".into()),
            committed_at: None,
            files: 1,
        }],
    };
    cx.update_entity(&store, |s: &mut Store, cx| {
        if let Some(form) = s.new_pull.as_mut() {
            form.head = "feat/x".into();
            form.base = "main".into();
        }
        s.compare = Some(((("main".to_string()), "feat/x".to_string()), Some(preview)));
        cx.notify();
    });
    draw(cx);

    click(cx, "new-pull-tab-files");
    assert!(cx.debug_bounds("new-pull-files").is_some());
    assert!(cx.debug_bounds("new-pull-file").is_some(), "a file row");
    assert!(cx.debug_bounds("new-pull-overview").is_none());

    click(cx, "new-pull-tab-commits");
    assert!(cx.debug_bounds("new-pull-commits").is_some());
    assert!(cx.debug_bounds("new-pull-commit").is_some(), "a commit row");

    click(cx, "new-pull-tab-overview");
    assert!(cx.debug_bounds("new-pull-overview").is_some());
}

/// Create refuses a blank title and two of the same branch before it spends a
/// process, and says why on Overview.
#[gpui::test]
fn the_form_refuses_what_gh_would_refuse(cx: &mut TestAppContext) {
    let (store, net_rx) =
        scratch_store(cx, "new-pull-refuse", vec![entry("alpha", "/probe/alpha")]);
    seed_github(&store, "/probe/alpha", 1, 0, cx);
    use_a_located_gh(&store, cx);
    let cx = open_window(&store, cx);
    draw(cx);
    click(cx, "detail-tab-pulls");
    click(cx, "list-new");
    while net_rx.try_recv().is_ok() {}

    // the source and the base both start on main
    click(cx, "new-pull-create");
    assert!(
        cx.debug_bounds("new-pull-error").is_some(),
        "two of the same branch is refused"
    );
    assert!(sent_creates(&net_rx).is_empty(), "nothing is sent");

    cx.update_entity(&store, |s: &mut Store, cx| {
        if let Some(form) = s.new_pull.as_mut() {
            form.head = "feat/x".into();
        }
        cx.notify();
    });
    draw(cx);
    click(cx, "new-pull-create");
    assert!(
        sent_creates(&net_rx).is_empty(),
        "an empty title is refused"
    );
    assert!(cx.debug_bounds("new-pull-error").is_some());
}

/// Create sends one request and the new pull request takes the pane, with the
/// list back on Open.
#[gpui::test]
fn creating_sends_one_request_and_selects_it(cx: &mut TestAppContext) {
    let (store, net_rx) =
        scratch_store(cx, "new-pull-create", vec![entry("alpha", "/probe/alpha")]);
    seed_github(&store, "/probe/alpha", 1, 0, cx);
    use_a_located_gh(&store, cx);
    let cx = open_window(&store, cx);
    draw(cx);
    click(cx, "detail-tab-pulls");
    click(cx, "list-new");
    cx.update_entity(&store, |s: &mut Store, cx| {
        if let Some(form) = s.new_pull.as_mut() {
            form.head = "feat/x".into();
            form.base = "main".into();
        }
        cx.notify();
    });
    draw(cx);
    while net_rx.try_recv().is_ok() {}

    click(cx, "new-pull-title");
    cx.simulate_keystrokes("a d d  space t h e  space t h i n g");
    draw(cx);
    click(cx, "new-pull-create");

    let sent = sent_creates(&net_rx);
    assert_eq!(sent.len(), 1, "one create");
    assert_eq!(sent[0].title, "add the thing");
    assert_eq!(sent[0].head, "feat/x");
    assert_eq!(sent[0].base, "main");
    assert!(!sent[0].draft);

    // gh's refusal comes back in the form, and the form stays open
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.on_net_event(
            UiEvent::GitHubPullCreated {
                folder: "/probe/alpha".into(),
                request: Box::new(sent[0].clone()),
                result: Err("no permission on the base".into()),
            },
            cx,
        );
    });
    draw(cx);
    assert!(
        cx.debug_bounds("new-pull-error").is_some(),
        "gh's words show"
    );

    // Create as draft rides the same verb
    click(cx, "new-pull-primary-menu");
    assert!(cx.debug_bounds("new-pull-menu").is_some());
    click(cx, "new-pull-menu-draft");
    let drafts = sent_creates(&net_rx);
    assert_eq!(drafts.len(), 1, "one more create");
    assert!(drafts[0].draft, "the menu's draft item drafts");

    // gh answers with the number it created
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.on_net_event(
            UiEvent::GitHubPullCreated {
                folder: "/probe/alpha".into(),
                request: Box::new(sent[0].clone()),
                result: Ok(7),
            },
            cx,
        );
    });
    draw(cx);

    assert!(cx.debug_bounds("new-pull").is_none(), "the form closes");
    assert!(
        cx.debug_bounds("pull-request").is_some(),
        "the new pull request shows"
    );
    assert_eq!(
        store.read_with(cx, |s, _| s.open_pull.as_ref().map(|o| o.item.number)),
        Some(7)
    );
    assert!(
        !store.read_with(cx, |s, _| s.pull_created),
        "the list drained the flag"
    );
}

/// Below 900 px of pane the form takes it whole, and Back and Escape return to
/// the list.
#[gpui::test]
fn the_form_takes_the_narrow_pane(cx: &mut TestAppContext) {
    let (store, _net_rx) =
        scratch_store(cx, "new-pull-narrow", vec![entry("alpha", "/probe/alpha")]);
    seed_github(&store, "/probe/alpha", 1, 0, cx);
    let cx = open_window(&store, cx);
    cx.simulate_resize(gpui::size(gpui::px(1100.), gpui::px(760.)));
    draw(cx);
    click(cx, "detail-tab-pulls");
    draw(cx);
    click(cx, "list-new");
    draw(cx);

    assert!(cx.debug_bounds("new-pull").is_some());
    assert!(cx.debug_bounds("pr-list").is_none(), "the list steps aside");
    assert!(cx.debug_bounds("new-pull-back").is_some(), "Back is here");

    cx.simulate_keystrokes("escape");
    draw(cx);
    assert!(
        cx.debug_bounds("new-pull").is_none(),
        "Escape leaves the form"
    );
    assert!(cx.debug_bounds("pr-list").is_some());
}

/// The Issues tab is the same workbench: the 320 list, the selected issue's
/// detail beside it, one `gh issue view` for the row that is picked.
#[gpui::test]
fn the_issue_workbench_shows_the_selected_issue(cx: &mut TestAppContext) {
    let (store, net_rx) = scratch_store(cx, "issue-bench", vec![entry("alpha", "/probe/alpha")]);
    seed_github(&store, "/probe/alpha", 0, 2, cx);
    use_a_located_gh(&store, cx);
    let cx = open_window(&store, cx);
    draw(cx);
    while net_rx.try_recv().is_ok() {}
    click(cx, "detail-tab-issues");

    assert!(
        cx.debug_bounds("issue-list-column").is_some(),
        "the work list"
    );
    assert!(cx.debug_bounds("issue-list").is_some());
    assert!(
        cx.debug_bounds("issue-detail").is_some(),
        "the first row is the selection"
    );
    assert_eq!(
        store.read_with(cx, |s, _| s.open_issue.as_ref().map(|o| o.item.number)),
        Some(100)
    );
    assert_eq!(
        sent_issue_asks(&net_rx),
        [100],
        "one gh issue view for the selected row"
    );

    // the answer fills the detail: description, state strip, thread
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.on_net_event(
            UiEvent::GitHubIssueLoaded {
                folder: "/probe/alpha".into(),
                number: 100,
                issue: Some(Box::new(warp_station_core::github::GitHubIssue {
                    number: 100,
                    title: "item 100".into(),
                    url: "https://example.invalid/100".into(),
                    state: "OPEN".into(),
                    author: Some("engin".into()),
                    labels: vec!["bug".into()],
                    assignees: vec!["mert".into()],
                    created_at: None,
                    updated_at: None,
                    body: "What it does".into(),
                    comments: vec![warp_station_core::github::GitHubComment {
                        author: "mert".into(),
                        created_at: None,
                        body: "Same here".into(),
                    }],
                })),
            },
            cx,
        );
    });
    draw(cx);
    assert!(cx.debug_bounds("issue-description").is_some());
    assert!(cx.debug_bounds("issue-status").is_some());
    assert!(cx.debug_bounds("issue-open").is_some(), "the one primary");

    // a press picks that row, and asks for it
    click(cx, "Issue-gh-101");
    assert_eq!(
        store.read_with(cx, |s, _| s.open_issue.as_ref().map(|o| o.item.number)),
        Some(101)
    );
    assert_eq!(sent_issue_asks(&net_rx), [101]);
}

/// Below 900 px of pane the Issues tab is one column with Back, like the pull
/// request's.
#[gpui::test]
fn a_narrow_issue_pane_keeps_one_column_and_back(cx: &mut TestAppContext) {
    let (store, _net_rx) = scratch_store(cx, "issue-narrow", vec![entry("alpha", "/probe/alpha")]);
    seed_github(&store, "/probe/alpha", 0, 2, cx);
    let cx = open_window(&store, cx);
    cx.simulate_resize(gpui::size(gpui::px(1100.), gpui::px(760.)));
    draw(cx);
    click(cx, "detail-tab-issues");
    draw(cx);

    assert!(
        cx.debug_bounds("issue-list").is_some(),
        "the list has the tab"
    );
    assert!(
        cx.debug_bounds("issue-detail").is_none(),
        "nothing is selected in a narrow pane"
    );

    click(cx, "Issue-gh-100");
    draw(cx);
    assert!(cx.debug_bounds("issue-detail").is_some());
    assert!(
        cx.debug_bounds("issue-list").is_none(),
        "the list steps aside"
    );
    assert!(
        cx.debug_bounds("issue-detail-back").is_some(),
        "Back is here"
    );

    cx.simulate_keystrokes("escape");
    draw(cx);
    assert!(cx.debug_bounds("issue-list").is_some(), "Escape goes back");
    assert!(cx.debug_bounds("issue-detail").is_none());
}

/// The Files rail is a column beside the diff on a wide detail, and a band
/// above it under 900 px of column — the workbench's column is `pane - 320`, so
/// a 1500 px window is already under it.
#[gpui::test]
fn the_files_rail_stacks_under_900(cx: &mut TestAppContext) {
    let (store, _net_rx) = scratch_store(cx, "rail-stack", vec![entry("alpha", "/probe/alpha")]);
    seed_github(&store, "/probe/alpha", 1, 0, cx);
    let cx = open_window(&store, cx);
    draw(cx);
    click(cx, "detail-tab-pulls");

    let mut pull = ready_pull(0);
    pull.files = vec![warp_station_core::github::GitHubFilePatch {
        path: "src/main.rs".into(),
        status: "M".into(),
        additions: 2,
        deletions: 1,
        patch: String::new(),
    }];
    pull.commits = vec![warp_station_core::github::GitHubCommit {
        oid: "abc1234def".into(),
        headline: "Add the thing".into(),
        author: Some("engin".into()),
        committed_at: None,
    }];
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.open_pull.as_mut().unwrap().pull = Some(pull);
        cx.notify();
    });
    draw(cx);
    click(cx, "pull-tab-files");

    // 1920 px: the pane is 1412 and the column 1092, so the rail is a column
    assert!(cx.debug_bounds("pull-files").is_some());
    assert!(
        cx.debug_bounds("pull-files-band").is_none(),
        "a wide column keeps the rail beside the diff"
    );

    // 1500 px: the pane is 992 and the column 672, so the rail is a band
    cx.simulate_resize(gpui::size(gpui::px(1500.), gpui::px(960.)));
    draw(cx);
    let band = cx
        .debug_bounds("pull-files-band")
        .expect("a narrow column stacks the rail");
    assert!(
        band.size.width >= gpui::px(600.),
        "the band spans the whole 672 px column, not a 240 px rail: {}",
        band.size.width
    );
    assert_eq!(band.size.height, gpui::px(180.), "a bounded band");
    assert!(
        cx.debug_bounds("pull-files").is_some(),
        "its rows still draw"
    );

    // the commit's inner rail follows the same rule
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.pull_commit_files.insert(
            "abc1234def".into(),
            Some(vec![warp_station_core::github::GitHubFilePatch {
                path: "src/main.rs".into(),
                status: "M".into(),
                additions: 2,
                deletions: 1,
                patch: String::new(),
            }]),
        );
        cx.notify();
    });
    click(cx, "pull-tab-commits");
    assert!(cx.debug_bounds("pull-commit-files-band").is_some());
}

/// The composer posts what was typed, and a landed comment empties it.
#[gpui::test]
fn the_comment_composer_posts_its_words(cx: &mut TestAppContext) {
    use warp_station_core::github::PullAction;

    let (store, net_rx) = scratch_store(cx, "pull-comment", vec![entry("alpha", "/probe/alpha")]);
    seed_github(&store, "/probe/alpha", 1, 0, cx);
    use_a_located_gh(&store, cx);
    let cx = open_window(&store, cx);
    draw(cx);
    click(cx, "detail-tab-pulls");
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.open_pull.as_mut().unwrap().pull = Some(ready_pull(0));
        cx.notify();
    });
    draw(cx);
    while net_rx.try_recv().is_ok() {}

    // the composer sits under the description, so the window has to be tall
    // enough for its bounds to be inside the overview's viewport
    cx.simulate_resize(gpui::size(gpui::px(1920.), gpui::px(1600.)));
    draw(cx);
    click(cx, "pull-comment-field");
    cx.simulate_keystrokes("n i c e");
    draw(cx);
    click(cx, "pull-comment");
    assert_eq!(
        sent_actions(&net_rx),
        [PullAction::Comment("nice".into())],
        "the composer sends its words"
    );

    settle(&store, PullAction::Comment(String::new()), cx);
    assert!(
        !store.read_with(cx, |s, _| s.pull_comment_sent),
        "the view drains the flag"
    );
}

/// Merging can't be undone: the first press arms, the second sends, and a
/// failing check leaves the button dead.
#[gpui::test]
fn the_merge_takes_two_presses_and_passing_checks(cx: &mut TestAppContext) {
    use warp_station_core::github::{CheckState, GitHubCheck};

    let (store, net_rx) = scratch_store(cx, "merge", vec![entry("alpha", "/probe/alpha")]);
    seed_github(&store, "/probe/alpha", 1, 0, cx);
    use_a_located_gh(&store, cx);
    let cx = open_window(&store, cx);
    draw(cx);
    click(cx, "detail-tab-pulls");
    click(cx, "PR-gh-0");

    let mut blocked = ready_pull(0);
    blocked.checks.push(GitHubCheck {
        name: "build".into(),
        state: CheckState::Failed,
        url: None,
    });
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.open_pull.as_mut().unwrap().pull = Some(blocked);
        cx.notify();
    });
    draw(cx);
    click(cx, "pull-checks-toggle");
    assert!(
        cx.debug_bounds("pull-checks").is_some(),
        "View checks opens the rows"
    );
    click(cx, "pull-merge");
    click(cx, "pull-merge");
    assert!(sent_merges(&net_rx).is_empty(), "a failed check blocks it");

    cx.update_entity(&store, |s: &mut Store, cx| {
        s.open_pull.as_mut().unwrap().pull = Some(ready_pull(0));
        cx.notify();
    });
    draw(cx);
    click(cx, "pull-merge");
    assert!(sent_merges(&net_rx).is_empty(), "the first press only arms");
    click(cx, "pull-merge");
    assert_eq!(sent_merges(&net_rx), [0]);
    click(cx, "pull-merge");
    assert!(sent_merges(&net_rx).is_empty(), "one merge at a time");
}

/// The filter narrows as it is typed, says when nothing matches, and Clear
/// filter and Escape both bring the rows back. Its letters stay out of the
/// row keymap.
#[gpui::test]
fn the_issue_filter_narrows_the_rows(cx: &mut TestAppContext) {
    let (store, _net_rx) = scratch_store(cx, "filter", vec![entry("alpha", "/probe/alpha")]);
    seed_github(&store, "/probe/alpha", 0, 2, cx);
    let cx = open_window(&store, cx);
    draw(cx);
    click(cx, "detail-tab-issues");
    assert!(cx.debug_bounds("Issue-gh-100").is_some());
    assert!(cx.debug_bounds("Issue-gh-101").is_some());

    click(cx, "issue-filter");
    cx.simulate_keystrokes("1 0 1");
    draw(cx);
    assert!(cx.debug_bounds("Issue-gh-100").is_none());
    assert!(cx.debug_bounds("Issue-gh-101").is_some());

    cx.simulate_keystrokes("c");
    draw(cx);
    assert!(
        cx.debug_bounds("detail-tab-issues-panel").is_some(),
        "c stays in the field instead of opening Changes"
    );
    assert!(cx.debug_bounds("issue-clear-filter").is_some());

    click(cx, "issue-clear-filter");
    draw(cx);
    assert!(cx.debug_bounds("Issue-gh-100").is_some());

    click(cx, "issue-filter");
    cx.simulate_keystrokes("1 0 1");
    draw(cx);
    assert!(cx.debug_bounds("Issue-gh-100").is_none());
    cx.simulate_keystrokes("escape");
    draw(cx);
    draw(cx);
    assert!(
        cx.debug_bounds("Issue-gh-100").is_some(),
        "Escape clears it"
    );
}

/// The design's Issues toolbar does not depend on the row count: an empty list
/// keeps Filter issues, Sort and the New verb when a create target is known.
/// Pull requests still carry no filter.
#[gpui::test]
fn the_issues_toolbar_survives_an_empty_list(cx: &mut TestAppContext) {
    let (store, _net_rx) = scratch_store(cx, "empty-issues", vec![entry("alpha", "/probe/alpha")]);
    seed_github(&store, "/probe/alpha", 0, 0, cx);
    let cx = open_window(&store, cx);
    draw(cx);

    click(cx, "detail-tab-issues");
    assert!(
        cx.debug_bounds("issue-filter").is_some(),
        "Filter issues stays on an empty list"
    );
    assert!(
        cx.debug_bounds("issue-sort").is_some(),
        "Sort stays on an empty list"
    );
    assert!(
        cx.debug_bounds("list-new").is_some(),
        "a known GitHub target keeps New issue"
    );
    assert!(
        cx.debug_bounds("issue-list").is_none(),
        "there are no rows to list"
    );
    click(cx, "issue-sort");
    assert!(
        cx.debug_bounds("issue-sort-menu").is_some(),
        "the sort menu opens over the empty state"
    );

    click(cx, "detail-tab-pulls");
    assert!(
        cx.debug_bounds("issue-filter").is_none(),
        "the design gives pull requests no filter"
    );
}

/// The menu marks the sort, a pick is remembered, and an outside press closes it.
#[gpui::test]
fn the_issue_sort_is_remembered(cx: &mut TestAppContext) {
    use warp_station_core::activity::IssueSort;

    let (store, _net_rx) = scratch_store(cx, "sort", vec![entry("alpha", "/probe/alpha")]);
    seed_github(&store, "/probe/alpha", 0, 2, cx);
    let cx = open_window(&store, cx);
    draw(cx);
    click(cx, "detail-tab-issues");

    click(cx, "issue-sort");
    assert!(cx.debug_bounds("issue-sort-menu").is_some());
    click(cx, "issue-sort-comments");
    assert!(cx.debug_bounds("issue-sort-menu").is_none());
    assert_eq!(
        store.read_with(cx, |s, _| s.settings().repos.issue_sort),
        Some("comments".to_string())
    );
    assert_eq!(
        store.read_with(cx, |s, _| s.issue_sort()),
        IssueSort::Comments
    );

    click(cx, "issue-sort");
    click(cx, "detail-tab-issues");
    assert!(cx.debug_bounds("issue-sort-menu").is_none());
}

/// Closed asks once, shows its rows, and a closed pull request opens in the
/// pane like an open one — the state pill says which it is.
#[gpui::test]
fn the_closed_list_loads_when_it_is_asked_for(cx: &mut TestAppContext) {
    let (store, net_rx) = scratch_store(cx, "closed", vec![entry("alpha", "/probe/alpha")]);
    seed_github(&store, "/probe/alpha", 1, 0, cx);
    use_a_located_gh(&store, cx);
    let cx = open_window(&store, cx);
    draw(cx);
    click(cx, "detail-tab-pulls");
    while net_rx.try_recv().is_ok() {}

    click(cx, "pr-state-closed");
    draw(cx);
    let mut asked = 0;
    while let Ok(command) = net_rx.try_recv() {
        if let NetCommand::GitHubClosed { pull_requests, .. } = command {
            assert!(pull_requests);
            asked += 1;
        }
    }
    assert_eq!(asked, 1, "one ask however many frames draw");

    cx.update_entity(&store, |s: &mut Store, cx| {
        use warp_station_core::activity::{ActivityItem, ActivitySource};
        let rows: Vec<ActivityItem> = [github_item(7, true)]
            .iter()
            .map(ActivityItem::of_github)
            .collect();
        s.closed.insert(
            ("/probe/alpha".to_string(), true, ActivitySource::GitHub),
            std::sync::Arc::new(rows),
        );
        cx.notify();
    });
    draw(cx);
    assert!(cx.debug_bounds("PR-gh-7").is_some());
    assert!(cx.debug_bounds("PR-gh-0").is_none());

    click(cx, "PR-gh-7");
    assert!(
        cx.debug_bounds("pull-request").is_some(),
        "a closed row opens its own detail"
    );

    click(cx, "pr-state-open");
    assert!(cx.debug_bounds("PR-gh-0").is_some());
}

fn select(store: &gpui::Entity<Store>, folder: &str, cx: &mut gpui::VisualTestContext) {
    store.update(cx, |s: &mut Store, cx| s.select_repo(folder, cx));
    draw(cx);
}

/// A job belongs to its repository: moving away keeps it running and leaves the
/// other repository free, and the job's row, tab and button all say so.
#[gpui::test]
fn a_job_stays_with_its_repository(cx: &mut TestAppContext) {
    let (store, net_rx) = scratch_store(
        cx,
        "jobs",
        vec![entry("alpha", "/probe/alpha"), entry("beta", "/probe/beta")],
    );
    let cx = open_window(&store, cx);
    draw(cx);
    select(&store, "/probe/alpha", cx);
    store.update(cx, |s: &mut Store, cx| {
        s.open_repo_tab("/probe/alpha", cx);
        s.show_repo_tab(None, cx);
        s.open_detail_tab(WorkspaceTab::Changes, cx);
    });
    draw(cx);
    select(&store, "/probe/alpha", cx);
    while net_rx.try_recv().is_ok() {}

    store.update(cx, |s: &mut Store, _| s.scanning = false);
    click(cx, "rail-Fetch");
    assert_eq!(store.read_with(cx, |s, _| s.running_job()), Some("fetch"));
    assert!(
        cx.debug_bounds("repo-row-job-0").is_some(),
        "the row says so"
    );
    assert!(
        cx.debug_bounds("repo-strip-job-0").is_some(),
        "so does the tab"
    );
    assert_eq!(
        store.read_with(cx, |s, _| s.activity()),
        Some("Fetching alpha".to_string())
    );

    click(cx, "rail-Fetch");
    let sent = std::iter::from_fn(|| net_rx.try_recv().ok())
        .filter(|command| matches!(command, NetCommand::Git { .. }))
        .count();
    assert_eq!(sent, 1, "one job per repository");

    select(&store, "/probe/beta", cx);
    assert!(!store.read_with(cx, |s, _| s.git_busy()), "beta is free");
    assert_eq!(
        store.read_with(cx, |s, _| s.git_jobs.get("/probe/alpha").copied()),
        Some("fetch"),
        "and alpha is still running"
    );
    click(cx, "rail-Fetch");
    assert_eq!(
        store.read_with(cx, |s, _| s.activity()),
        Some("2 running".to_string())
    );
}

/// A landed job holds its past tense for a moment and only on its own repo.
#[gpui::test]
fn a_landed_job_says_so_for_a_moment(cx: &mut TestAppContext) {
    let (store, _net_rx) = scratch_store(
        cx,
        "landed",
        vec![entry("alpha", "/probe/alpha"), entry("beta", "/probe/beta")],
    );
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.select_repo("/probe/alpha", cx);
        s.git_jobs.insert("/probe/alpha".into(), "pull");
        s.on_net_event(
            UiEvent::GitDone {
                folder: "/probe/alpha".into(),
                label: "pull",
                error: None,
                commit_hash: None,
                detail: None,
            },
            cx,
        );
    });
    assert_eq!(
        cx.read_entity(&store, |s, _| s.finished_job()),
        Some("pull")
    );
    assert!(!cx.read_entity(&store, |s, _| s.git_busy()));

    cx.update_entity(&store, |s: &mut Store, cx| s.select_repo("/probe/beta", cx));
    assert_eq!(cx.read_entity(&store, |s, _| s.finished_job()), None);

    cx.update_entity(&store, |s: &mut Store, cx| {
        s.select_repo("/probe/alpha", cx);
    });
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(2));
    cx.run_until_parked();
    assert_eq!(cx.read_entity(&store, |s, _| s.finished_job()), None);
}

/// A failed job leaves no past tense behind: the toast says what went wrong.
#[gpui::test]
fn a_failed_job_shows_no_success(cx: &mut TestAppContext) {
    let (store, _net_rx) = scratch_store(cx, "failed", vec![entry("alpha", "/probe/alpha")]);
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.select_repo("/probe/alpha", cx);
        s.git_jobs.insert("/probe/alpha".into(), "push");
        s.on_net_event(
            UiEvent::GitDone {
                folder: "/probe/alpha".into(),
                label: "push",
                error: Some("rejected".into()),
                commit_hash: None,
                detail: None,
            },
            cx,
        );
    });
    assert_eq!(cx.read_entity(&store, |s, _| s.finished_job()), None);
    assert!(!cx.read_entity(&store, |s, _| s.git_busy()));
}

/// The menu adds tags up, the button names what is on, and Any lets go.
#[gpui::test]
fn the_tag_menu_filters_the_list(cx: &mut TestAppContext) {
    let mut rust = entry("alpha", "/probe/alpha");
    rust.tags = vec!["rust".into()];
    let mut web = entry("beta", "/probe/beta");
    web.tags = vec!["web".into()];
    let (store, _net_rx) = scratch_store(cx, "tag-menu", vec![rust, web]);
    let cx = open_window(&store, cx);
    draw(cx);

    click(cx, "tag-button");
    assert!(cx.debug_bounds("tag-menu").is_some());
    click(cx, "tag-pick-rust");
    assert!(cx.debug_bounds("tag-menu").is_some(), "a pick stays open");
    assert!(cx.debug_bounds("repo-row-1").is_none(), "web left the list");

    click(cx, "tag-pick-web");
    assert!(cx.debug_bounds("repo-row-1").is_some(), "tags add up");

    click(cx, "tag-any");
    assert!(cx.debug_bounds("tag-menu").is_none());
    assert!(store.read_with(cx, |s, _| s.tag_filters.is_empty()));
}

/// Open tabs come back on restart, and one whose repo left the list doesn't.
#[gpui::test]
fn repo_tabs_come_back_after_a_restart(cx: &mut TestAppContext) {
    let (store, _net_rx) = scratch_store(
        cx,
        "tabs-saved",
        vec![entry("alpha", "/probe/alpha"), entry("beta", "/probe/beta")],
    );
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.open_repo_tab("/probe/beta", cx);
        s.open_repo_tab("/probe/alpha", cx);
    });
    let saved = cx.read_entity(&store, |s, _| s.settings().repos.repo_tabs.clone());
    assert_eq!(
        saved,
        Some(vec!["/probe/beta".to_string(), "/probe/alpha".to_string()])
    );

    // a second run over the same settings file, before its scan lands
    let scratch = std::env::temp_dir().join(format!(
        "warp-station-probe-split-tabs-saved-{}.json",
        std::process::id()
    ));
    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| {
        let mut store = Store::with_settings(
            net_tx,
            std::sync::Arc::new(warp_station_core::settings::SettingsService::at(scratch)),
        );
        store.scanning = true;
        store.replace_repos(Vec::new());
        store
    });
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.ensure_row_selected(cx);
        assert!(s.repo_tabs.is_empty(), "no repo has landed yet");

        s.replace_repos(vec![entry("alpha", "/probe/alpha")]);
        s.ensure_row_selected(cx);
        assert_eq!(s.repo_tabs.len(), 1, "alpha lands");
        assert_eq!(
            s.settings().repos.repo_tabs.map(|tabs| tabs.len()),
            Some(2),
            "beta may still land"
        );

        s.scanning = false;
        s.ensure_row_selected(cx);
        assert_eq!(s.repo_tabs.len(), 1);
    });
    assert_eq!(
        cx.read_entity(&store, |s, _| s.settings().repos.repo_tabs),
        Some(vec!["/probe/alpha".to_string()]),
        "the scan settled and beta is gone"
    );
}

/// A drag on the list's edge moves it inside the bounds, and the width is kept.
#[gpui::test]
fn the_list_edge_drags_inside_its_bounds(cx: &mut TestAppContext) {
    use gpui::{Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, point, px};

    let (store, _net_rx) = scratch_store(cx, "width", vec![entry("alpha", "/probe/alpha")]);
    let cx = open_window(&store, cx);
    draw(cx);
    let before = cx
        .debug_bounds("repos-list-pane")
        .expect("the list")
        .size
        .width;
    let handle = cx.debug_bounds("list-resize").expect("the handle").center();

    let drag_to = |x: f32, cx: &mut gpui::VisualTestContext| {
        cx.simulate_event(MouseDownEvent {
            position: handle,
            button: MouseButton::Left,
            modifiers: Modifiers::default(),
            click_count: 1,
            first_mouse: false,
        });
        let target = point(px(x), handle.y);
        for position in [point(handle.x + px(8.), handle.y), target, target] {
            cx.simulate_event(MouseMoveEvent {
                position,
                pressed_button: Some(MouseButton::Left),
                modifiers: Modifiers::default(),
            });
        }
        cx.simulate_event(MouseUpEvent {
            position: target,
            button: MouseButton::Left,
            modifiers: Modifiers::default(),
            click_count: 1,
        });
        cx.run_until_parked();
        draw(cx);
    };

    let left = cx.debug_bounds("repos-list-pane").unwrap().origin.x;
    drag_to(f32::from(left) + 380., cx);
    let moved = cx.debug_bounds("repos-list-pane").unwrap().size.width;
    assert!(moved != before, "the drag moved it: {moved:?}");
    assert_eq!(moved, px(380.));
    assert_eq!(
        store.read_with(cx, |s, _| s.settings().repos.list_width),
        Some(380.)
    );

    let handle_now = cx.debug_bounds("list-resize").unwrap().center();
    assert!(handle_now.x < handle.x);
}

/// An integration that is off takes its GitHub tabs and its Overview card
/// along, and anything still asking for one of those tabs lands on Overview.
/// Pipelines is every repository's tab and brings its own empty state.
#[gpui::test]
fn the_integration_switches_take_their_tabs_along(cx: &mut TestAppContext) {
    let (store, _net_rx) = scratch_store(cx, "tabs-off", vec![entry("alpha", "/probe/alpha")]);
    // GitHub data alone must not put a row on Overview while the switch is off
    seed_github(&store, "/probe/alpha", 1, 1, cx);
    cx.read_entity(&store, |s, _| {
        s.update_settings(|settings| {
            settings.repos.enable_github = false;
            settings.repos.enable_azure_devops = false;
        });
    });
    let cx = open_window(&store, cx);
    assert!(
        cx.debug_bounds("insp-github-open").is_none(),
        "the GitHub rows leave with the switch"
    );

    cx.update_entity(&store, |s: &mut Store, cx| {
        s.open_detail_tab(WorkspaceTab::Issues, cx);
    });
    draw(cx);
    draw(cx);

    assert!(cx.debug_bounds("detail-tab-issues").is_none());
    assert!(cx.debug_bounds("detail-tab-pulls").is_none());
    assert!(
        cx.debug_bounds("detail-tab-pipelines").is_some(),
        "Pipelines is always a tab"
    );
    assert!(
        cx.debug_bounds("detail-tab-overview-panel").is_some(),
        "a tab that is gone falls back to Overview"
    );

    click(cx, "detail-tab-pipelines");
    draw(cx);
    assert!(
        cx.debug_bounds("detail-tab-pipelines-panel").is_some(),
        "the empty Pipelines tab opens like any other"
    );
}

#[gpui::test]
fn the_overview_links_open_their_tab(cx: &mut TestAppContext) {
    let (store, _net_rx) = scratch_store(cx, "links", vec![entry("alpha", "/probe/alpha")]);
    seed_github(&store, "/probe/alpha", 0, 4, cx);
    let cx = open_window(&store, cx);
    let visit = |cx: &mut gpui::VisualTestContext, link: &'static str, panel: &'static str| {
        click(cx, link);
        draw(cx);
        assert!(cx.debug_bounds(panel).is_some(), "{link} opens {panel}");
        click(cx, "detail-tab-overview");
    };

    visit(cx, "insp-commit", "detail-tab-changes-panel");
    visit(cx, "insp-github-open", "detail-tab-issues-panel");
}

/// With no issues the card's latest pull request opens the pull request
/// itself, not its list: the pane is the one detail surface.
#[gpui::test]
fn the_latest_pull_request_opens_its_detail(cx: &mut TestAppContext) {
    let (store, _net_rx) = scratch_store(cx, "links-prs", vec![entry("alpha", "/probe/alpha")]);
    seed_github(&store, "/probe/alpha", 2, 0, cx);
    let cx = open_window(&store, cx);

    assert!(cx.debug_bounds("insp-github-open").is_none());
    click(cx, "insp-open-pr");
    draw(cx);

    assert!(cx.debug_bounds("pull-request").is_some());
}

#[gpui::test]
fn the_uncommitted_row_reviews_in_changes(cx: &mut TestAppContext) {
    let (store, _net_rx) = scratch_store(cx, "links-files", vec![entry("alpha", "/probe/alpha")]);
    cx.update_entity(&store, |s: &mut Store, _| {
        let status_files = (0..4)
            .map(|n| warp_station_core::model::GitChangedFile {
                path: format!("src/file{n}.rs"),
                status_code: ".M".into(),
                additions: None,
                deletions: None,
            })
            .collect();
        s.selected_repo = Some("/probe/alpha".into());
        s.set_changes(Some(warp_station_core::model::GitChangeSnapshot {
            status_files,
            ..Default::default()
        }));
        s.changes_loaded_for = Some("/probe/alpha".into());
    });
    let cx = open_window(&store, cx);

    click(cx, "insp-commit");
    draw(cx);

    assert!(cx.debug_bounds("detail-tab-changes-panel").is_some());
}

/// Azure only reaches Overview when its latest run failed; the row opens the
/// Pipelines tab, where the rest of the project lives.
#[gpui::test]
fn a_failed_azure_run_opens_its_pipelines(cx: &mut TestAppContext) {
    let (store, _net_rx) = scratch_store(cx, "azure-rail", vec![entry("alpha", "/probe/alpha")]);
    let cx = open_window(&store, cx);
    assert!(cx.debug_bounds("insp-azure-row").is_none());

    let seed = |cx: &mut gpui::VisualTestContext, failed: bool| {
        cx.update_entity(&store, |s: &mut Store, cx| {
            let snapshot = warp_station_core::azure::AzureSnapshot {
                available: true,
                pipelines: vec![warp_station_core::azure::PipelineRun {
                    id: 7,
                    build_number: "20261102.3".into(),
                    definition_name: Some("Build and test".into()),
                    result: failed.then(|| "failed".to_string()),
                    status: Some("completed".into()),
                    ..Default::default()
                }],
                ..Default::default()
            };
            s.azure_by_folder
                .insert("/probe/alpha".to_string(), std::sync::Arc::new(snapshot));
            cx.notify();
        });
        draw(cx);
    };

    seed(cx, false);
    assert!(
        cx.debug_bounds("insp-azure-row").is_none(),
        "a green run leaves the card quiet"
    );

    seed(cx, true);
    assert!(cx.debug_bounds("insp-azure-row").is_some());
    click(cx, "insp-azure-open");
    assert!(cx.debug_bounds("detail-tab-pipelines-panel").is_some());
    assert!(
        cx.debug_bounds("pipeline-run-0").is_some(),
        "the Pipelines tab lists the recent runs"
    );
    assert!(
        cx.debug_bounds("azure-refresh").is_some(),
        "and carries the refresh for its pass"
    );
}

/// Nothing waiting says so, once, instead of an empty card.
#[gpui::test]
fn a_clean_tree_says_there_is_nothing_to_do(cx: &mut TestAppContext) {
    let mut repo = entry("alpha", "/probe/alpha");
    repo.git_modified_count = 0;
    repo.git_to_push_count = 0;
    let (store, _net_rx) = scratch_store(cx, "needs-clean", vec![repo]);
    let cx = open_window(&store, cx);

    assert!(cx.debug_bounds("insp-needs-clean").is_some());
    assert!(cx.debug_bounds("insp-uncommitted").is_none());
    assert!(cx.debug_bounds("insp-pull").is_none());
    assert!(cx.debug_bounds("insp-push").is_none());
}

/// The wand's draft renders in place, and only Save touches the file.
#[gpui::test]
fn a_readme_draft_saves_or_discards_from_its_tab(cx: &mut TestAppContext) {
    let repo_dir = std::env::temp_dir().join(format!(
        "warp-station-probe-split-readme-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&repo_dir).expect("scratch repo");
    let readme = repo_dir.join("README.md");
    std::fs::write(&readme, "# Old\n").expect("seed README");

    let folder = repo_dir.to_string_lossy().to_string();
    let (store, _net_rx) = scratch_store(cx, "readme", vec![entry("alpha", &folder)]);
    let cx = open_window(&store, cx);
    draw(cx);
    assert!(
        cx.debug_bounds("readme-card").is_some(),
        "the README card is on Overview"
    );
    assert!(
        cx.debug_bounds("readme-save").is_none(),
        "nothing to save without a draft"
    );

    let draft = |cx: &mut gpui::VisualTestContext, text: &str| {
        cx.update_entity(&store, |s: &mut Store, cx| {
            s.readme_draft = Some(text.to_string());
            cx.notify();
        });
        draw(cx);
    };

    draft(cx, "# Discarded");
    click(cx, "readme-discard");
    assert!(cx.read_entity(&store, |s, _| s.readme_draft.is_none()));
    assert_eq!(std::fs::read_to_string(&readme).unwrap(), "# Old\n");

    draft(cx, "# New");
    click(cx, "readme-save");
    assert_eq!(std::fs::read_to_string(&readme).unwrap(), "# New\n");
    assert!(
        cx.debug_bounds("readme-save").is_none(),
        "the draft is spent once saved"
    );

    let _ = std::fs::remove_dir_all(&repo_dir);
}

/// A press on the README card selects text; it must not take the window down.
#[gpui::test]
fn clicking_the_readme_card_keeps_the_window_alive(cx: &mut TestAppContext) {
    let repo_dir = std::env::temp_dir().join(format!(
        "warp-station-probe-split-readme-click-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&repo_dir).expect("scratch repo");
    std::fs::write(
        repo_dir.join("README.md"),
        "# Alpha\n\nSome words to select.\n",
    )
    .expect("seed README");

    let folder = repo_dir.to_string_lossy().to_string();
    let (store, _net_rx) = scratch_store(cx, "readme-click", vec![entry("alpha", &folder)]);
    let cx = open_window(&store, cx);
    draw(cx);

    let bounds = cx.debug_bounds("readme-pane").expect("the card draws");
    cx.simulate_click(bounds.center(), gpui::Modifiers::default());
    draw(cx);

    assert!(
        cx.debug_bounds("readme-card").is_some(),
        "the card survives the press"
    );
    let _ = std::fs::remove_dir_all(&repo_dir);
}

#[gpui::test]
fn an_empty_list_says_why(cx: &mut TestAppContext) {
    let (store, _net_rx) = scratch_store(cx, "empty", Vec::new());
    let cx = open_window(&store, cx);

    assert!(cx.debug_bounds("repos-empty").is_some());
}

/// Selecting the selected repo changes nothing: it must not throw away the
/// workspace the rail already loaded.
#[gpui::test]
fn selecting_the_selected_repo_keeps_its_workspace(cx: &mut TestAppContext) {
    let (store, _net_rx) = scratch_store(cx, "reselect", vec![entry("alpha", "/probe/alpha")]);

    cx.update_entity(&store, |s: &mut Store, cx| {
        s.select_repo("/probe/alpha", cx);
        s.changes_loaded_for = Some("/probe/alpha".into());
        s.select_repo("/probe/alpha", cx);
    });

    assert_eq!(
        cx.read_entity(&store, |s, _| s.changes_loaded_for.clone())
            .as_deref(),
        Some("/probe/alpha")
    );
}

/// A dialog is a layer on top of the split screen: the rail stays behind it,
/// and Escape closes it.
#[gpui::test]
fn dialogs_render_over_the_split_layout(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| {
        let mut store = Store::with_settings(
            net_tx,
            scratch_service("split-dialogs_render_over_the_split_layout"),
        );
        store.group_by_root = false;
        store.replace_repos(vec![entry("alpha", "/probe/alpha")]);
        store
    });
    let handle = store.clone();
    let (_window, cx) = cx.add_window_view(|window, cx| MainWindow::new(handle, window, cx));
    draw(cx);

    let button = cx
        .debug_bounds("add-repo-button")
        .expect("the toolbar offers Add repo");
    cx.simulate_click(button.center(), gpui::Modifiers::default());
    draw(cx);

    assert!(
        cx.debug_bounds("add-repo-overlay").is_some(),
        "the dialog opens"
    );
    assert!(
        cx.debug_bounds("repos-detail").is_some(),
        "the detail pane stays behind the dialog"
    );

    cx.simulate_keystrokes("escape");
    draw(cx);
    assert!(
        cx.debug_bounds("add-repo-overlay").is_none(),
        "Escape closes the dialog"
    );
}

// ---- repo tabs -------------------------------------------------------------

fn two_repos(cx: &mut TestAppContext) -> (gpui::Entity<Store>, &mut gpui::VisualTestContext) {
    let (store, net_rx) = scratch_store(
        cx,
        "repo-tabs",
        vec![entry("alpha", "/probe/alpha"), entry("beta", "/probe/beta")],
    );
    // the channel stays open for the whole probe
    std::mem::forget(net_rx);
    let cx = open_window(&store, cx);
    (store, cx)
}

fn active_tab(store: &gpui::Entity<Store>, cx: &mut gpui::VisualTestContext) -> Option<String> {
    cx.read_entity(store, |s, _| s.active_repo_tab.clone())
}

#[gpui::test]
fn a_repo_opens_in_a_tab_with_the_list_out_of_the_way(cx: &mut TestAppContext) {
    let (store, cx) = two_repos(cx);
    assert!(cx.debug_bounds("repos-list-pane").is_some());

    cx.simulate_keystrokes("ctrl-t");
    draw(cx);
    assert_eq!(active_tab(&store, cx).as_deref(), Some("/probe/alpha"));
    assert!(
        cx.debug_bounds("repo-strip-tab-0").is_some(),
        "the tab shows"
    );
    assert!(
        cx.debug_bounds("repos-list-pane").is_none(),
        "the list leaves"
    );
    assert!(
        cx.debug_bounds("detail-refresh").is_some(),
        "the head keeps its refresh inside a tab"
    );

    click(cx, "repo-tab-home");
    assert!(
        cx.debug_bounds("repos-list-pane").is_some(),
        "and comes back"
    );
    assert_eq!(cx.read_entity(&store, |s, _| s.repo_tabs.len()), 1);
}

#[gpui::test]
fn each_view_keeps_its_own_detail_tab(cx: &mut TestAppContext) {
    let (store, cx) = two_repos(cx);
    cx.simulate_keystrokes("ctrl-t");
    draw(cx);
    click(cx, "detail-tab-changes");
    click(cx, "repo-tab-home");
    assert!(
        cx.debug_bounds("detail-tab-overview-panel").is_some(),
        "Repositories stayed on Overview"
    );

    click(cx, "repo-strip-tab-0");
    assert!(
        cx.debug_bounds("detail-tab-changes-panel").is_some(),
        "the tab stayed on its Changes"
    );
    assert_eq!(selected(&store, cx).as_deref(), Some("/probe/alpha"));
}

#[gpui::test]
fn closing_the_active_tab_hands_over_to_its_neighbour(cx: &mut TestAppContext) {
    let (store, cx) = two_repos(cx);
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.open_repo_tab("/probe/alpha", cx);
        s.open_repo_tab("/probe/beta", cx);
    });
    draw(cx);

    click(cx, "repo-strip-close-1");
    assert_eq!(
        active_tab(&store, cx).as_deref(),
        Some("/probe/alpha"),
        "the last tab hands over to the one before it"
    );
    click(cx, "repo-strip-close-0");
    assert_eq!(active_tab(&store, cx), None);
    assert!(
        cx.debug_bounds("repos-list-pane").is_some(),
        "back on the list"
    );
}

#[gpui::test]
fn a_row_goes_to_its_tab_from_inside_a_tab_and_previews_otherwise(cx: &mut TestAppContext) {
    let (store, cx) = two_repos(cx);
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.open_repo_tab("/probe/alpha", cx);
        s.open_repo_tab("/probe/beta", cx);
        s.pick_repo("/probe/alpha", cx);
    });
    assert_eq!(
        active_tab(&store, cx).as_deref(),
        Some("/probe/alpha"),
        "a repo with a tab goes there"
    );
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.close_repo_tab("/probe/beta", cx);
        s.pick_repo("/probe/beta", cx);
    });
    assert_eq!(active_tab(&store, cx), None, "one without previews");
    assert_eq!(selected(&store, cx).as_deref(), Some("/probe/beta"));
}

/// The explorer's gestures on a repo row: one press previews the repo in the
/// Repositories tab, a second press (a double click) keeps it in a tab of its
/// own, and the middle button opens a tab without a preview step.
#[gpui::test]
fn a_double_click_keeps_a_repo_and_the_middle_click_opens_its_tab(cx: &mut TestAppContext) {
    let (store, cx) = two_repos(cx);
    draw(cx);

    double_click(cx, "repo-row-1");
    assert_eq!(
        active_tab(&store, cx).as_deref(),
        Some("/probe/beta"),
        "the second press keeps the repo in its own tab"
    );
    assert_eq!(cx.read_entity(&store, |s, _| s.repo_tabs.len()), 1);

    click(cx, "repo-tab-home");
    assert_eq!(active_tab(&store, cx), None, "back on the list");

    middle_click(cx, "repo-row-0");
    assert_eq!(
        active_tab(&store, cx).as_deref(),
        Some("/probe/alpha"),
        "the middle button opens a tab"
    );
    assert_eq!(cx.read_entity(&store, |s, _| s.repo_tabs.len()), 2);
}

/// The same second press works from inside a tab: the first press previews
/// the repo back in the Repositories tab, the second keeps it.
#[gpui::test]
fn a_double_click_inside_a_tab_still_keeps_the_repo(cx: &mut TestAppContext) {
    let (store, cx) = two_repos(cx);
    cx.simulate_keystrokes("ctrl-t");
    draw(cx);
    assert_eq!(active_tab(&store, cx).as_deref(), Some("/probe/alpha"));
    click(cx, "list-toggle");

    double_click(cx, "repo-row-1");
    assert_eq!(
        active_tab(&store, cx).as_deref(),
        Some("/probe/beta"),
        "the second press keeps beta in its own tab"
    );
    assert_eq!(cx.read_entity(&store, |s, _| s.repo_tabs.len()), 2);
}

#[gpui::test]
fn a_repo_leaving_the_list_takes_its_tab_along(cx: &mut TestAppContext) {
    let (store, cx) = two_repos(cx);
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.open_repo_tab("/probe/alpha", cx);
        s.replace_repos(vec![entry("beta", "/probe/beta")]);
        s.ensure_row_selected(cx);
    });
    draw(cx);
    assert_eq!(cx.read_entity(&store, |s, _| s.repo_tabs.len()), 0);
    assert_eq!(active_tab(&store, cx), None);
    assert_eq!(selected(&store, cx).as_deref(), Some("/probe/beta"));
}

#[gpui::test]
fn the_tab_keys_walk_the_strip(cx: &mut TestAppContext) {
    let (store, cx) = two_repos(cx);
    cx.simulate_keystrokes("ctrl-t");
    draw(cx);
    assert_eq!(
        active_tab(&store, cx).as_deref(),
        Some("/probe/alpha"),
        "Ctrl+T opens the selected repo"
    );
    cx.simulate_keystrokes("ctrl-1");
    draw(cx);
    assert_eq!(active_tab(&store, cx), None, "Ctrl+1 is Repositories");
    cx.simulate_keystrokes("ctrl-tab");
    draw(cx);
    assert!(active_tab(&store, cx).is_some(), "Ctrl+Tab walks the strip");
    cx.simulate_keystrokes("ctrl-w");
    draw(cx);
    assert_eq!(cx.read_entity(&store, |s, _| s.repo_tabs.len()), 0);
}

#[gpui::test]
fn the_list_toggle_follows_the_view_it_is_on(cx: &mut TestAppContext) {
    let (_store, cx) = two_repos(cx);
    click(cx, "list-toggle");
    assert!(cx.debug_bounds("repos-list-pane").is_none(), "hidden");
    click(cx, "list-toggle");
    assert!(cx.debug_bounds("repos-list-pane").is_some(), "shown");

    cx.simulate_keystrokes("ctrl-t");
    draw(cx);
    assert!(
        cx.debug_bounds("repos-list-pane").is_none(),
        "a tab hides it"
    );
    click(cx, "list-toggle");
    assert!(
        cx.debug_bounds("repos-list-pane").is_some(),
        "and the toggle brings it back"
    );
}

#[gpui::test]
fn the_rail_moves_between_sections(cx: &mut TestAppContext) {
    let (_store, cx) = two_repos(cx);
    click(cx, "rail-tools");
    assert!(cx.debug_bounds("tool-page").is_some(), "Tools");
    assert!(
        cx.debug_bounds("repo-tab-strip").is_none(),
        "the strip belongs to Repos"
    );
    click(cx, "rail-settings");
    assert!(cx.debug_bounds("settings-detail").is_some(), "Settings");
    click(cx, "rail-summary");
    assert!(cx.debug_bounds("summary-button").is_some(), "Summary");
    click(cx, "rail-repos");
    assert!(cx.debug_bounds("repos-list-pane").is_some(), "Repos");
}

/// The whole last-run arrangement comes back on a restart: the on-screen tab
/// with its resting detail view, the selected row, and the Archive toggle —
/// each while its repo is still waiting for the scan to name it.
#[gpui::test]
fn the_last_arrangement_restores_on_a_restart(cx: &mut TestAppContext) {
    use std::sync::Arc;
    use warp_station_core::settings::RepoTabView;

    let (store, _net_rx) = scratch_store(
        cx,
        "arrange",
        vec![entry("alpha", "/probe/alpha"), entry("beta", "/probe/beta")],
    );
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.open_repo_tab("/probe/beta", cx);
        s.open_detail_tab(WorkspaceTab::Changes, cx);
        s.show_repo_tab(Some("/probe/beta".into()), cx);
        s.toggle_show_archived(cx);
    });
    // The selected-row write is debounced (held arrow keys must not rewrite
    // the settings file per row); let the 300 ms timer land before reading.
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(400));
    cx.run_until_parked();
    let saved = cx.read_entity(&store, |s, _| s.settings().repos.clone());
    assert_eq!(
        saved.open_tab.as_deref(),
        Some("/probe/beta"),
        "the open tab persisted"
    );
    assert_eq!(
        saved.selected_repo.as_deref(),
        Some("/probe/beta"),
        "the selection persisted"
    );
    assert_eq!(
        saved.show_archived,
        Some(true),
        "the archive toggle persisted"
    );
    assert_eq!(
        saved.tab_views,
        Some(vec![RepoTabView {
            folder: "/probe/beta".into(),
            tab: "Changes".into()
        }]),
        "the tab's resting view persisted"
    );

    // a second run over the same settings file, before its scan settles
    let scratch = std::env::temp_dir().join(format!(
        "warp-station-probe-split-arrange-{}.json",
        std::process::id()
    ));
    let (net_tx, _net_rx2) = async_channel::unbounded();
    let store = cx.new(|_cx| {
        let mut store = Store::with_settings(
            net_tx,
            Arc::new(warp_station_core::settings::SettingsService::at(scratch)),
        );
        store.scanning = true;
        store.replace_repos(Vec::new());
        store
    });
    assert!(
        cx.read_entity(&store, |s, _| s.show_archived),
        "the archive toggle reads before the scan lands"
    );
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.ensure_row_selected(cx);
        assert!(s.active_repo_tab.is_none(), "nothing has landed yet");
        assert_eq!(s.selected_repo, None, "no row to stand in yet");

        s.replace_repos(vec![
            entry("alpha", "/probe/alpha"),
            entry("beta", "/probe/beta"),
        ]);
        s.ensure_row_selected(cx);
        assert_eq!(
            s.active_repo_tab.as_deref(),
            Some("/probe/beta"),
            "beta's tab comes back on screen"
        );
        assert_eq!(
            s.detail_tab,
            WorkspaceTab::Changes,
            "on the view it was left on"
        );
        assert_eq!(
            s.selected_repo.as_deref(),
            Some("/probe/beta"),
            "and the selection follows the tab"
        );
    });
}

/// Both platforms' rows in one list: each carries its own reference shape and
/// its own source tag, and one list orders them for you.
#[gpui::test]
fn the_pull_requests_list_unions_the_platforms(cx: &mut TestAppContext) {
    use std::sync::Arc;
    use warp_station_core::azure::{AzureItem, AzureSnapshot};

    let (store, _net_rx) = scratch_store(cx, "union", vec![entry("alpha", "/probe/alpha")]);
    seed_github(&store, "/probe/alpha", 1, 1, cx);
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.update_settings(|settings| {
            settings.repos.enable_azure_devops = true;
            settings.repos.azure_devops_pat = "pat".into();
        });
        let snapshot = AzureSnapshot {
            available: true,
            url: Some("https://dev.azure.com/o/p/_git/r".into()),
            pulls: vec![AzureItem {
                number: 214,
                title: "Sync the bridge".into(),
                state: Some("active".into()),
                source_ref_name: Some("refs/heads/fix/bridge".into()),
                target_ref_name: Some("refs/heads/main".into()),
                ..Default::default()
            }],
            work_items: vec![AzureItem {
                number: 3381,
                title: "Battery drains while syncing".into(),
                state: Some("Active".into()),
                work_item_type: Some("Bug".into()),
                assigned_to: Some("engin".into()),
                ..Default::default()
            }],
            ..Default::default()
        };
        s.azure_by_folder
            .insert("/probe/alpha".to_string(), Arc::new(snapshot));
        cx.notify();
    });
    let cx = open_window(&store, cx);
    draw(cx);
    click(cx, "detail-tab-pulls");

    assert!(cx.debug_bounds("PR-gh-0").is_some(), "the GitHub row");
    assert!(cx.debug_bounds("PR-az-214").is_some(), "the Azure row");
    // one platform alone is what the badge says; a merged list leaves each
    // row's own tag to say it
    let open_count = store.read_with(cx, |s: &Store, _| {
        let snapshot = s.github_by_folder.get("/probe/alpha").unwrap();
        let azure = s.azure_by_folder.get("/probe/alpha").unwrap();
        snapshot.pulls.len() + azure.pulls.len()
    });
    assert_eq!(open_count, 2);

    // the tab strip counts the union, and so does the row
    let (pulls, issues) = store.read_with(cx, |s: &Store, _| {
        let repo = &s.repos[0];
        (repo.pull_total(), repo.issue_total())
    });
    assert_eq!(
        (pulls, issues),
        (0, 0),
        "the counts come from the pass, not the seeded maps"
    );

    click(cx, "detail-tab-issues");
    assert!(
        cx.debug_bounds("Issue-gh-100").is_some(),
        "the GitHub issue"
    );
    assert!(
        cx.debug_bounds("Issue-az-3381").is_some(),
        "the Azure work item"
    );

    // an Azure pull request opens the Azure pane, not the GitHub one
    click(cx, "detail-tab-pulls");
    click(cx, "PR-az-214");
    draw(cx);
    assert!(
        cx.debug_bounds("azure-pull").is_some(),
        "the Azure pane took the column"
    );
    assert!(
        cx.debug_bounds("pull-title").is_none(),
        "the GitHub pane did not"
    );
}

/// Closed asks both platforms once, and a failing one keeps the other's rows
/// with a way to ask again.
#[gpui::test]
fn the_closed_list_asks_both_platforms(cx: &mut TestAppContext) {
    use std::sync::Arc;
    use warp_station_core::azure::AzureSnapshot;

    let (store, net_rx) = scratch_store(cx, "closed-union", vec![entry("alpha", "/probe/alpha")]);
    seed_github(&store, "/probe/alpha", 1, 0, cx);
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.update_settings(|settings| {
            settings.repos.enable_azure_devops = true;
            settings.repos.azure_devops_pat = "pat".into();
        });
        s.azure_by_folder.insert(
            "/probe/alpha".to_string(),
            Arc::new(AzureSnapshot {
                available: true,
                ..Default::default()
            }),
        );
        cx.notify();
    });
    use_a_located_gh(&store, cx);
    let cx = open_window(&store, cx);
    draw(cx);
    click(cx, "detail-tab-pulls");
    while net_rx.try_recv().is_ok() {}

    click(cx, "pr-state-closed");
    draw(cx);
    let mut github = 0;
    let mut azure = 0;
    while let Ok(command) = net_rx.try_recv() {
        match command {
            NetCommand::GitHubClosed { .. } => github += 1,
            NetCommand::AzureClosed { .. } => azure += 1,
            _ => {}
        }
    }
    assert_eq!(github, 1, "one ask per platform, however many frames draw");
    assert_eq!(azure, 1, "the Azure half is asked too");

    // GitHub answers; Azure fails. The row stays, and the note says which
    // platform did not, with Retry.
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.on_net_event(
            UiEvent::GitHubClosedLoaded {
                folder: "/probe/alpha".into(),
                pull_requests: true,
                items: Some(vec![github_item(7, true)]),
            },
            cx,
        );
        s.on_net_event(
            UiEvent::AzureClosedLoaded {
                folder: "/probe/alpha".into(),
                pull_requests: true,
                items: None,
            },
            cx,
        );
    });
    draw(cx);
    assert!(
        cx.debug_bounds("PR-gh-7").is_some(),
        "the answered rows stay"
    );
    assert!(
        cx.debug_bounds("closed-retry-note").is_some(),
        "the failed half is named beside the rows"
    );
}
