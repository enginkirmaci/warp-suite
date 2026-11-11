//! The About screen: the rail opens it on the app you are in, the list moves
//! the selection, Copy version writes the one-line contract, and the license
//! card stays inside the app.

use gpui::TestAppContext;
use gpui::prelude::*;
use warp_station::store::Store;
use warp_station::ui::window::MainWindow;

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

fn clipboard_text(cx: &gpui::VisualTestContext) -> Option<String> {
    cx.read_from_clipboard().and_then(|item| item.text())
}

/// Scratch settings: the About screen must not read or write the developer's
/// live `~/.warp/station/settings.json`.
fn scratch_service(name: &str) -> std::sync::Arc<warp_station_core::settings::SettingsService> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!(
        "warp-station-probe-about-{name}-{}-{seq}.json",
        std::process::id()
    ));
    std::fs::write(&path, "{}").expect("scratch settings");
    std::sync::Arc::new(warp_station_core::settings::SettingsService::at(path))
}

#[gpui::test]
fn about_opens_on_station_and_copies_the_selected_app(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| Store::with_settings(net_tx, scratch_service("copy")));
    let handle = store.clone();
    let (_window, cx) = cx.add_window_view(|window, cx| MainWindow::new(handle, window, cx));
    draw(cx);

    click(cx, "rail-about");

    // The family is in the list, and the current app owns the primary.
    for key in [
        "about-row-station",
        "about-row-agent",
        "about-row-notes",
        "about-row-snapit",
    ] {
        cx.debug_bounds(key)
            .unwrap_or_else(|| panic!("{key} renders"));
    }
    cx.debug_bounds("about-primary").expect("the primary");

    // Picking Notes moves the detail and the copy line with it.
    click(cx, "about-row-notes");
    click(cx, "about-copy-version");
    let line = clipboard_text(cx).expect("copy version writes the clipboard");
    let version = env!("CARGO_PKG_VERSION");
    assert!(
        line.starts_with(&format!("Warp Notes {version} (")),
        "the copy line names the selected app, got {line:?}"
    );
    assert!(line.ends_with("· dev"), "a plain build says dev: {line:?}");
}

#[gpui::test]
fn the_license_card_opens_and_closes_over_the_page(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| Store::with_settings(net_tx, scratch_service("license")));
    let handle = store.clone();
    let (_window, cx) = cx.add_window_view(|window, cx| MainWindow::new(handle, window, cx));
    draw(cx);

    click(cx, "rail-about");
    assert!(
        cx.debug_bounds("about-license-dialog").is_none(),
        "the card starts closed"
    );

    click(cx, "about-link-license");
    cx.debug_bounds("about-license-dialog")
        .expect("View license opens the card");

    click(cx, "about-license-close");
    assert!(
        cx.debug_bounds("about-license-dialog").is_none(),
        "Close dismisses the card"
    );
    // The page is still there underneath.
    cx.debug_bounds("about-page").expect("the page stays");
}
