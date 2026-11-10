//! Headless probes for the Overview's README card: its rendered document is
//! selectable text, and the window's Copy shortcut reaches that selection.

use gpui::TestAppContext;
use gpui::prelude::*;
use std::path::PathBuf;
use warp_station::store::Store;
use warp_station::ui::window::MainWindow;
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
        git_has_remote: false,
        git_branch_published: true,
        git_status_loaded: true,
        github_pr_count: 0,
        github_issue_count: 0,
        azure_pull_count: 0,
        azure_work_item_count: 0,
    }
}

fn draw(cx: &mut gpui::VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
}

fn scratch_store(
    cx: &mut TestAppContext,
    name: &str,
    repos: Vec<RepoEntry>,
) -> gpui::Entity<Store> {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let scratch = std::env::temp_dir().join(format!(
        "warp-station-probe-readme-{name}-{}.json",
        std::process::id()
    ));
    std::fs::write(&scratch, "{}").expect("scratch settings");
    let (net_tx, _net_rx) = async_channel::unbounded();
    cx.new(|_cx| {
        let mut store = Store::with_settings(
            net_tx,
            std::sync::Arc::new(warp_station_core::settings::SettingsService::at(scratch)),
        );
        store.group_by_root = false;
        store.replace_repos(repos);
        store
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

/// A repo whose README is short enough to sit inside the card untruncated.
fn readme_repo(name: &str) -> String {
    let repo = std::env::temp_dir().join(format!(
        "warp-station-probe-readme-{name}-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&repo).expect("scratch repo");
    std::fs::write(
        repo.join("README.md"),
        "# Alpha\n\nthe readme has selectable words here\n",
    )
    .expect("scratch readme");
    repo.to_string_lossy().to_string()
}

/// Drags from the first body line to the middle of it, inside the card.
fn drag_over_the_readme(cx: &mut gpui::VisualTestContext) {
    let pane = cx
        .debug_bounds("readme-pane")
        .expect("the README pane renders");
    let start = gpui::point(pane.origin.x + gpui::px(24.), pane.origin.y + gpui::px(55.));
    let end = gpui::point(
        pane.origin.x + gpui::px(400.),
        pane.origin.y + gpui::px(75.),
    );
    cx.simulate_mouse_down(start, gpui::MouseButton::Left, gpui::Modifiers::default());
    cx.simulate_mouse_move(
        end,
        Some(gpui::MouseButton::Left),
        gpui::Modifiers::default(),
    );
    cx.simulate_mouse_up(end, gpui::MouseButton::Left, gpui::Modifiers::default());
    draw(cx);
}

#[gpui::test]
fn a_drag_over_the_readme_selects_text(cx: &mut TestAppContext) {
    let folder = readme_repo("drag");
    let store = scratch_store(cx, "drag", vec![entry("alpha", &folder)]);
    let cx = open_window(&store, cx);
    draw(cx);
    assert!(
        cx.debug_bounds("readme-card").is_some(),
        "the README card lives on Overview"
    );

    drag_over_the_readme(cx);

    let selected = cx.update(|_, cx| warpui_kit_markdown::read_only_selection_text(cx));
    assert!(
        selected
            .as_deref()
            .is_some_and(|text| text.contains("selectable words")),
        "a drag over the README selects its text: {selected:?}"
    );
}

/// The document surface takes no focus, so its own Copy action never runs:
/// the window's Ctrl+C hands the selection to the clipboard instead.
#[gpui::test]
fn ctrl_c_copies_the_readme_selection(cx: &mut TestAppContext) {
    let folder = readme_repo("copy");
    let store = scratch_store(cx, "copy", vec![entry("alpha", &folder)]);
    let cx = open_window(&store, cx);
    draw(cx);

    drag_over_the_readme(cx);
    cx.simulate_keystrokes("ctrl-c");

    let copied = cx.read_from_clipboard().and_then(|item| item.text());
    assert!(
        copied
            .as_deref()
            .is_some_and(|text| text.contains("selectable words")),
        "Ctrl+C copies the README selection: {copied:?}"
    );
}
