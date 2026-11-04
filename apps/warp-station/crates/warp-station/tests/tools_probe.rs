//! Headless draw probes for the tool pages: two draws without a panic means the
//! trees are sound (mirrors the settings/settings_probe harness).

use gpui::TestAppContext;
use gpui::prelude::*;
use warp_station::store::Store;
use warp_station::ui::clipboard_password::ClipboardPasswordDrawer;
use warp_station::ui::formatters::FormattersDrawer;
use warp_station::ui::nuget::NugetDrawer;
use warp_station::ui::windows_tools::SnapItPage;
use warp_station_core::snapit::{SnapLayout, SnapLine};

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
        "warp-station-probe-tools-{name}-{}-{seq}.json",
        std::process::id()
    ));
    std::fs::write(&path, "{}").expect("scratch settings");
    std::sync::Arc::new(warp_station_core::settings::SettingsService::at(path))
}

#[gpui_kit::gpui::test]
fn clipboard_password_drawer_draws(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| {
        Store::with_settings(
            net_tx,
            scratch_service("tools-clipboard_password_drawer_draws"),
        )
    });
    let (_handle, cx) =
        cx.add_window_view(|window, cx| ClipboardPasswordDrawer::new(store, window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    cx.update(|window, cx| drop(window.draw(cx)));
}

#[gpui_kit::gpui::test]
fn nuget_drawer_draws(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store =
        cx.new(|_cx| Store::with_settings(net_tx, scratch_service("tools-nuget_drawer_draws")));
    let probe_store = store.clone();
    let (_handle, cx) = cx.add_window_view(|window, cx| NugetDrawer::new(store, window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    cx.update(|window, cx| drop(window.draw(cx)));

    // no watch is running just from opening the drawer
    assert!(!cx.update(|_, cx| probe_store.read(cx).nuget.watching));
}

#[gpui_kit::gpui::test]
fn the_formatters_run_button_sits_in_the_head(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| {
        Store::with_settings(
            net_tx,
            scratch_service("tools-the_formatters_run_button_sits_in_the_head"),
        )
    });
    let (_handle, cx) = cx.add_window_view(|window, cx| FormattersDrawer::new(store, window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    let field = cx.debug_bounds("fmt-input").expect("field");
    let run = cx.debug_bounds("fmt-run").expect("button");
    // the prototype's input is `textarea.area` with six rows, not a one-line
    // field: a pasted block is what the page formats
    assert!(
        field.size.height >= gpui::px(100.),
        "the formatter input is the six-row area, not a 36-high field ({:?})",
        field.size.height
    );
    // the design keeps Run in the detail head, above the input, not beside it
    assert!(
        run.origin.y + run.size.height <= field.origin.y,
        "Run {run:?} should sit above the field {field:?}"
    );
    // and the mode strip is one row of choices, not a two-by-two grid
    let encode = cx.debug_bounds("fmt-mode-Base64 Encode").expect("mode");
    let decode = cx.debug_bounds("fmt-mode-Base64 Decode").expect("mode");
    assert_eq!(encode.origin.y, decode.origin.y, "modes share one row");
}

/// The tool head carries its actions and they stay inside the pane: the
/// overflow that clipped Copy was the page root taking its width from
/// `size_full` while also padding.
#[gpui_kit::gpui::test]
fn the_tool_head_actions_stay_inside_the_pane(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| {
        Store::with_settings(
            net_tx,
            scratch_service("tools-the_tool_head_actions_stay_inside_the_pane"),
        )
    });
    let (_handle, cx) =
        cx.add_window_view(|window, cx| ClipboardPasswordDrawer::new(store, window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    let pane = cx.debug_bounds("clipboard-password-drawer").expect("page");
    let copy = cx.debug_bounds("clipboard-copy").expect("Copy");
    assert!(
        copy.origin.x >= pane.origin.x && copy.right() <= pane.right(),
        "Copy {copy:?} escapes the page {pane:?}"
    );
    assert!(copy.origin.y < pane.center().y, "Copy belongs to the head");
}

/// The page reads no real folder here, and a Linux run still draws it: the
/// row that opens it is what stays Windows-only.
#[gpui_kit::gpui::test]
fn the_snapit_page_previews_its_layouts(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let layouts = vec![
        SnapLayout {
            name: "Two".into(),
            width: 100.,
            height: 50.,
            lines: vec![SnapLine {
                x: 50.,
                y: 0.,
                width: 0.,
                height: 50.,
            }],
        },
        SnapLayout {
            name: "Tall".into(),
            width: 50.,
            height: 100.,
            lines: Vec::new(),
        },
    ];
    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| {
        Store::with_settings(
            net_tx,
            scratch_service("tools-the_snapit_page_previews_its_layouts"),
        )
    });
    let (_handle, cx) = cx.add_window_view(|_, _| SnapItPage::with_layouts(store, layouts));
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    let wide = cx.debug_bounds("snapit-preview").expect("a preview");
    assert_eq!(
        wide.size.height,
        wide.size.width / 2.0,
        "the layout's ratio"
    );
    assert!(cx.debug_bounds("snapit-run").is_some());

    let pick = cx
        .debug_bounds("snapit-layout-1")
        .expect("the second layout");
    cx.simulate_click(pick.center(), gpui::Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    let tall = cx.debug_bounds("snapit-preview").expect("a preview");
    assert_eq!(tall.size.height, tall.size.width * 2.0, "the pick follows");
}

#[gpui_kit::gpui::test]
fn main_window_draws_the_tools_section(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    // SAFETY: single-threaded test setup, read once at MainWindow::new
    unsafe { std::env::set_var("WARP_STATION_PAGE", "tools") };
    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| {
        Store::with_settings(
            net_tx,
            scratch_service("tools-main_window_draws_the_tools_section"),
        )
    });
    let (_handle, cx) = cx
        .add_window_view(|window, cx| warp_station::ui::window::MainWindow::new(store, window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    cx.update(|window, cx| drop(window.draw(cx)));
    unsafe { std::env::remove_var("WARP_STATION_PAGE") };
    assert!(cx.debug_bounds("tool-page").is_some(), "the page draws");
    assert!(cx.debug_bounds("tool-formatters").is_some(), "with its row");
}

/// Both head buttons are exposed: the a11y tree is how a screen reader (and
/// the background automation this suite is tested with) reaches them.
#[gpui_kit::gpui::test]
fn both_nuget_head_buttons_are_addressable(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| {
        Store::with_settings(
            net_tx,
            scratch_service("tools-both_nuget_head_buttons_are_addressable"),
        )
    });
    let (_handle, cx) = cx.add_window_view(|window, cx| NugetDrawer::new(store, window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    let register = cx.debug_bounds("nuget-register").expect("register");
    let watch = cx.debug_bounds("nuget-watch").expect("watch");
    assert!(
        register.right() <= watch.origin.x,
        "Register {register:?} overlaps the watch button {watch:?}"
    );
}
