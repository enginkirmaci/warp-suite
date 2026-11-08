//! The Changes diff's header toggle and its persistence: the icon button
//! switches unified/split, the pick lands in `changes.diff_view`, and a fresh
//! view over the same settings file starts in the saved mode.

use gpui::prelude::*;
use gpui::{Modifiers, TestAppContext, point};
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
    let store = store.clone();
    vcx.update(|_, cx| {
        store.update(cx, |store, _| {
            store.change_patch_result = Some((FILE_PATH.into(), false, PATCH.into()));
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
        vcx.debug_bounds("diff-wrap-toggle").is_some(),
        "the header carries the word-wrap button"
    );
    click(vcx, "diff-wrap-toggle");

    assert!(
        vcx.debug_bounds("diff-wrap-body").is_some(),
        "the click draws the wrapped body"
    );
    assert!(
        vcx.debug_bounds("diff-left-text").is_none(),
        "wrapped rows are not the one-text columns"
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
}
