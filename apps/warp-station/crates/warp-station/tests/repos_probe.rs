//! Headless probes for the repos page's flows: real Store + MainWindow pairs
//! drawn in a test window, covering the toasts, the sheet's branch picker, the
//! background passes, the dialogs and the folder grouping.

use gpui::TestAppContext;
use gpui::prelude::*;
use std::path::PathBuf;
use warp_station::store::Store;
use warp_station::store::projection::ProjectedRow;
use warp_station::ui::window::MainWindow;
use warp_station_core::repo_store::RepoEntry;

/// Points HOME at a scratch folder: `Store::new` reads the settings file under
/// it, and what a probe saves must never reach the developer's real one.
fn scratch_home() {
    static HOME: std::sync::Once = std::sync::Once::new();
    HOME.call_once(|| {
        let home =
            std::env::temp_dir().join(format!("warp-station-repos-probe-{}", std::process::id()));
        std::fs::create_dir_all(home.join(".warp")).expect("scratch home");
        // SAFETY: process-global env; every probe here calls this first.
        // USERPROFILE too: Windows resolves the data root from it first, and
        // a HOME-only redirect leaves the real ~/.warp live for a save.
        unsafe {
            std::env::set_var("HOME", &home);
            #[cfg(windows)]
            std::env::set_var("USERPROFILE", &home);
        };
    });
}

fn entry(name: &str, path: &str, tags: &[&str]) -> RepoEntry {
    RepoEntry {
        name: name.into(),
        folder_path: PathBuf::from(path),
        solution_path: None,
        tags: tags.iter().map(|t| t.to_string()).collect(),
        git_branch_name: Some("main".into()),
        git_modified_count: 3,
        git_to_push_count: 1,
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

fn fake_store(cx: &mut TestAppContext) -> gpui::Entity<Store> {
    let (net_tx, _net_rx) = async_channel::unbounded();
    cx.new(|cx| {
        let mut store = Store::new(cx, net_tx);
        // pin the flat layout: the developer's live settings must never
        // re-shape the probes' row math (the grouping probes seed their own
        // scratch-settings store)
        store.group_by_root = false;
        store.replace_repos(vec![
            entry("alpha", "/probe/alpha", &["favorites"]),
            entry("beta", "/probe/beta", &[]),
            entry("gamma", "/probe/gamma", &[]),
        ]);
        store
    })
}

/// App alerts surface as toasts in the window's bottom-right corner, not as
/// inline header text: a store push renders a severity card, and its close
/// button removes exactly that card.
#[gpui::test]
fn toasts_render_stack_and_dismiss(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    scratch_home();
    cx.update(warp_station::theme::apply_kit_theme);

    let store = fake_store(cx);
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.notify_error("Pull failed for warp-agent: cannot pull with rebase");
        s.notify_success("Fetched alpha");
        cx.notify();
    });

    let store_handle = store.clone();
    let (_window_handle, cx) =
        cx.add_window_view(|window, cx| MainWindow::new(store_handle.clone(), window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    assert!(
        cx.debug_bounds("toast-stack").is_some(),
        "the toast stack renders"
    );
    assert!(
        cx.debug_bounds("toast-0").is_some(),
        "the error toast renders"
    );
    assert!(
        cx.debug_bounds("toast-1").is_some(),
        "the success toast renders"
    );

    let close = cx
        .debug_bounds("toast-close-1")
        .expect("each toast carries a close button");
    cx.simulate_click(close.center(), gpui::Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    assert!(
        cx.debug_bounds("toast-1").is_none(),
        "the dismissed toast is gone"
    );
    assert!(
        cx.debug_bounds("toast-0").is_some(),
        "the sibling toast stays"
    );
}

/// The dropdown groups local branches first, then an inert "Remote" label,
/// then the remote-tracking branches.
#[gpui::test]
fn branch_menu_groups_locals_above_remotes(cx: &mut TestAppContext) {
    use warp_station_core::model::{GitBranchKind, GitBranchRef};

    cx.update(gpui_kit::init);
    scratch_home();
    cx.update(warp_station::theme::apply_kit_theme);

    let store = fake_store(cx);
    let store_handle = store.clone();
    let (_window_handle, cx) =
        cx.add_window_view(|window, cx| MainWindow::new(store_handle.clone(), window, cx));
    cx.run_until_parked();
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.select_repo("/probe/alpha", cx);
        s.open_detail_tab(warp_station::store::WorkspaceTab::Changes, cx);
        s.branches = vec![
            GitBranchRef {
                name: "main".into(),
                kind: GitBranchKind::Local,
            },
            GitBranchRef {
                name: "feature".into(),
                kind: GitBranchKind::Local,
            },
            GitBranchRef {
                name: "origin/main".into(),
                kind: GitBranchKind::Remote,
            },
            GitBranchRef {
                name: "origin/rust".into(),
                kind: GitBranchKind::Remote,
            },
        ];
    });
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    let button = cx
        .debug_bounds("branch-button")
        .expect("the branch selector renders in the rail");
    cx.simulate_click(button.center(), gpui::Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    assert!(
        cx.debug_bounds("branch-menu").is_some(),
        "the dropdown opens"
    );
    let label = cx
        .debug_bounds("branch-group-remote")
        .expect("the Remote group label renders");
    let local = cx
        .debug_bounds("branch-row-main")
        .expect("the local row renders");
    let remote = cx
        .debug_bounds("branch-row-origin/main")
        .expect("the remote row renders");
    assert!(
        local.bottom() <= label.origin.y,
        "locals render above the Remote label"
    );
    assert!(
        label.bottom() <= remote.origin.y,
        "the Remote label renders above the remotes"
    );
}

/// The row trash pins to the row's right edge: hugging the label it lands
/// mid-row, where the hover-faded button intercepts the click meant for the
/// branch name.
#[gpui::test]
fn branch_delete_button_sits_at_the_row_right_edge(cx: &mut TestAppContext) {
    use warp_station_core::model::{GitBranchKind, GitBranchRef};

    cx.update(gpui_kit::init);
    scratch_home();
    cx.update(warp_station::theme::apply_kit_theme);

    let store = fake_store(cx);
    let store_handle = store.clone();
    let (_window_handle, cx) =
        cx.add_window_view(|window, cx| MainWindow::new(store_handle.clone(), window, cx));
    cx.run_until_parked();
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.select_repo("/probe/alpha", cx);
        s.open_detail_tab(warp_station::store::WorkspaceTab::Changes, cx);
        s.branches = vec![
            GitBranchRef {
                name: "main".into(),
                kind: GitBranchKind::Local,
            },
            GitBranchRef {
                name: "feature".into(),
                kind: GitBranchKind::Local,
            },
        ];
    });
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    let button = cx
        .debug_bounds("branch-button")
        .expect("the branch selector renders in the rail");
    cx.simulate_click(button.center(), gpui::Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    let row = cx
        .debug_bounds("branch-row-feature")
        .expect("the row renders");
    let trash = cx
        .debug_bounds("branch-delete-feature")
        .expect("the deletable row carries the trash");
    let inset = row.right() - trash.right();
    assert!(
        (6.0..=10.0).contains(&inset.as_f32()),
        "the trash hugs the row's right edge (inset {inset:?}, row {:?})",
        row.size
    );
}

/// The rail action's pending-work pill pins to the row's right edge instead
/// of hugging the label.
#[gpui::test]
fn the_push_button_carries_its_count(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    scratch_home();
    cx.update(warp_station::theme::apply_kit_theme);

    let store = fake_store(cx);
    let store_handle = store.clone();
    let (_window_handle, cx) =
        cx.add_window_view(|window, cx| MainWindow::new(store_handle.clone(), window, cx));
    cx.run_until_parked();
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.select_repo("/probe/alpha", cx);
        s.open_detail_tab(warp_station::store::WorkspaceTab::Changes, cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    let row = cx
        .debug_bounds("rail-Push")
        .expect("the Push action renders");
    let badge = cx
        .debug_bounds("rail-badge-Push")
        .expect("the pending-push count renders");
    assert!(
        badge.origin.x > row.origin.x && badge.right() <= row.right(),
        "the count sits inside its button ({badge:?} in {row:?})"
    );
}

/// The FilteredPicker branch dropdown: opening overlays the rail without
/// reflowing it, the auto-focused filter narrows the rows (dropping the
/// Remote group when no remote matches), and picking a match checks it out.
#[gpui::test]
fn branch_menu_filters_via_typing_and_pick_checks_out(cx: &mut TestAppContext) {
    use warp_station_core::model::{GitBranchKind, GitBranchRef};

    cx.update(gpui_kit::init);
    scratch_home();
    cx.update(warp_station::theme::apply_kit_theme);

    let store = fake_store(cx);
    let store_handle = store.clone();
    let (_window_handle, cx) =
        cx.add_window_view(|window, cx| MainWindow::new(store_handle.clone(), window, cx));
    cx.run_until_parked();
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.select_repo("/probe/alpha", cx);
        s.open_detail_tab(warp_station::store::WorkspaceTab::Changes, cx);
        s.branches = vec![
            GitBranchRef {
                name: "main".into(),
                kind: GitBranchKind::Local,
            },
            GitBranchRef {
                name: "feature".into(),
                kind: GitBranchKind::Local,
            },
            GitBranchRef {
                name: "origin/main".into(),
                kind: GitBranchKind::Remote,
            },
        ];
    });
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    // the panel's entrance runs on the wall clock
    std::thread::sleep(warpui_kit_core::motion::BASE * 2);
    cx.update(|window, cx| drop(window.draw(cx)));

    let rail_fetch = cx
        .debug_bounds("rail-Fetch")
        .expect("the git actions render");
    let button = cx
        .debug_bounds("branch-button")
        .expect("the branch selector renders in the rail");
    cx.simulate_click(button.center(), gpui::Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    assert!(
        cx.debug_bounds("branch-menu").is_some(),
        "the dropdown opens"
    );
    let rail_fetch_open = cx.debug_bounds("rail-Fetch").expect("the git actions stay");
    assert_eq!(
        rail_fetch.origin, rail_fetch_open.origin,
        "opening the popup overlays the rail, never reflowing it"
    );

    // the filter is focused on open: typing narrows straight away, and the
    // Remote group label vanishes when no remote matches
    cx.simulate_input("feat");
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    assert!(
        cx.debug_bounds("branch-row-feature").is_some(),
        "the filter keeps a match"
    );
    assert!(
        cx.debug_bounds("branch-row-main").is_none(),
        "the filter drops non-matches"
    );
    assert!(
        cx.debug_bounds("branch-group-remote").is_none(),
        "the Remote label only renders before a matching remote"
    );

    // picking the match checks it out and closes the dropdown
    let row = cx
        .debug_bounds("branch-row-feature")
        .expect("the matched row renders");
    cx.simulate_click(row.center(), gpui::Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    assert!(
        cx.debug_bounds("branch-menu").is_none(),
        "picking a branch closes the dropdown"
    );
    assert!(
        cx.read_entity(&store, |s, _| s.git_busy()),
        "the checkout went out"
    );
}

/// Long branch names are only ever trimmed for display: the picker lists the
/// row under its full name and picking it checks out the full branch.
#[gpui::test]
fn a_long_branch_picks_under_its_full_name(cx: &mut TestAppContext) {
    use warp_station_core::model::{GitBranchKind, GitBranchRef};

    const LONG: &str = "users/enginkirmaci/test";
    const ROW: &str = "branch-row-users/enginkirmaci/test";

    cx.update(gpui_kit::init);
    scratch_home();
    cx.update(warp_station::theme::apply_kit_theme);

    // the receiver stays live so the checkout can be read off the channel;
    // startup status probes may queue their own commands first, hence the
    // scan for the Checkout below
    let (net_tx, net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| {
        let mut store = Store::new(cx, net_tx);
        store.group_by_root = false;
        let mut alpha = entry("alpha", "/probe/alpha", &[]);
        alpha.git_branch_name = Some(LONG.into());
        store.replace_repos(vec![alpha, entry("beta", "/probe/beta", &[])]);
        store
    });
    let store_handle = store.clone();
    let (_window_handle, cx) =
        cx.add_window_view(|window, cx| MainWindow::new(store_handle.clone(), window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    cx.update_entity(&store, |s: &mut Store, cx| {
        s.select_repo("/probe/alpha", cx);
        s.open_detail_tab(warp_station::store::WorkspaceTab::Changes, cx);
        s.branches = vec![
            GitBranchRef {
                name: LONG.into(),
                kind: GitBranchKind::Local,
            },
            GitBranchRef {
                name: "main".into(),
                kind: GitBranchKind::Local,
            },
            GitBranchRef {
                name: "origin/users/enginkirmaci/test".into(),
                kind: GitBranchKind::Remote,
            },
        ];
    });
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    let button = cx
        .debug_bounds("branch-button")
        .expect("the branch selector renders in the rail");
    cx.simulate_click(button.center(), gpui::Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    assert!(
        cx.debug_bounds("branch-menu").is_some(),
        "the dropdown opens"
    );
    let row = cx
        .debug_bounds(ROW)
        .expect("the row renders under its full name");
    cx.simulate_click(row.center(), gpui::Modifiers::default());
    cx.run_until_parked();

    let mut checkout = None;
    while let Ok(cmd) = net_rx.try_recv() {
        if let warp_station::net::NetCommand::Git { action, .. } = cmd
            && let warp_station::net::GitAction::Checkout(name) = action
        {
            checkout = Some(name);
        }
    }
    assert_eq!(
        checkout.as_deref(),
        Some(LONG),
        "picking the trimmed row checks out the full branch"
    );
}

/// The picker's label budget is the rail's real width, not the pill's column
/// cap: a name that fits renders whole, an overflowing one sheds its head.
#[gpui::test]
fn picker_label_shows_full_name_while_it_fits(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    scratch_home();
    let store = fake_store(cx);
    let store_handle = store.clone();
    let (_window_handle, cx) =
        cx.add_window_view(|window, cx| MainWindow::new(store_handle.clone(), window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| {
        drop(window.draw(cx));
        // past BRANCH_LABEL_CHARS, but narrow enough to fit the rail
        let fits = "fix/ui/list/title/tilt";
        assert_eq!(
            warp_station::ui::branch_label(fits, window),
            fits,
            "a name that fits the rail renders whole"
        );
        let long = "users/enginkirmaci/a-branch-name-far-too-long-for-the-rail";
        let label = warp_station::ui::branch_label(long, window);
        assert!(
            label.starts_with('…'),
            "an overflowing name sheds its head (got {label:?})"
        );
    });
}

/// The warp status bridge: pushes land per folder, and availability-loss
/// clears them — the row hints follow warp-agent down with it.
#[gpui::test]
fn warp_status_tracks_running_workspaces(cx: &mut TestAppContext) {
    use warp_station::net::{UiEvent, WarpWorkspaceStatus};

    cx.update(gpui_kit::init);
    scratch_home();

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| Store::new(cx, net_tx));

    cx.update_entity(&store, |s: &mut Store, cx| {
        s.on_net_event(
            UiEvent::WarpStatus {
                workspaces: vec![(
                    "/repos/a".into(),
                    WarpWorkspaceStatus {
                        connected: true,
                        running: vec!["chat one".into(), "chat two".into()],
                    },
                )],
            },
            cx,
        );
        s.on_net_event(UiEvent::WarpAvailability { connected: true }, cx);
    });

    assert!(cx.read_entity(&store, |s, _| s.warp_connected));
    assert_eq!(
        cx.read_entity(&store, |s, _| s
            .warp_running
            .get("/repos/a")
            .map(|w| w.running.clone()))
            .unwrap(),
        vec!["chat one".to_string(), "chat two".to_string()]
    );

    cx.update_entity(&store, |s: &mut Store, cx| {
        s.on_net_event(UiEvent::WarpAvailability { connected: false }, cx);
    });
    assert!(!cx.read_entity(&store, |s, _| s.warp_connected));
    assert!(cx.read_entity(&store, |s, _| s.warp_running.is_empty()));
}

/// Switching repos resets the whole per-repo workspace: snapshot, branches,
/// history, drafts, wand state and the busy flags — the rail never shows the
/// previous repo's detail.
#[gpui::test]
fn switching_repos_resets_the_workspace(cx: &mut TestAppContext) {
    use warp_station_core::model::{GitBranchKind, GitBranchRef};

    cx.update(gpui_kit::init);
    scratch_home();

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| Store::new(cx, net_tx));

    cx.update_entity(&store, |s: &mut Store, cx| {
        s.select_repo("/probe/alpha", cx);
        // prime everything the switch must clear (repo A's state)
        s.set_changes(None);
        s.branches = vec![GitBranchRef {
            name: "main".into(),
            kind: GitBranchKind::Local,
        }];
        s.recent_commits = vec![warp_station_core::model::GitCommitInfo {
            hash: "abc".into(),
            subject: "old".into(),
            author: None,
            date: chrono::Local::now(),
        }];
        s.unpushed = Some(["abc".to_string()].into_iter().collect());
        s.commit_message_draft = Some("A's draft".into());
        s.readme_draft = Some("A's readme".into());
        s.wand_busy = Some(warp_station::net::WandKind::Commit);
        s.wand_repo = Some("/probe/alpha".into());
        s.wand_error = Some("stale".into());
        s.git_jobs.insert("/probe/alpha".into(), "pull");
    });

    cx.update_entity(&store, |s: &mut Store, cx| {
        s.select_repo("/probe/beta", cx);
    });
    cx.run_until_parked();

    cx.read_entity(&store, |s, _| {
        assert_eq!(s.selected_repo.as_deref(), Some("/probe/beta"));
        assert!(s.branches.is_empty(), "branches reset");
        assert!(s.recent_commits.is_empty(), "history reset");
        assert!(s.unpushed.is_none(), "unpushed reset");
        assert!(s.commit_message_draft.is_none(), "commit draft reset");
        assert!(s.readme_draft.is_none(), "readme draft reset");
        assert!(s.wand_error.is_none(), "wand error reset");
        assert!(!s.git_busy(), "the new repo is free");
        // the in-flight run keeps its busy flag but loses its repo: it can
        // no longer land a draft on beta
        assert_eq!(s.wand_repo, None);
    });
}

/// A wand run kicked for repo A never lands its draft once repo B is
/// selected — the run ends, but beta's commit box stays empty.
#[gpui::test]
fn wand_done_for_a_switched_away_repo_lands_nothing(cx: &mut TestAppContext) {
    use warp_station::net::{UiEvent, WandKind};

    cx.update(gpui_kit::init);
    scratch_home();

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| Store::new(cx, net_tx));

    cx.update_entity(&store, |s: &mut Store, cx| {
        s.select_repo("/probe/alpha", cx);
    });
    // the run was kicked for alpha, the user has since moved to beta
    cx.update_entity(&store, |s, _| {
        s.wand_busy = Some(WandKind::Commit);
        s.wand_repo = Some("/probe/alpha".into());
    });
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.select_repo("/probe/beta", cx);
        s.wand_busy = Some(WandKind::Commit);
        s.wand_repo = Some("/probe/alpha".into());
    });
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.on_net_event(
            UiEvent::WandDone {
                wand_id: "w".into(),
                kind: WandKind::Commit,
                result: Ok("A's generated message".into()),
            },
            cx,
        );
    });

    cx.read_entity(&store, |s, _| {
        assert!(s.wand_busy.is_none(), "the run ended");
        assert!(
            s.commit_message_draft.is_none(),
            "alpha's draft never lands in beta's commit box"
        );
    });

    // for comparison: a run for the still-selected repo lands its draft
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.wand_busy = Some(WandKind::Commit);
        s.wand_repo = Some("/probe/beta".into());
        s.on_net_event(
            UiEvent::WandDone {
                wand_id: "w".into(),
                kind: WandKind::Commit,
                result: Ok("B's generated message".into()),
            },
            cx,
        );
    });
    cx.read_entity(&store, |s, _| {
        assert_eq!(
            s.commit_message_draft.as_deref(),
            Some("B's generated message")
        );
    });
}

/// The Refresh button: a scan first (local), then one fetch pass carrying only
/// the repos with a remote, so the local refresh never waits for the network.
/// The button is the only fetch trigger; no timer re-arms behind it.
#[gpui::test]
fn refresh_scans_first_and_fetches_second(cx: &mut TestAppContext) {
    use warp_station::net::NetCommand;

    cx.update(gpui_kit::init);
    scratch_home();

    let scratch = std::env::temp_dir().join(format!(
        "warp-station-refresh-sync-{}.json",
        std::process::id()
    ));
    std::fs::write(&scratch, "{}").expect("scratch settings");

    let (net_tx, net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| {
        let mut store = Store::with_settings(
            net_tx,
            std::sync::Arc::new(warp_station_core::settings::SettingsService::at(
                scratch.clone(),
            )),
        );
        store.replace_repos(vec![
            entry("alpha", "/probe/alpha", &[]),
            entry("beta", "/probe/beta", &[]),
        ]);
        store.repos[1].git_has_remote = false;
        store.scanning = false;
        store
    });
    while net_rx.try_recv().is_ok() {}

    cx.update_entity(&store, |s: &mut Store, cx| s.refresh(cx));

    let mut fetched: Option<Vec<PathBuf>> = None;
    let mut scan_index: Option<usize> = None;
    let mut send_index = 0;
    while let Ok(command) = net_rx.try_recv() {
        match command {
            NetCommand::FetchAll { folders } => fetched = Some(folders),
            NetCommand::Scan { .. } => scan_index = Some(send_index),
            _ => {}
        }
        send_index += 1;
    }
    let scan_index = scan_index.expect("refresh rescans");
    assert_eq!(
        fetched.expect("refresh fetches the remote repos"),
        vec![PathBuf::from("/probe/alpha")],
        "only the repos with a remote are fetched"
    );
    assert_eq!(
        scan_index, 0,
        "the scan goes out first: the local refresh never waits for the network"
    );
    cx.read_entity(&store, |s, _| {
        assert!(s.scanning, "the toolbar shows Refreshing");
        assert!(
            s.fetch_progress.is_none(),
            "the pass reports its own progress as it streams"
        );
    });

    let _ = std::fs::remove_file(&scratch);
}
/// A scan that settles after a status probe and a GitHub snapshot landed must
/// keep their counts: the merge runs against the live list, not against the
/// snapshot the scan was queued with (the launch queues its activity pass
/// behind the scan, and a fetch can hold the scan for longer).
#[gpui::test]
fn a_settling_scan_keeps_counts_that_landed_while_it_waited(cx: &mut TestAppContext) {
    use warp_station::net::UiEvent;
    use warp_station_core::github::{GitHubItem, GitHubSnapshot};
    use warp_station_core::repo_store::StatusProbe;

    cx.update(gpui_kit::init);
    scratch_home();

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| {
        let mut store = Store::new(cx, net_tx);
        // the cache load: known identity, no runtime state
        store.replace_repos(vec![entry("alpha", "/probe/alpha", &[])]);
        store.repos[0].git_status_loaded = false;
        store.repos[0].git_modified_count = 0;
        store.repos[0].github_pr_count = 0;
        store.repos[0].github_issue_count = 0;
        store
    });

    cx.update_entity(&store, |s: &mut Store, cx| {
        s.on_net_event(
            UiEvent::StatusSettled {
                probes: vec![StatusProbe {
                    folder_path: "/probe/alpha".into(),
                    branch_name: Some("main".into()),
                    modified_count: 10,
                    ..Default::default()
                }],
                after_fetch: false,
                progress: None,
            },
            cx,
        );
        s.on_net_event(
            UiEvent::GitHubSettled {
                items: vec![(
                    "/probe/alpha".into(),
                    GitHubSnapshot {
                        url: Some("https://github.com/example/alpha".into()),
                        pulls: vec![GitHubItem {
                            number: 1,
                            ..Default::default()
                        }],
                        issues: vec![
                            GitHubItem {
                                number: 2,
                                ..Default::default()
                            },
                            GitHubItem {
                                number: 3,
                                ..Default::default()
                            },
                        ],
                        available: true,
                    },
                )],
            },
            cx,
        );
    });

    // the scan settles last, naming the same repo it named when it was queued
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.on_net_event(
            UiEvent::ScanSettled {
                scanned: vec![scanned("alpha", "/probe/alpha")],
            },
            cx,
        );
    });

    cx.read_entity(&store, |s, _| {
        let alpha = s
            .repos
            .iter()
            .find(|repo| repo.name == "alpha")
            .expect("alpha survives the scan");
        assert!(
            alpha.git_status_loaded,
            "the probe's loaded flag survives the scan"
        );
        assert_eq!(
            alpha.git_branch_name.as_deref(),
            Some("main"),
            "the probe's branch survives the scan"
        );
        assert_eq!(
            alpha.git_modified_count, 10,
            "the probe's change count survives the scan"
        );
        assert_eq!(
            alpha.github_pr_count, 1,
            "the GitHub snapshot's pull count survives the scan"
        );
        assert_eq!(
            alpha.github_issue_count, 2,
            "the GitHub snapshot's issue count survives the scan"
        );
    });
}

/// Local-first at launch: the scan goes out before the network-bound
/// GitHub/Azure pass, no separate probe rides in front of it (the scan's
/// settle probes the merged list), and a repo the scan names joins the list.
#[gpui::test]
fn launch_scans_first_and_probes_on_settle(cx: &mut TestAppContext) {
    use warp_station::net::{NetCommand, UiEvent};

    cx.update(gpui_kit::init);
    scratch_home();

    let (net_tx, net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| Store::new(cx, net_tx));

    let mut commands = Vec::new();
    while let Ok(command) = net_rx.try_recv() {
        commands.push(command);
    }
    let scan = commands
        .iter()
        .position(|command| matches!(command, NetCommand::Scan { .. }))
        .expect("the launch scans");
    assert!(
        !commands
            .iter()
            .any(|command| matches!(command, NetCommand::ProbeAll { .. })),
        "the settle probes the scanned list; a launch probe would double the work"
    );
    if let Some(activity) = commands.iter().position(|command| {
        matches!(
            command,
            NetCommand::GitHubPass { .. } | NetCommand::AzurePass { .. }
        )
    }) {
        assert!(
            scan < activity,
            "the local scan never waits for the network"
        );
    }

    cx.update_entity(&store, |s: &mut Store, cx| {
        s.on_net_event(
            UiEvent::ScanSettled {
                scanned: vec![scanned("alpha", "/probe/alpha")],
            },
            cx,
        );
    });

    let mut probed = Vec::new();
    while let Ok(command) = net_rx.try_recv() {
        if let NetCommand::ProbeAll { folders } = command {
            probed = folders;
        }
    }
    assert_eq!(
        probed,
        vec![PathBuf::from("/probe/alpha")],
        "the settle probes the repo the scan names"
    );
}

/// Drains the store's net channel, reporting whether a GitHub pass was
/// queued (the eager kick and the activity clock both emit one).
fn drained_a_github_pass(net_rx: &async_channel::Receiver<warp_station::net::NetCommand>) -> bool {
    let mut found = false;
    while let Ok(command) = net_rx.try_recv() {
        if matches!(command, warp_station::net::NetCommand::GitHubPass { .. }) {
            found = true;
        }
    }
    found
}

/// A failed GitHub probe is not a fresh answer: the row keeps its last known
/// counts, the eager kick retries instead of parking for the TTL, and the
/// Refresh button's explicit ask outranks the gate even when it is fresh.
#[gpui::test]
fn activity_failures_keep_counts_and_the_eager_kick_retries(cx: &mut TestAppContext) {
    use warp_station::net::UiEvent;
    use warp_station_core::github::{GitHubItem, GitHubSnapshot};

    cx.update(gpui_kit::init);
    scratch_home();

    let gh = fake_gh_executable("keep-counts");
    let scratch = std::env::temp_dir().join(format!(
        "warp-station-github-failure-{}.json",
        std::process::id()
    ));
    std::fs::write(
        &scratch,
        format!(
            r#"{{"Repos": {{ "GitHubExecutable": "{}" }}, "Notifications": {{ "Enabled": false }}}}"#,
            gh.display()
        ),
    )
    .expect("scratch settings");

    let (net_tx, net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| {
        let mut store = Store::with_settings(
            net_tx,
            std::sync::Arc::new(warp_station_core::settings::SettingsService::at(
                scratch.clone(),
            )),
        );
        // a favorite, so the eager set names it
        store.replace_repos(vec![entry("alpha", "/probe/alpha", &["favorites"])]);
        store.scanning = false;
        store
    });
    while net_rx.try_recv().is_ok() {}

    let snapshot = |available: bool, pulls: u32, issues: u32| GitHubSnapshot {
        url: Some("https://github.com/example/alpha".into()),
        pulls: (0..pulls)
            .map(|ix| GitHubItem {
                number: u64::from(ix) + 1,
                ..Default::default()
            })
            .collect(),
        issues: (0..issues)
            .map(|ix| GitHubItem {
                number: u64::from(ix) + 1,
                ..Default::default()
            })
            .collect(),
        available,
    };
    let settle = |folder: &str, snapshot: GitHubSnapshot| UiEvent::GitHubSettled {
        items: vec![(folder.to_string(), snapshot)],
    };

    // nothing has landed yet: the eager set is due
    cx.update_entity(&store, |s: &mut Store, _| s.kick_eager_activity(false));
    assert!(drained_a_github_pass(&net_rx), "the first eager kick asks");

    // a failed pass must not stamp the freshness gate
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.on_net_event(settle("/probe/alpha", snapshot(false, 0, 0)), cx);
    });
    cx.update_entity(&store, |s: &mut Store, _| s.kick_eager_activity(false));
    assert!(
        drained_a_github_pass(&net_rx),
        "a failed probe leaves the eager set due"
    );

    // a good pass fills the counts and counts as the fresh answer
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.on_net_event(settle("/probe/alpha", snapshot(true, 2, 1)), cx);
    });
    cx.read_entity(&store, |s, _| {
        assert_eq!(s.repos[0].github_pr_count, 2);
        assert_eq!(s.repos[0].github_issue_count, 1);
    });
    cx.update_entity(&store, |s: &mut Store, _| s.kick_eager_activity(false));
    assert!(
        !drained_a_github_pass(&net_rx),
        "a fresh snapshot is not re-asked by the next kick"
    );

    // a later failure keeps the counts it cannot replace
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.on_net_event(settle("/probe/alpha", snapshot(false, 0, 0)), cx);
    });
    cx.read_entity(&store, |s, _| {
        assert_eq!(
            s.repos[0].github_pr_count, 2,
            "a failed probe does not blank the row"
        );
        assert_eq!(s.repos[0].github_issue_count, 1);
    });

    // the Refresh button forces the eager set even inside the TTL
    cx.update_entity(&store, |s: &mut Store, cx| s.refresh(cx));
    assert!(
        drained_a_github_pass(&net_rx),
        "a manual refresh re-asks the eager set"
    );
}

/// A row with no status yet says so; "clean" is a claim only a probe earns.
#[gpui::test]
fn an_unprobed_row_says_checking_instead_of_clean(cx: &mut TestAppContext) {
    use warp_station::net::UiEvent;
    use warp_station_core::repo_store::StatusProbe;

    cx.update(gpui_kit::init);
    scratch_home();
    cx.update(warp_station::theme::apply_kit_theme);

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| {
        let mut store = Store::new(cx, net_tx);
        let mut alpha = entry("alpha", "/probe/alpha", &[]);
        alpha.git_status_loaded = false;
        alpha.git_branch_name = None;
        alpha.git_modified_count = 0;
        store.replace_repos(vec![alpha]);
        store
    });

    let store_handle = store.clone();
    let (_window, cx) = cx.add_window_view(|window, cx| MainWindow::new(store_handle, window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    assert!(
        cx.debug_bounds("repo-checking-0").is_some(),
        "an unprobed row shows the checking state"
    );
    assert!(
        cx.debug_bounds("repo-clean-0").is_none(),
        "and never claims the repo is clean"
    );

    cx.update_entity(&store, |s: &mut Store, cx| {
        s.on_net_event(
            UiEvent::StatusSettled {
                probes: vec![StatusProbe {
                    folder_path: "/probe/alpha".into(),
                    branch_name: Some("main".into()),
                    modified_count: 0,
                    ..Default::default()
                }],
                after_fetch: false,
                progress: None,
            },
            cx,
        );
    });
    cx.update(|window, cx| drop(window.draw(cx)));

    assert!(
        cx.debug_bounds("repo-clean-0").is_some(),
        "the probe's settle earns the clean state"
    );
    assert!(cx.debug_bounds("repo-checking-0").is_none());
}

/// Opens a fake `gh` executable so the pass kickers resolve deterministically
/// (the real `gh` on PATH would leak in through the same code path).
fn fake_gh_executable(tag: &str) -> PathBuf {
    let path =
        std::env::temp_dir().join(format!("warp-station-fake-gh-{tag}-{}", std::process::id()));
    std::fs::write(&path, b"").expect("scratch gh");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    path
}

/// The GitHub/Azure activity schedule: when due it sweeps the live list —
/// every remote-relevant non-archived repo, archived rows stay out — then
/// re-arms the cadence; with either integration switched off, nothing goes
/// out at all.
#[gpui::test]
fn activity_passes_sweep_the_live_list_and_skip_archived(cx: &mut TestAppContext) {
    use warp_station::net::NetCommand;

    cx.update(gpui_kit::init);
    scratch_home();

    let gh = fake_gh_executable("activity");
    let scratch = std::env::temp_dir().join(format!(
        "warp-station-activity-probe-{}.json",
        std::process::id()
    ));
    std::fs::write(
        &scratch,
        format!(
            r#"{{"Repos": {{ "GitHubExecutable": "{}", "AzureDevOpsPat": "pat" }}, "Notifications": {{ "Enabled": false }}}}"#,
            gh.display()
        ),
    )
    .expect("scratch settings");

    let (net_tx, net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| {
        let mut store = Store::with_settings(
            net_tx,
            std::sync::Arc::new(warp_station_core::settings::SettingsService::at(
                scratch.clone(),
            )),
        );
        store.replace_repos(vec![
            entry("alpha", "/probe/alpha", &[]),
            entry("beta", "/probe/beta", &["archived"]),
        ]);
        // a pass due right now on the next tick
        store.next_activity_refresh =
            Some(std::time::Instant::now() - std::time::Duration::from_secs(1));
        store
    });
    // the constructor's own startup scan and full activity pass are in flight
    while net_rx.try_recv().is_ok() {}

    cx.update_entity(&store, |s: &mut Store, cx| s.check_activity_refresh(cx));

    let mut github_folders: Option<Vec<PathBuf>> = None;
    let mut azure_folders: Option<Vec<PathBuf>> = None;
    while let Ok(command) = net_rx.try_recv() {
        match command {
            NetCommand::GitHubPass { folders, .. } => github_folders = Some(folders),
            NetCommand::AzurePass { folders, .. } => azure_folders = Some(folders),
            _ => {}
        }
    }
    let live = vec![PathBuf::from("/probe/alpha")];
    assert_eq!(
        github_folders.expect("the GitHub sweep fires when enabled"),
        live,
        "the activity sweep covers the live repos only"
    );
    assert_eq!(
        azure_folders.expect("the Azure sweep fires when enabled"),
        live,
        "the activity sweep covers the live repos only"
    );
    cx.read_entity(&store, |s, _| {
        assert!(
            s.next_activity_refresh.expect("each fire re-arms") > std::time::Instant::now(),
            "the activity clock re-arms a cadence out"
        );
    });

    // an integration switched off sends nothing, even due
    cx.update_entity(&store, |s: &mut Store, _| {
        s.update_settings(|settings| {
            settings.repos.enable_git_hub = false;
            settings.repos.enable_azure_dev_ops = false;
        });
        s.next_activity_refresh =
            Some(std::time::Instant::now() - std::time::Duration::from_secs(1));
    });
    cx.update_entity(&store, |s: &mut Store, cx| s.check_activity_refresh(cx));
    while let Ok(command) = net_rx.try_recv() {
        assert!(
            !matches!(
                command,
                NetCommand::GitHubPass { .. } | NetCommand::AzurePass { .. }
            ),
            "disabled integrations never queue an activity pass"
        );
    }
    cx.read_entity(&store, |s, _| {
        assert!(
            s.next_activity_refresh.expect("the clock re-arms anyway") > std::time::Instant::now(),
        );
    });

    let _ = std::fs::remove_file(&scratch);
    let _ = std::fs::remove_file(&gh);
}

/// A refresh's eager GitHub/Azure set: favorites and the selected repo, the
/// archived tagged rows excluded; disabled integrations keep the eager set
/// silent too.
#[gpui::test]
fn refresh_kicks_the_eager_github_and_azure_set_only(cx: &mut TestAppContext) {
    use warp_station::net::NetCommand;

    cx.update(gpui_kit::init);
    scratch_home();

    let gh = fake_gh_executable("eager");
    let scratch = std::env::temp_dir().join(format!(
        "warp-station-eager-probe-{}.json",
        std::process::id()
    ));
    std::fs::write(
        &scratch,
        format!(
            r#"{{"Repos": {{ "GitHubExecutable": "{}", "AzureDevOpsPat": "pat" }}, "Notifications": {{ "Enabled": false }}}}"#,
            gh.display()
        ),
    )
    .expect("scratch settings");

    let (net_tx, net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| {
        let mut store = Store::with_settings(
            net_tx,
            std::sync::Arc::new(warp_station_core::settings::SettingsService::at(
                scratch.clone(),
            )),
        );
        store.replace_repos(vec![
            entry("alpha", "/probe/alpha", &["favorites"]),
            entry("beta", "/probe/beta", &[]),
            entry("gamma", "/probe/gamma", &["favorites", "archived"]),
        ]);
        store
    });
    while net_rx.try_recv().is_ok() {}

    cx.update_entity(&store, |s: &mut Store, _| {
        s.scanning = false;
        s.selected_repo = Some("/probe/beta".to_string());
    });
    cx.update_entity(&store, |s: &mut Store, cx| s.refresh(cx));

    let mut github_folders: Option<Vec<PathBuf>> = None;
    let mut azure_folders: Option<Vec<PathBuf>> = None;
    while let Ok(command) = net_rx.try_recv() {
        match command {
            NetCommand::GitHubPass { folders, .. } => github_folders = Some(folders),
            NetCommand::AzurePass { folders, .. } => azure_folders = Some(folders),
            _ => {}
        }
    }
    let eager = vec![PathBuf::from("/probe/alpha"), PathBuf::from("/probe/beta")];
    assert_eq!(
        github_folders.expect("the favorite and the selected repo re-ask"),
        eager,
        "the eager set covers favorites and the selection, not the archived"
    );
    assert_eq!(
        azure_folders.expect("the same set feeds Azure"),
        eager,
        "the eager set covers favorites and the selection, not the archived"
    );

    // a disabled integration silences its side of the eager set
    cx.update_entity(&store, |s: &mut Store, _| {
        s.scanning = false;
        s.update_settings(|settings| {
            settings.repos.enable_git_hub = false;
            settings.repos.enable_azure_dev_ops = false;
        });
    });
    cx.update_entity(&store, |s: &mut Store, cx| s.refresh(cx));
    while let Ok(command) = net_rx.try_recv() {
        assert!(
            !matches!(
                command,
                NetCommand::GitHubPass { .. } | NetCommand::AzurePass { .. }
            ),
            "disabled integrations never queue the eager set"
        );
    }

    let _ = std::fs::remove_file(&scratch);
    let _ = std::fs::remove_file(&gh);
}

/// The fetch pass streams: each per-repo settle applies its probe the moment
/// it lands and drives the toolbar's Fetching k/n stage, which clears when
/// the pass's last repo lands.
#[gpui::test]
fn fetch_pass_reports_progress_per_repo(cx: &mut TestAppContext) {
    use warp_station::net::UiEvent;

    cx.update(gpui_kit::init);
    scratch_home();

    let scratch = std::env::temp_dir().join(format!(
        "warp-station-progress-probe-{}.json",
        std::process::id()
    ));
    std::fs::write(&scratch, r#"{"Notifications": {"Enabled": false}}"#).expect("scratch settings");

    let (net_tx, net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| {
        let mut store = Store::with_settings(
            net_tx,
            std::sync::Arc::new(warp_station_core::settings::SettingsService::at(
                scratch.clone(),
            )),
        );
        store.replace_repos(vec![
            entry("alpha", "/probe/alpha", &[]),
            entry("beta", "/probe/beta", &[]),
        ]);
        store
    });
    while net_rx.try_recv().is_ok() {}
    // the constructor's startup scan is still "in flight" (the store shows
    // Refreshing): a refresh while it runs coalesces away, so clear it first
    cx.update_entity(&store, |s: &mut Store, _| s.scanning = false);
    cx.update_entity(&store, |s: &mut Store, cx| s.refresh(cx));
    // drain the scan + fetch queue before the settled events arrive
    while net_rx.try_recv().is_ok() {}

    let probe = |folder: &str| warp_station_core::repo_store::StatusProbe {
        folder_path: folder.to_string(),
        ..Default::default()
    };
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.on_net_event(
            UiEvent::StatusSettled {
                probes: vec![probe("/probe/alpha")],
                after_fetch: true,
                progress: Some((1, 2)),
            },
            cx,
        );
    });
    cx.read_entity(&store, |s, _| {
        assert_eq!(
            s.fetch_progress,
            Some((1, 2)),
            "the button stage shows the settled repo's k/n"
        );
    });

    cx.update_entity(&store, |s: &mut Store, cx| {
        s.on_net_event(
            UiEvent::StatusSettled {
                probes: vec![probe("/probe/beta")],
                after_fetch: true,
                progress: Some((2, 2)),
            },
            cx,
        );
    });
    cx.read_entity(&store, |s, _| {
        assert_eq!(
            s.fetch_progress, None,
            "the stage clears once the pass's last repo lands"
        );
    });

    let _ = std::fs::remove_file(&scratch);
}

/// Drains the store's net channel for LoadChanges requests.
fn load_changes_folders(
    net_rx: &async_channel::Receiver<warp_station::net::NetCommand>,
) -> Vec<String> {
    let mut folders = Vec::new();
    while let Ok(command) = net_rx.try_recv() {
        if let warp_station::net::NetCommand::LoadChanges { folder } = command {
            folders.push(folder.to_string_lossy().to_string());
        }
    }
    folders
}

/// A fetch pass's settled probes re-load the selected repo's workspace (the
/// bar's file list, branches and unpushed) without a manual reselect; plain
/// probe passes and passes not covering the selection leave it alone.
#[gpui::test]
fn fetch_settled_reloads_the_selected_workspace(cx: &mut TestAppContext) {
    use warp_station_core::repo_store::StatusProbe;

    // like every probe here: the scratch HOME must be in place before the
    // store reads settings, whatever order the parallel tests start in
    scratch_home();
    let (net_tx, net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| {
        let mut store = Store::new(cx, net_tx);
        store.replace_repos(vec![entry("alpha", "/probe/alpha", &[])]);
        store
    });
    cx.update_entity(&store, |s: &mut Store, _| {
        s.selected_repo = Some("/probe/alpha".into());
    });
    // drain the construction kicks
    assert!(load_changes_folders(&net_rx).is_empty());

    let probe = |folder: &str| StatusProbe {
        folder_path: folder.to_string(),
        ..Default::default()
    };

    // the fetch pass covering the selection reloads the open workspace
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.on_net_event(
            warp_station::net::UiEvent::StatusSettled {
                probes: vec![probe("/probe/alpha")],
                after_fetch: true,
                progress: None,
            },
            cx,
        );
    });
    assert_eq!(
        load_changes_folders(&net_rx),
        vec!["/probe/alpha".to_string()],
        "the fetch pass re-loads the selected repo's workspace"
    );

    // a plain probe pass (launch, scan settle) does not
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.on_net_event(
            warp_station::net::UiEvent::StatusSettled {
                probes: vec![probe("/probe/alpha")],
                after_fetch: false,
                progress: None,
            },
            cx,
        );
    });
    assert!(
        load_changes_folders(&net_rx).is_empty(),
        "a non-fetch pass must not re-load the workspace"
    );

    // a fetch pass that skipped the selection (busy exclusion) does not either
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.on_net_event(
            warp_station::net::UiEvent::StatusSettled {
                probes: vec![probe("/probe/other")],
                after_fetch: true,
                progress: None,
            },
            cx,
        );
    });
    assert!(
        load_changes_folders(&net_rx).is_empty(),
        "a fetch pass not covering the selection must not re-load the workspace"
    );
}

/// Leaks a per-row selector for debug_bounds (which wants 'static).
fn row_selector(prefix: &str, row: usize) -> &'static str {
    Box::leak(format!("{prefix}{row}").into_boxed_str())
}

/// The archiving flow: an archived repo leaves the projection (and every
/// background pass) by default, the Archive chip counts it and toggles the
/// limp rows back on below the live ones, and the rail's menu restores the
/// selected one: the tag drops, the folder re-probes, a toast confirms.
#[gpui::test]
fn archived_repos_hide_toggle_and_restore(cx: &mut TestAppContext) {
    use warp_station::net::NetCommand;

    cx.update(gpui_kit::init);
    scratch_home();
    cx.update(warp_station::theme::apply_kit_theme);

    let (net_tx, net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| {
        let mut store = Store::new(cx, net_tx);
        store.group_by_root = false;
        store.replace_repos(vec![
            entry("alpha", "/probe/alpha", &["favorites"]),
            entry("beta", "/probe/beta", &["archived"]),
            entry("gamma", "/probe/gamma", &[]),
        ]);
        store
    });
    let store_handle = store.clone();
    let (_window_handle, cx) =
        cx.add_window_view(|window, cx| MainWindow::new(store_handle.clone(), window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    assert!(
        cx.read_entity(&store, |s, _| s
            .repos
            .iter()
            .find(|r| r.name == "beta")
            .is_some_and(|r| r.is_archived())),
        "the seeded repo carries the archived tag"
    );
    assert_eq!(
        cx.read_entity(&store, |s, _| s.projected.len()),
        2,
        "the archived repo hides from the default projection"
    );
    assert!(
        cx.debug_bounds("archive-chip").is_some(),
        "the Archive chip shows while an archived repo exists"
    );

    // the chip toggles the archived rows back on, below the live ones
    let chip = cx
        .debug_bounds("archive-chip")
        .expect("the archive chip renders");
    cx.simulate_click(chip.center(), gpui::Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    assert!(cx.read_entity(&store, |s, _| s.show_archived));
    assert_eq!(cx.read_entity(&store, |s, _| s.projected.len()), 3);
    let beta_row = cx.read_entity(&store, |s, _| {
        s.projected
            .iter()
            .position(|row| matches!(row, ProjectedRow::Repo(ix) if s.repos[*ix].name == "beta"))
            .expect("the archived row joins the projection when shown")
    });
    assert_eq!(
        beta_row, 2,
        "the archived row sorts below every live repo (favorites first)"
    );
    // restore: the rail's menu acts on the row it opened on
    dialog_click(cx, row_selector("repo-row-", beta_row));
    dialog_click(cx, "insp-more");
    dialog_click(cx, "insp-menu-archive");
    assert!(
        !cx.read_entity(&store, |s, _| s
            .repos
            .iter()
            .find(|r| r.name == "beta")
            .is_some_and(|r| r.is_archived())),
        "restore drops the archived tag"
    );
    let mut restored_probe = false;
    while let Ok(command) = net_rx.try_recv() {
        let NetCommand::ProbeAll { folders } = command else {
            continue;
        };
        if folders == vec![PathBuf::from("/probe/beta")] {
            restored_probe = true;
        }
    }
    assert!(restored_probe, "restore re-probes the restored folder");
    assert!(
        cx.debug_bounds("toast-0").is_some(),
        "restore confirms with a toast"
    );
}

fn scanned(name: &str, path: &str) -> warp_station_core::scan::ScannedRepo {
    warp_station_core::scan::ScannedRepo {
        name: name.into(),
        folder_path: PathBuf::from(path),
        solution_path: None,
        tags: Vec::new(),
    }
}

fn dialog_click(cx: &mut gpui::VisualTestContext, selector: &'static str) {
    let b = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} missing from the frame"));
    cx.simulate_click(b.center(), gpui::Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
}

fn dialog_draw(cx: &mut gpui::VisualTestContext) {
    cx.update(|window, cx| drop(window.draw(cx)));
}

/// The Add dialog: results arrive through the store (the net settle path),
/// Add stays gated until a row is checked — a no-op click must not close the
/// dialog — and Rescan resets to the Scan state through the real field +
/// button interaction.
#[gpui::test]
fn add_dialog_gated_add_and_rescan(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    scratch_home();
    cx.update(warp_station::theme::apply_kit_theme);

    let store = fake_store(cx);
    let store_handle = store.clone();
    let (_window_handle, cx) =
        cx.add_window_view(|window, cx| MainWindow::new(store_handle.clone(), window, cx));
    cx.run_until_parked();
    dialog_draw(cx);

    dialog_click(cx, "add-repo-button");
    assert!(
        cx.debug_bounds("add-repo-overlay").is_some(),
        "dialog opens"
    );
    assert!(
        cx.debug_bounds("btn-Scan").is_some(),
        "the dedicated Scan button renders"
    );
    assert!(
        cx.debug_bounds("add-dialog-close").is_some(),
        "the header close button renders"
    );

    // the net settle path: the worker drops results on the store, the page
    // drains them while drawing
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.deep_scan_result = Some(vec![
            scanned("repo-x", "/probe/scan/repo-x"),
            scanned("repo-y", "/probe/scan/repo-y"),
        ]);
        cx.notify();
    });
    cx.run_until_parked();
    dialog_draw(cx);
    assert!(
        cx.debug_bounds("btn-accent-Add 2").is_some(),
        "the footer names the count it will add"
    );
    assert!(cx.debug_bounds("found-repo-0").is_some());

    // uncheck both rows: the disabled Add must be a no-op, not a close
    dialog_click(cx, "found-repo-0");
    dialog_click(cx, "found-repo-1");
    dialog_click(cx, "btn-accent-Add");
    assert!(
        cx.debug_bounds("add-repo-overlay").is_some(),
        "gated Add must not close the dialog"
    );

    // type a fresh path and Scan again: stale rows are cleared, the dialog
    // holds; the results well is back to its empty text
    dialog_click(cx, "add-root-input");
    cx.simulate_input("/probe/scan-root");
    cx.run_until_parked();
    dialog_draw(cx);
    dialog_click(cx, "btn-Scan");
    assert!(
        cx.debug_bounds("found-repo-0").is_none(),
        "stale results are cleared on rescan"
    );
    assert!(cx.debug_bounds("add-repo-overlay").is_some());

    // the header X closes the dialog
    dialog_click(cx, "add-dialog-close");
    assert!(
        cx.debug_bounds("add-repo-overlay").is_none(),
        "the X close button dismisses the dialog"
    );
}

/// The Add dialog's commit must persist the new folders as scan roots: the
/// listing scan rebuilds the list from the roots and drops everything else,
/// so an add that only lands in the repo cache is erased by the next rescan
/// (startup, Refresh).
#[gpui::test]
fn register_repos_persists_new_folders_as_scan_roots(cx: &mut TestAppContext) {
    let home = std::env::temp_dir().join(format!(
        "warp-station-register-probe-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join(".warp")).unwrap();
    // SAFETY: process-global env; other tests in this binary tolerate a
    // default-settings store, so a racing HOME read is harmless. USERPROFILE
    // too on Windows: this probe saves through the store's default service,
    // which would otherwise persist into the real ~/.warp.
    unsafe {
        std::env::set_var("HOME", &home);
        #[cfg(windows)]
        std::env::set_var("USERPROFILE", &home);
    };

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| Store::new(cx, net_tx));
    cx.update_entity(&store, |s: &mut Store, _| {
        s.replace_repos(vec![entry("alpha", "/probe/alpha", &[])]);
    });

    cx.update_entity(&store, |s: &mut Store, cx| {
        s.register_repos(
            &[
                scanned("repo-x", "/probe/scan/repo-x"),
                scanned("alpha", "/probe/alpha"),
            ],
            cx,
        )
    });

    let (roots, names) = cx.read_entity(&store, |s, _| {
        (
            s.settings().repos.repo_scan_folders.clone(),
            s.repos.iter().map(|r| r.name.clone()).collect::<Vec<_>>(),
        )
    });
    assert_eq!(
        names,
        vec!["alpha", "repo-x"],
        "the add lands in the live list"
    );
    assert_eq!(
        roots,
        Some(vec!["/probe/scan/repo-x".to_string()]),
        "the new folder joins the persisted scan roots; known folders stay untouched"
    );

    let _ = std::fs::remove_dir_all(&home);
}

/// The titlebar's window controls keep their designed 26px boxes and 30px
/// pitch: the right group's width is computed from the enabled chips, and the
/// old fixed guess sat below the real content, so the flex row crushed the
/// minimize/maximize/close cluster to a fraction of its pitch.
#[gpui::test]
fn titlebar_window_controls_keep_their_pitch(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    scratch_home();
    cx.update(warp_station::theme::apply_kit_theme);

    // a missing scratch file = default settings = NuGet + warp chips on, the
    // widest right-group the Linux build can render
    let scratch = std::env::temp_dir().join(format!(
        "warp-station-probe-titlebar-{}.json",
        std::process::id()
    ));
    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| {
        Store::with_settings(
            net_tx,
            std::sync::Arc::new(warp_station_core::settings::SettingsService::at(
                scratch.clone(),
            )),
        )
    });
    let store_handle = store.clone();
    let (_window_handle, cx) =
        cx.add_window_view(|window, cx| MainWindow::new(store_handle.clone(), window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    let right = cx
        .debug_bounds("titlebar-right")
        .expect("right group renders");
    let list = cx.debug_bounds("list-toggle").expect("list toggle bounds");
    let minimize = cx
        .debug_bounds("minimize-window")
        .expect("minimize renders");
    let zoom = cx.debug_bounds("zoom").expect("maximize renders");
    let close = cx.debug_bounds("close-window").expect("close renders");
    // the computed width must actually hold the content: a too-small box
    // spills the chips out of its start edge and collapses the row's gaps
    assert!(
        list.origin.x >= right.origin.x - gpui::px(0.5),
        "the toggle fits inside the right group (toggle at {:?}, group at {:?})",
        list.origin.x,
        right.origin.x
    );
    for (name, bounds) in [("minimize", minimize), ("zoom", zoom), ("close", close)] {
        assert!(
            (bounds.size.width - gpui::px(36.)).abs() < gpui::px(0.5),
            "{name} keeps its 36px box (got {:?})",
            bounds.size.width
        );
    }
    // adjacent caption buttons: glyph centers one 36px box apart — the
    // export's caption box, and the pitch has to survive the right column
    let pitch = |a: gpui::Bounds<gpui::Pixels>, b: gpui::Bounds<gpui::Pixels>| {
        (b.center().x - a.center().x).abs()
    };
    assert!(
        (pitch(minimize, zoom) - gpui::px(36.)).abs() < gpui::px(1.),
        "minimize/maximize sit box-to-box (got {:?})",
        pitch(minimize, zoom)
    );
    assert!(
        (pitch(zoom, close) - gpui::px(36.)).abs() < gpui::px(1.),
        "maximize/close sit box-to-box (got {:?})",
        pitch(zoom, close)
    );

    let _ = std::fs::remove_file(&scratch);
}

/// The Clone dialog: the destination is editable and feeds validation — an
/// empty destination holds the dialog open with the inline error, a valid
/// one lets the commit flip the button to its busy label.
#[gpui::test]
fn clone_dialog_editable_destination_feeds_validation(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    scratch_home();
    cx.update(warp_station::theme::apply_kit_theme);

    let store = fake_store(cx);
    let store_handle = store.clone();
    let (_window_handle, cx) =
        cx.add_window_view(|window, cx| MainWindow::new(store_handle.clone(), window, cx));
    cx.run_until_parked();
    dialog_draw(cx);

    dialog_click(cx, "clone-button");
    assert!(cx.debug_bounds("clone-overlay").is_some(), "dialog opens");
    assert!(cx.debug_bounds("btn-accent-Clone").is_some());

    // no URL yet: the disabled Clone is a no-op
    dialog_click(cx, "btn-accent-Clone");
    assert!(cx.debug_bounds("clone-overlay").is_some());

    // URL typed, destination cleared: validation must hold the dialog open
    dialog_click(cx, "clone-url-input");
    cx.simulate_input("https://github.com/owner/dt-probe-clone.git");
    cx.run_until_parked();
    dialog_draw(cx);
    dialog_click(cx, "clone-dest-input");
    cx.simulate_keystrokes("ctrl-a backspace");
    cx.run_until_parked();
    dialog_draw(cx);
    dialog_click(cx, "btn-accent-Clone");
    assert!(
        cx.debug_bounds("clone-overlay").is_some(),
        "a missing destination keeps the dialog open"
    );
    assert!(
        cx.debug_bounds("btn-accent-Cloning…").is_none(),
        "the clone must not have been sent"
    );

    // a valid destination passes validation: the button flips to busy (the
    // net channel has no worker in the probe, so the result never settles).
    // The platform temp dir: validation wants an existing folder, and `/tmp`
    // is only one on Unix.
    dialog_click(cx, "clone-dest-input");
    let dest = std::env::temp_dir().to_string_lossy().into_owned();
    cx.simulate_input(&dest);
    cx.run_until_parked();
    dialog_draw(cx);
    dialog_click(cx, "btn-accent-Clone");
    assert!(
        cx.debug_bounds("btn-accent-Cloning…").is_some(),
        "validation passed and the commit went out"
    );
    assert!(cx.debug_bounds("clone-overlay").is_some());
}

// ---- repo groups by folder -------------------------------------------------

/// Leaks a per-key selector for debug_bounds (which wants 'static).
fn key_selector(prefix: &str, key: &str) -> &'static str {
    Box::leak(format!("{prefix}{key}").into_boxed_str())
}

/// A scratch-settings store with grouping on: two roots in disjoint trees,
/// two repos under root A, one under root B, one under neither (the "Other"
/// fallback). The default depth (1) makes each disjoint root its own header.
fn grouped_store(cx: &mut TestAppContext, probe: &str) -> gpui::Entity<Store> {
    let scratch = std::env::temp_dir().join(format!(
        "warp-station-groups-{probe}-{}.json",
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
            std::sync::Arc::new(warp_station_core::settings::SettingsService::at(
                scratch.clone(),
            )),
        );
        store.replace_repos(vec![
            entry("alpha", "/roots/a/alpha", &[]),
            entry("beta", "/roots/a/beta", &[]),
            entry("gamma", "/second/b/gamma", &[]),
            entry("delta", "/elsewhere/delta", &[]),
        ]);
        store
    })
}

/// The grouped list: bands render keyed by their root and interleave before
/// their repos, and the "Other" fallback sorts last.
#[gpui::test]
fn grouped_bands_render_in_folder_order(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    scratch_home();
    cx.update(warp_station::theme::apply_kit_theme);

    let store = grouped_store(cx, "render");
    let store_handle = store.clone();
    let (_window_handle, cx) =
        cx.add_window_view(|window, cx| MainWindow::new(store_handle.clone(), window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    let sketch = cx.read_entity(&store, |s, _| {
        s.projected
            .iter()
            .map(|row| match row {
                ProjectedRow::Repo(ix) => s.repos[*ix].name.clone(),
                ProjectedRow::Group { key, count, .. } => format!("#{key}({count})"),
            })
            .collect::<Vec<_>>()
    });
    assert_eq!(
        sketch,
        vec![
            "#/roots/a(2)",
            "alpha",
            "beta",
            "#/second/b(1)",
            "gamma",
            "#other(1)",
            "delta",
        ],
        "headers interleave before their group's repos; the fallback group sorts last"
    );

    for key in ["/roots/a", "/second/b", "other"] {
        assert!(
            cx.debug_bounds(key_selector("repo-band-", key)).is_some(),
            "the {key} band renders"
        );
        assert!(
            cx.debug_bounds(key_selector("repo-band-solo-", key))
                .is_some(),
            "the {key} solo glyph renders"
        );
    }

    let _ = std::fs::remove_file(std::env::temp_dir().join(format!(
        "warp-station-groups-render-{}.json",
        std::process::id()
    )));
}

/// Depth-level grouping: depth 1 puts the one header on the scan roots'
/// shared folder (the default), pressing it persists that key, and
/// set_group_depth(2) live-moves the header one folder up without a rescan.
#[gpui::test]
fn depth_one_headers_sit_on_the_shared_folder_and_climb(cx: &mut TestAppContext) {
    use warp_station::net::NetCommand;
    cx.update(gpui_kit::init);
    scratch_home();
    cx.update(warp_station::theme::apply_kit_theme);

    let scratch = std::env::temp_dir().join(format!(
        "warp-station-groups-depth-{}.json",
        std::process::id()
    ));
    std::fs::write(
        &scratch,
        r#"{"Repos": {"RepoScanFolders": ["/roots/a"], "GroupReposByRoot": true}}"#,
    )
    .expect("scratch settings");
    let (net_tx, net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| {
        let mut store = Store::with_settings(
            net_tx,
            std::sync::Arc::new(warp_station_core::settings::SettingsService::at(
                scratch.clone(),
            )),
        );
        store.replace_repos(vec![
            entry("alpha", "/roots/a/work/alpha", &[]),
            entry("beta", "/roots/a/work/beta", &[]),
            entry("gamma", "/roots/a/side/gamma", &[]),
        ]);
        store
    });
    let store_handle = store.clone();
    let (_window_handle, cx) =
        cx.add_window_view(|window, cx| MainWindow::new(store_handle.clone(), window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    let sketch = cx.read_entity(&store, |s, _| {
        s.projected
            .iter()
            .map(|row| match row {
                ProjectedRow::Repo(ix) => s.repos[*ix].name.clone(),
                ProjectedRow::Group { key, count, .. } => format!("#{key}({count})"),
            })
            .collect::<Vec<_>>()
    });
    assert_eq!(
        sketch,
        vec!["#/roots/a(3)", "alpha", "beta", "gamma"],
        "depth 1 folds every repo under the root into one header"
    );
    assert!(
        cx.debug_bounds("repo-band-/roots").is_none(),
        "the parent-folder band is not where depth 1 puts it"
    );
    assert!(
        cx.debug_bounds("repo-band-/roots/a").is_some(),
        "the shared-folder band renders"
    );

    let band = cx
        .debug_bounds("repo-band-/roots/a")
        .expect("the shared-folder band renders");
    cx.simulate_click(band.center(), gpui::Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    assert_eq!(
        cx.read_entity(&store, |s, _| s.settings().repos.collapsed_groups.clone()),
        Some(vec!["/roots/a".to_string()]),
        "the shared-folder key persists like any group key"
    );

    cx.update_entity(&store, |s: &mut Store, _| s.set_group_depth(2));
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    let sketch = cx.read_entity(&store, |s, _| {
        s.projected
            .iter()
            .map(|row| match row {
                ProjectedRow::Repo(ix) => s.repos[*ix].name.clone(),
                ProjectedRow::Group { key, count, .. } => format!("#{key}({count})"),
            })
            .collect::<Vec<_>>()
    });
    assert_eq!(
        sketch,
        vec!["#/roots(3)", "alpha", "beta", "gamma"],
        "depth 2 raises the header to the root's parent; the old collapse key stops matching"
    );

    let mut scans = 0;
    while let Ok(command) = net_rx.try_recv() {
        if matches!(command, NetCommand::Scan { .. }) {
            scans += 1;
        }
    }
    assert_eq!(scans, 1, "the depth move must not rescan");

    let _ = std::fs::remove_file(std::env::temp_dir().join(format!(
        "warp-station-groups-depth-{}.json",
        std::process::id()
    )));
}

/// Without the grouping setting the list stays flat: no bands anywhere and one
/// row per repo.
#[gpui::test]
fn grouping_off_keeps_the_list_flat(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    scratch_home();
    cx.update(warp_station::theme::apply_kit_theme);

    // same scan roots, no GroupReposByRoot — the setting's absence means off
    let scratch = std::env::temp_dir().join(format!(
        "warp-station-groups-off-{}.json",
        std::process::id()
    ));
    std::fs::write(
        &scratch,
        r#"{"Repos": {"RepoScanFolders": ["/roots/a", "/roots/b"]}}"#,
    )
    .expect("scratch settings");
    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| {
        let mut store = Store::with_settings(
            net_tx,
            std::sync::Arc::new(warp_station_core::settings::SettingsService::at(
                scratch.clone(),
            )),
        );
        store.replace_repos(vec![
            entry("alpha", "/roots/a/alpha", &[]),
            entry("beta", "/roots/a/beta", &[]),
            entry("gamma", "/roots/b/gamma", &[]),
            entry("delta", "/elsewhere/delta", &[]),
        ]);
        store
    });
    let store_handle = store.clone();
    let (_window_handle, cx) =
        cx.add_window_view(|window, cx| MainWindow::new(store_handle.clone(), window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    assert_eq!(
        cx.read_entity(&store, |s, _| s.projected.len()),
        4,
        "one row per repo, no headers"
    );
    for key in ["/roots/a", "/roots/b", "other"] {
        assert!(
            cx.debug_bounds(key_selector("repo-band-", key)).is_none(),
            "no band renders for {key} while grouping is off"
        );
    }
    assert!(
        cx.debug_bounds("repo-row-0").is_some(),
        "row 0 is a repo row, not a header"
    );

    let _ = std::fs::remove_file(&scratch);
}

/// Held arrow keys must not rewrite settings per row (audit B2): the
/// in-memory selection moves at once, the file gets only the final row after
/// the debounce.
#[gpui::test]
fn selection_persists_once_after_the_debounce(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);
    let scratch = std::env::temp_dir().join(format!(
        "warp-station-selection-{}.json",
        std::process::id()
    ));
    std::fs::write(&scratch, "{}").expect("scratch settings");
    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| {
        let mut store = Store::with_settings(
            net_tx,
            std::sync::Arc::new(warp_station_core::settings::SettingsService::at(scratch)),
        );
        store.replace_repos(vec![
            entry("alpha", "/probe/alpha", &[]),
            entry("beta", "/probe/beta", &[]),
        ]);
        store
    });

    cx.update_entity(&store, |s: &mut Store, cx| {
        s.select_repo("/probe/alpha", cx);
        s.select_repo("/probe/beta", cx);
    });
    assert_eq!(
        store.read_with(cx, |s: &Store, _| s.settings().repos.selected_repo.clone()),
        None,
        "the burst has not written the settings file yet"
    );

    cx.executor()
        .advance_clock(std::time::Duration::from_millis(400));
    cx.run_until_parked();
    assert_eq!(
        store.read_with(cx, |s: &Store, _| s.settings().repos.selected_repo.clone()),
        Some("/probe/beta".to_string()),
        "only the row the burst landed on is persisted"
    );
}
