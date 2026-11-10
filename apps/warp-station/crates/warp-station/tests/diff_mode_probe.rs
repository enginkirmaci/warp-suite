//! The Changes diff's header toggle and its persistence: the icon button
//! switches unified/split, the pick lands in `changes.diff_view`, and a fresh
//! view over the same settings file starts in the saved mode.

use gpui::prelude::*;
use gpui::{
    Modifiers, ScrollDelta, ScrollEvent, ScrollSource, TestAppContext, TouchPhase, point, px,
};
use std::path::PathBuf;
use std::sync::Arc;
use warp_station::store::{Store, WorkspaceTab};
use warp_station::ui::changes::ChangesView;
use warp_station_core::model::{GitChangeSnapshot, GitChangedFile};
use warp_station_core::repo_store::RepoEntry;
use warp_station_core::settings::{DiffViewMode, SettingsService};

const FILE_PATH: &str = "src/x.rs";
const PATCH: &str = "\
--- a/src/x.rs
+++ b/src/x.rs
@@ -1,3 +1,3 @@
 fn a() {}
-fn b() {}
+fn b() { let x = 1; }
 fn c() {}
";

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "warp-station-diff-mode-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("settings.json");
    std::fs::write(&path, "{}").unwrap();
    path
}

fn repo() -> RepoEntry {
    RepoEntry {
        name: "alpha".into(),
        folder_path: PathBuf::from("/probe/alpha"),
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
        azure_work_item_count: 0,
    }
}

fn changed_file() -> GitChangedFile {
    GitChangedFile {
        path: FILE_PATH.into(),
        status_code: "M".into(),
        additions: Some(1),
        deletions: Some(1),
    }
}

fn snapshot() -> GitChangeSnapshot {
    let file = changed_file();
    GitChangeSnapshot {
        status_files: vec![file.clone()],
        staged_files: Vec::new(),
        unstaged_files: vec![file],
        worktree_counts: Default::default(),
        cached_counts: Default::default(),
    }
}

fn seeded_store(cx: &mut TestAppContext, settings: PathBuf) -> gpui::Entity<Store> {
    let (net_tx, _net_rx) = async_channel::unbounded();
    cx.new(|cx| {
        let mut store = Store::with_settings(net_tx, Arc::new(SettingsService::at(settings)));
        store.replace_repos(vec![repo()]);
        store.select_repo("/probe/alpha", cx);
        store.open_detail_tab(WorkspaceTab::Changes, cx);
        store.changes_loaded_for = Some("/probe/alpha".into());
        store.set_changes(Some(snapshot()));
        store
    })
}

fn draw(vcx: &mut gpui::VisualTestContext) {
    vcx.run_until_parked();
    vcx.update(|window, cx| drop(window.draw(cx)));
}

/// The first draw installs the snapshot (which clears stale patches) and
/// selects the file; the patch then arrives as the second draw's drain.
fn inject_patch(vcx: &mut gpui::VisualTestContext, store: &gpui::Entity<Store>) {
    inject_patch_text(vcx, store, PATCH);
}

fn inject_patch_text(vcx: &mut gpui::VisualTestContext, store: &gpui::Entity<Store>, patch: &str) {
    let store = store.clone();
    let patch = patch.to_string();
    vcx.update(|_, cx| {
        store.update(cx, |store, _| {
            store.change_patch_result = Some((FILE_PATH.into(), false, patch));
        });
    });
    draw(vcx);
}

fn click(vcx: &mut gpui::VisualTestContext, selector: &'static str) {
    let bounds = vcx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} was not drawn"));
    vcx.simulate_click(
        point(
            bounds.origin.x + bounds.size.width / 2.,
            bounds.origin.y + bounds.size.height / 2.,
        ),
        Modifiers::default(),
    );
    vcx.run_until_parked();
    vcx.update(|window, cx| drop(window.draw(cx)));
}

#[gpui_kit::gpui::test]
fn the_toggles_switch_and_save_the_layout(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);
    let settings = scratch("toggle");
    let store = seeded_store(cx, settings.clone());
    let patch_store = store.clone();
    let (_window, vcx) = cx.add_window_view(|window, cx| ChangesView::new(store, window, cx));
    draw(vcx);
    inject_patch(vcx, &patch_store);

    assert!(
        vcx.debug_bounds("diff-view-toggle").is_some(),
        "the header carries the mode button"
    );
    assert!(
        vcx.debug_bounds("diff-text").is_some(),
        "a fresh view starts unified"
    );
    assert!(
        vcx.debug_bounds("diff-split").is_none(),
        "no split body until the button is pressed"
    );
    assert!(
        vcx.debug_bounds("diff-scrollbars").is_some(),
        "the unwrapped diff carries a pinned horizontal bar"
    );

    click(vcx, "diff-view-toggle");

    assert!(
        vcx.debug_bounds("diff-split").is_some(),
        "the click draws the split body"
    );
    assert!(
        vcx.debug_bounds("diff-left-text").is_some(),
        "the old side is a real column"
    );
    assert!(
        vcx.debug_bounds("diff-right-text").is_some(),
        "the new side is a real column"
    );
    assert!(
        vcx.debug_bounds("diff-text").is_none(),
        "the unified body is gone"
    );
    assert!(
        vcx.debug_bounds("diff-scrollbars").is_some(),
        "split keeps the pinned bars"
    );

    assert!(
        vcx.debug_bounds("diff-wrap-toggle").is_some(),
        "the header carries the word-wrap button"
    );
    click(vcx, "diff-wrap-toggle");

    assert!(
        vcx.debug_bounds("diff-wrap-body").is_some(),
        "the click draws the wrapped body"
    );
    let body = vcx.debug_bounds("diff-wrap-body").unwrap();
    assert!(
        body.size.height > px(50.),
        "the wrapped body fills the pane: {body:?}"
    );
    assert!(
        vcx.debug_bounds("diff-wrap-code-0-l").is_some(),
        "the wrapped list draws its first row"
    );
    assert!(
        vcx.debug_bounds("diff-left-text").is_none(),
        "wrapped rows are not the one-text columns"
    );
    assert!(
        vcx.debug_bounds("diff-scrollbars").is_none(),
        "wrap has no horizontal bar to pin"
    );

    let reloaded = SettingsService::at(settings.clone());
    assert_eq!(
        reloaded.get().changes.diff_view,
        DiffViewMode::Split,
        "the layout pick is written to settings"
    );
    assert!(
        reloaded.get().changes.diff_wrap,
        "the wrap pick is written to settings"
    );

    // a new view over the same file restores both choices without a click
    let store = seeded_store(cx, settings);
    let patch_store = store.clone();
    let (_window, vcx) = cx.add_window_view(|window, cx| ChangesView::new(store, window, cx));
    draw(vcx);
    inject_patch(vcx, &patch_store);
    assert!(
        vcx.debug_bounds("diff-wrap-body").is_some(),
        "the next view starts wrapped"
    );
    assert!(
        vcx.debug_bounds("diff-split").is_none(),
        "the wrapped split body is the row renderer, not the one-text columns"
    );
    assert!(
        vcx.debug_bounds("diff-text").is_none(),
        "the next view keeps split, not unified"
    );
    assert!(
        vcx.debug_bounds("diff-scrollbars").is_none(),
        "the next view keeps wrap off the bar row"
    );
}

#[gpui_kit::gpui::test]
fn a_large_wrapped_diff_virtualises_its_rows_and_scrolls(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);
    let settings = scratch("virtual");
    let store = seeded_store(cx, settings);
    let patch_store = store.clone();
    let (_window, vcx) = cx.add_window_view(|window, cx| ChangesView::new(store, window, cx));
    draw(vcx);

    let mut patch = String::from("--- a/src/x.rs\n+++ b/src/x.rs\n@@ -1,400 +1,400 @@\n");
    for index in 0..400 {
        patch.push_str(&format!("+row {index}: {}\n", "word ".repeat(30)));
    }
    inject_patch_text(vcx, &patch_store, &patch);

    click(vcx, "diff-wrap-toggle");
    assert!(
        vcx.debug_bounds("diff-wrap-code-0-u").is_some(),
        "the first wrapped row is drawn"
    );
    assert!(
        vcx.debug_bounds("diff-wrap-code-399-u").is_none(),
        "the list does not build every row"
    );

    let bounds = vcx.debug_bounds("diff-wrap-code-0-u").unwrap();
    let at = point(bounds.origin.x + px(20.), bounds.origin.y + px(8.));
    for _ in 0..40 {
        vcx.simulate_event(ScrollEvent {
            position: at,
            delta: ScrollDelta::Pixels(point(px(0.), px(-120.))),
            modifiers: Modifiers::default(),
            touch_phase: TouchPhase::Moved,
            source: ScrollSource::Pointer,
        });
    }
    draw(vcx);

    assert!(
        vcx.debug_bounds("diff-wrap-code-0-u").is_none(),
        "the wheel scrolled the wrapped body"
    );
    assert!(
        vcx.debug_bounds("diff-wrap-code-399-u").is_none(),
        "scrolling still draws only the visible rows"
    );
}

#[gpui_kit::gpui::test]
fn a_horizontal_wheel_scrolls_only_the_unwrapped_code(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);
    let settings = scratch("wheel");
    let store = seeded_store(cx, settings);
    let patch_store = store.clone();
    let (_window, vcx) = cx.add_window_view(|window, cx| ChangesView::new(store, window, cx));
    draw(vcx);

    let mut patch = String::from("--- a/src/x.rs\n+++ b/src/x.rs\n@@ -1,300 +1,300 @@\n");
    patch.push_str(&format!(
        "+fn wide() {{ let x = \"{}\"; }}\n",
        "a".repeat(300)
    ));
    for index in 1..300 {
        patch.push_str(&format!("+fn row_{index}() {{}}\n"));
    }
    inject_patch_text(vcx, &patch_store, &patch);

    let before = vcx.debug_bounds("diff-text").expect("the code column");
    let at = point(before.origin.x + px(30.), before.origin.y + px(30.));
    scroll(vcx, at, point(px(-120.), px(0.)));
    let horizontal = vcx.debug_bounds("diff-text").expect("the code column");
    assert!(
        horizontal.origin.x < before.origin.x,
        "the horizontal wheel moved the code: {horizontal:?} vs {before:?}"
    );
    assert_eq!(
        horizontal.origin.y, before.origin.y,
        "the horizontal wheel must not move the body vertically"
    );

    scroll(vcx, at, point(px(0.), px(-120.)));
    let vertical = vcx.debug_bounds("diff-text").expect("the code column");
    assert!(
        vertical.origin.y < horizontal.origin.y,
        "the vertical wheel moved the body: {vertical:?} vs {horizontal:?}"
    );
    assert_eq!(
        vertical.origin.x, horizontal.origin.x,
        "the vertical wheel must not move the code horizontally"
    );
}

fn scroll(
    vcx: &mut gpui::VisualTestContext,
    at: gpui::Point<gpui::Pixels>,
    delta: gpui::Point<gpui::Pixels>,
) {
    vcx.simulate_event(ScrollEvent {
        position: at,
        delta: ScrollDelta::Pixels(delta),
        modifiers: Modifiers::default(),
        touch_phase: TouchPhase::Moved,
        source: ScrollSource::Pointer,
    });
    draw(vcx);
}
