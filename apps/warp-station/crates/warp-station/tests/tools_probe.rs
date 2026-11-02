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

#[gpui_kit::gpui::test]
fn clipboard_password_drawer_draws(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| Store::new(cx, net_tx));
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
    let store = cx.new(|cx| Store::new(cx, net_tx));
    let probe_store = store.clone();
    let (_handle, cx) = cx.add_window_view(|window, cx| NugetDrawer::new(store, window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    cx.update(|window, cx| drop(window.draw(cx)));

    // no watch is running just from opening the drawer
    assert!(!cx.update(|_, cx| probe_store.read(cx).nuget.watching));
}

#[gpui_kit::gpui::test]
fn the_formatter_field_and_its_button_share_a_height(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| Store::new(cx, net_tx));
    let (_handle, cx) = cx.add_window_view(|window, cx| FormattersDrawer::new(store, window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    let field = cx.debug_bounds("fmt-input").expect("field");
    let run = cx.debug_bounds("fmt-run").expect("button");
    assert_eq!(field.size.height, run.size.height);
    assert_eq!(field.origin.y, run.origin.y);
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
    let store = cx.new(|cx| Store::new(cx, net_tx));
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
    let store = cx.new(|cx| Store::new(cx, net_tx));
    let (_handle, cx) = cx
        .add_window_view(|window, cx| warp_station::ui::window::MainWindow::new(store, window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    cx.update(|window, cx| drop(window.draw(cx)));
    unsafe { std::env::remove_var("WARP_STATION_PAGE") };
    assert!(cx.debug_bounds("tool-page").is_some(), "the page draws");
    assert!(cx.debug_bounds("tool-formatters").is_some(), "with its row");
}
