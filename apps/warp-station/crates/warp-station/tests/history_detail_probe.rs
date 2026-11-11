//! The History tab's commit detail: the head band over a file rail and the
//! selected file's diff. The diff is selectable text, a plain press in it must
//! not change the selection, and a repository with no commits settles into the
//! design's empty state instead of "Loading history…" forever.

use gpui::prelude::*;
use gpui::{
    Context, Entity, Modifiers, MouseButton, Render, TestAppContext, VisualTestContext, Window,
    div, point, px,
};
use std::path::PathBuf;
use std::sync::Arc;
use warp_station::net::{NetCommand, UiEvent};
use warp_station::store::{Store, WorkspaceTab};
use warp_station::ui::changes::ChangesView;
use warp_station_core::model::{GitChangedFile, GitCommitDetails, GitCommitInfo};
use warp_station_core::repo_store::RepoEntry;
use warp_station_core::settings::SettingsService;

const HASH: &str = "6272a3c0000000000000000000000000000000000";
const FILE: &str = "src/x.rs";
const OTHER_FILE: &str = "Cargo.toml";
const PATCH: &str = "\
--- a/src/x.rs
+++ b/src/x.rs
@@ -1,3 +1,3 @@
 fn a() {}
-fn b() {}
+fn b() { let x = 1; }
 fn c() {}
";
const OTHER_PATCH: &str = "\
--- a/Cargo.toml
+++ b/Cargo.toml
@@ -1,2 +1,3 @@
 [package]
+version = \"0.2.0\"
";

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "warp-station-history-detail-{name}-{}",
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
        github_web_url: None,
        azure_work_item_count: 0,
    }
}

fn changed_file(path: &str, additions: u32, deletions: u32) -> GitChangedFile {
    GitChangedFile {
        path: path.into(),
        status_code: "M".into(),
        additions: Some(additions),
        deletions: Some(deletions),
    }
}

struct Host {
    changes: Entity<ChangesView>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(gpui_kit::base::TextSelectionLayer)
            .child(self.changes.clone().into_any_element())
    }
}

fn draw(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    vcx.update(|window, cx| drop(window.draw(cx)));
}

fn click(vcx: &mut VisualTestContext, selector: &'static str) {
    let bounds = vcx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} was not drawn"));
    vcx.simulate_click(bounds.center(), Modifiers::default());
    draw(vcx);
}

#[gpui_kit::gpui::test]
fn the_selected_file_diff_stays_selectable(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let settings = scratch("split");
    let (net_tx, net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| {
        let mut store = Store::with_settings(net_tx, Arc::new(SettingsService::at(settings)));
        store.replace_repos(vec![repo()]);
        store.select_repo("/probe/alpha", cx);
        store.set_history(vec![GitCommitInfo {
            hash: HASH.into(),
            subject: "fix: split the history detail".into(),
            author: Some("Probe Author".into()),
            date: chrono::Local::now(),
        }]);
        store.open_detail_tab(WorkspaceTab::History, cx);
        store
    });
    store.update(cx, |store, _| {
        store.commit_details = Some(std::sync::Arc::new(GitCommitDetails {
            hash: HASH.into(),
            files: vec![changed_file(FILE, 1, 1), changed_file(OTHER_FILE, 1, 0)],
            body: Some("The body stays readable too.".into()),
        }));
        store.commit_file_patches = vec![(format!("{HASH}|{FILE}"), PATCH.into())];
    });

    let (_host, vcx) = cx.add_window_view(|window, cx| {
        let changes = cx.new(|cx| ChangesView::new(store.clone(), window, cx));
        Host { changes }
    });
    draw(vcx);
    draw(vcx);

    assert!(
        vcx.debug_bounds("commit-subject").is_some(),
        "the head band names the commit it shows"
    );
    for selector in ["detail-copy-sha", "detail-revert", "detail-checkout"] {
        assert!(
            vcx.debug_bounds(selector).is_some(),
            "the head band keeps {selector}"
        );
    }
    assert!(
        vcx.debug_bounds("commit-file-0").is_some() && vcx.debug_bounds("commit-file-1").is_some(),
        "the rail lists every file in the commit"
    );
    assert!(
        vcx.debug_bounds("commit-detail-patch").is_none(),
        "the old inline patch fold is gone"
    );
    assert!(
        vcx.debug_bounds("commit-diff").is_some(),
        "the selected file gets the diff pane"
    );
    let patch = vcx
        .debug_bounds("diff-text")
        .expect("the selected file's patch is drawn");

    // A plain click in the diff body is a press into selectable text, not a
    // file selection: the rail and the diff must stay where they are.
    vcx.simulate_click(patch.center(), Modifiers::default());
    draw(vcx);
    assert!(
        vcx.debug_bounds("diff-text").is_some(),
        "a click inside the diff keeps it on screen"
    );

    // Dragging across a line selects it; the mouse-up is still inside the
    // diff, so it must not reach the rail either.
    // drag inside the diff's own text column, whose origin moves with the
    // code font: a fixed offset from the card would land in the gutter once
    // the diff drew one step smaller
    let text = vcx
        .debug_bounds("diff-text")
        .expect("the patch draws its code column");
    let line = text.size.height / 4.;
    let start = point(text.origin.x + px(2.), text.origin.y + line * 1.5);
    let end = point(text.origin.x + px(120.), text.origin.y + line * 1.5);
    vcx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
    vcx.simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::default());
    vcx.simulate_mouse_up(end, MouseButton::Left, Modifiers::default());
    draw(vcx);

    let selected = vcx.update(gpui_kit::base::TextSelection::selected_text);
    assert!(
        selected.contains("fn a()") || selected.contains("fn b()"),
        "the drag selects the diff's code: {selected:?}"
    );

    // Pick the other file: the rail's selection moves and its patch is put on
    // the wire, which is what the store's answer below fills.
    click(vcx, "commit-file-1");
    let asked = std::iter::from_fn(|| net_rx.try_recv().ok()).any(|command| {
        matches!(
            command,
            NetCommand::GetCommitFilePatch { path, .. } if path == OTHER_FILE
        )
    });
    assert!(asked, "selecting a rail row asks for that file's patch");

    vcx.update_entity(&store, |store, _| {
        store.commit_file_patches = vec![(format!("{HASH}|{OTHER_FILE}"), OTHER_PATCH.into())];
    });
    draw(vcx);
    draw(vcx);
    assert!(
        vcx.debug_bounds("diff-text").is_some(),
        "the second file's patch draws in the same pane"
    );
    assert!(
        vcx.debug_bounds("commit-diff").is_some(),
        "the diff pane survives the selection"
    );
}

#[gpui_kit::gpui::test]
fn a_repository_with_no_commits_shows_the_empty_state(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let settings = scratch("empty");
    let (net_tx, net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| {
        let mut store = Store::with_settings(net_tx, Arc::new(SettingsService::at(settings)));
        store.replace_repos(vec![repo()]);
        store.select_repo("/probe/alpha", cx);
        store.open_detail_tab(WorkspaceTab::History, cx);
        store
    });

    let (_host, vcx) = cx.add_window_view(|window, cx| {
        let changes = cx.new(|cx| ChangesView::new(store.clone(), window, cx));
        Host { changes }
    });
    draw(vcx);

    assert!(
        vcx.debug_bounds("history-loading").is_some(),
        "the tab says it is loading the first time it opens"
    );
    assert!(
        vcx.debug_bounds("history-empty").is_none(),
        "an in-flight pass is not an empty repository"
    );
    let asked = std::iter::from_fn(|| net_rx.try_recv().ok())
        .any(|command| matches!(command, NetCommand::LoadHistory { .. }));
    assert!(asked, "opening History asks the net thread for a pass");

    // The pass settles with no commits: emptiness is now an answer.
    vcx.update_entity(&store, |store, cx| {
        store.on_net_event(
            UiEvent::HistoryLoaded {
                folder: "/probe/alpha".into(),
                commits: Vec::new(),
                unpushed: None,
            },
            cx,
        );
    });
    draw(vcx);

    assert!(
        vcx.debug_bounds("history-empty").is_some(),
        "a settled empty history shows the design's empty state"
    );
    assert!(
        vcx.debug_bounds("history-loading").is_none(),
        "nothing is loading once the pass settles"
    );
}

/// A commit that touched a thousand files must not lay out every rail row:
/// the file rail is a `uniform_list`, so only the viewport is built. The first
/// row exists and the thousandth cannot.
#[gpui_kit::gpui::test]
fn a_huge_commit_rail_draws_only_the_viewport(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let settings = scratch("rail-10k");
    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| {
        let mut store = Store::with_settings(net_tx, Arc::new(SettingsService::at(settings)));
        store.replace_repos(vec![repo()]);
        store.select_repo("/probe/alpha", cx);
        store.set_history(vec![GitCommitInfo {
            hash: HASH.into(),
            subject: "refactor: sweep every file".into(),
            author: Some("Probe Author".into()),
            date: chrono::Local::now(),
        }]);
        store.open_detail_tab(WorkspaceTab::History, cx);
        store
    });
    store.update(cx, |store, _| {
        store.commit_details = Some(Arc::new(GitCommitDetails {
            hash: HASH.into(),
            files: (0..1000)
                .map(|ix| changed_file(&format!("src/f{ix:04}.rs"), 1, 0))
                .collect(),
            body: None,
        }));
    });

    let (_host, vcx) = cx.add_window_view(|window, cx| {
        let changes = cx.new(|cx| ChangesView::new(store.clone(), window, cx));
        Host { changes }
    });
    draw(vcx);
    draw(vcx);

    assert!(
        vcx.debug_bounds("commit-file-0").is_some(),
        "the first rail row renders"
    );
    assert!(
        vcx.debug_bounds("commit-file-999").is_none(),
        "the last rail row is off screen and must not be built"
    );
}
