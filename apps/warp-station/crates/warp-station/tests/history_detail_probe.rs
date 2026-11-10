//! The History tab's commit detail: a file's title row folds its patch, while
//! the patch itself is selectable text. A press or a selection drag inside the
//! diff must not reach the fold toggle (it used to sit on the whole row).

use gpui::prelude::*;
use gpui::{
    Context, Entity, Modifiers, MouseButton, Render, TestAppContext, VisualTestContext, Window,
    div, point, px,
};
use std::path::PathBuf;
use std::sync::Arc;
use warp_station::store::{Store, WorkspaceTab};
use warp_station::ui::changes::ChangesView;
use warp_station_core::model::{GitChangedFile, GitCommitDetails, GitCommitInfo};
use warp_station_core::repo_store::RepoEntry;
use warp_station_core::settings::SettingsService;

const HASH: &str = "6272a3c0000000000000000000000000000000000";
const FILE: &str = "src/x.rs";
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
        azure_work_item_count: 0,
    }
}

fn changed_file() -> GitChangedFile {
    GitChangedFile {
        path: FILE.into(),
        status_code: "M".into(),
        additions: Some(1),
        deletions: Some(1),
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
fn the_diff_body_stays_open_and_selectable(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let settings = scratch("fold");
    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| {
        let mut store = Store::with_settings(net_tx, Arc::new(SettingsService::at(settings)));
        store.replace_repos(vec![repo()]);
        store.select_repo("/probe/alpha", cx);
        store.set_history(vec![GitCommitInfo {
            hash: HASH.into(),
            subject: "fix: keep the diff open".into(),
            author: Some("Probe Author".into()),
            date: chrono::Local::now(),
        }]);
        store.open_detail_tab(WorkspaceTab::History, cx);
        store
    });
    store.update(cx, |store, _| {
        store.commit_details = Some(GitCommitDetails {
            hash: HASH.into(),
            files: vec![changed_file()],
            body: Some("The body stays readable too.".into()),
        });
        store.commit_file_patch = Some((format!("{HASH}|{FILE}"), PATCH.into()));
    });

    let (_host, vcx) = cx.add_window_view(|window, cx| {
        let changes = cx.new(|cx| ChangesView::new(store.clone(), window, cx));
        Host { changes }
    });
    draw(vcx);

    assert!(
        vcx.debug_bounds("commit-detail-file").is_some(),
        "the commit detail lists its changed file"
    );
    assert!(
        vcx.debug_bounds("commit-detail-patch").is_none(),
        "the patch starts folded"
    );

    click(vcx, "commit-detail-file");
    let patch = vcx
        .debug_bounds("commit-detail-patch")
        .expect("the title row opens the patch");

    // A plain click in the diff body is a press into selectable text, not a
    // fold: this is the reported bug.
    vcx.simulate_click(patch.center(), Modifiers::default());
    draw(vcx);
    assert!(
        vcx.debug_bounds("commit-detail-patch").is_some(),
        "a click inside the diff must not fold it"
    );

    // Dragging across a line selects it; the mouse-up is still inside the
    // diff, so it must not reach the file's toggle either.
    let patch = vcx
        .debug_bounds("commit-detail-patch")
        .expect("the patch is still open");
    let start = point(patch.origin.x + px(36.), patch.origin.y + px(30.));
    let end = point(patch.origin.x + px(220.), patch.origin.y + px(30.));
    vcx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
    vcx.simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::default());
    vcx.simulate_mouse_up(end, MouseButton::Left, Modifiers::default());
    draw(vcx);

    let selected = vcx.update(gpui_kit::base::TextSelection::selected_text);
    assert!(
        selected.contains("fn a()") || selected.contains("fn b()"),
        "the drag selects the diff's code: {selected:?}"
    );
    assert!(
        vcx.debug_bounds("commit-detail-patch").is_some(),
        "a selection drag leaves the diff open"
    );

    // The title row is still the one control that folds it.
    click(vcx, "commit-detail-file");
    assert!(
        vcx.debug_bounds("commit-detail-patch").is_none(),
        "clicking the title row folds the patch"
    );
}
