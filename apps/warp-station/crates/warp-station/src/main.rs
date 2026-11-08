//! Warp Station — the GPUI repository workbench.
// no console window behind the packaged Windows exe; debug builds keep it for logs
#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

use gpui::prelude::*;
use gpui::{App, Bounds, TitlebarOptions, WindowBounds, WindowOptions, px, size};
use gpui_platform::application;
use warp_station::{assets, logging, store::Store, theme, ui::window::MainWindow};

fn main() {
    // The Hyprland global-bind payload: ping the running instance's net
    // thread and exit before any logging/GPUI startup.
    #[cfg(target_os = "linux")]
    if std::env::args().nth(1).as_deref() == Some(warp_station_core::hotkey::COPY_PASSWORD_ARG) {
        if let Err(err) = warp_station_core::hotkey::ping_copy_password() {
            eprintln!("warp-station: clipboard password ping failed: {err:#}");
            std::process::exit(1);
        }
        return;
    }

    // a live instance owns the single-instance door: hand it this launch and
    // exit before migrations or logging touch the same files
    if warp_station::ipc::try_forward() {
        return;
    }

    // migrations run before logging so the fresh ~/.warp/logs never collides
    // with the legacy one being absorbed
    // the data root moved to the shared ~/.warp suite root: absorb the
    // legacy ~/.devtools once
    warp_station_core::paths::migrate_legacy_data();

    // the prompts and the catalog cache shared one folder (`opencode`, briefly
    // `ai`) before the AI surface moved to warp-agent: split them out once
    warp_station_core::paths::migrate_legacy_ai_data();

    // every app now has its own folder under ~/.warp, and the notes store
    // moved to ~/notes: adopt the root files and the old store once
    warp_station_core::paths::migrate_suite_layout();

    let _log_guard = logging::initialize();

    application()
        .with_assets(assets::EmbedAssets::new())
        .run(|cx: &mut App| {
            let (_net_handle, net_sender, event_rx) = warp_station::net::spawn_net_thread();
            let store = cx.new(|cx| Store::new(cx, net_sender));
            // the shared startup: kit init, the shared text size (seeded from
            // this app's old setting) and palette, follow hooks, the first
            // kit-theme pass and reduced motion
            let (text_size, reduce_motion) = {
                let s = store.read(cx);
                (
                    s.settings().general.text_size.clone(),
                    s.settings().general.reduce_motion,
                )
            };
            warpui_kit_core::bootstrap::init(
                cx,
                text_size.as_deref(),
                reduce_motion,
                |cx: &mut gpui::App| {
                    theme::apply_kit_theme(cx);
                },
            );

            // the companion watches this process (--parent) and exits when Warp Station dies
            #[cfg(windows)]
            {
                let snapit = store.read(cx).settings().snapit;
                if snapit.auto_start {
                    if let Err(err) = warp_station_core::snapit::start(&snapit.executable) {
                        tracing::warn!("SnapIt auto-start failed: {err}");
                    }
                }
            }

            #[cfg(windows)]
            if store.read(cx).settings().keep_awake.enabled {
                if let Err(err) = warp_station_core::keepawake::set_enabled(true) {
                    tracing::warn!("Keep awake auto-start failed: {err}");
                }
            }

            // net events → store; the timer-fallback cadence, the burst cap
            // and the gpui-rev caveat live in the pump
            warpui_kit_core::pump::spawn(cx, event_rx, {
                let store = store.downgrade();
                move |cx, event| {
                    let Some(store) = store.upgrade() else {
                        return false;
                    };
                    store.update(cx, |s, cx| s.on_net_event(event, cx));
                    true
                }
            });

            // toasts expire on a once-per-second beat; the same beat tracks the
            // window focus the notification gate needs and the activity clock
            let notice_store = store.downgrade();
            cx.spawn(async move |cx| {
                loop {
                    cx.background_executor()
                        .timer(std::time::Duration::from_secs(1))
                        .await;
                    if let Some(store) = notice_store.upgrade() {
                        store.update(cx, |s, cx| {
                            s.tick_notices(cx);
                            s.check_window_focus(cx);
                            s.check_activity_refresh(cx);
                        });
                    } else {
                        break;
                    }
                }
            })
            .detach();

            let bounds = Bounds::centered(None, size(px(1500.), px(960.)), cx);
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    // the rail, the list and a usable detail pane
                    window_min_size: Some(size(px(960.), px(560.))),
                    // empty by default in GPUI — needed so Hyprland/launchers match
                    // StartupWMClass=warp-station in the desktop entry
                    app_id: Some("warp-station".into()),
                    titlebar: Some(TitlebarOptions {
                        title: Some("Warp Station".into()),
                        appears_transparent: true,
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                cx,
                |window, cx| {
                    let window_store = store.clone();
                    cx.new(|cx| MainWindow::new(window_store, window, cx))
                },
            )
            .unwrap();

            cx.on_window_closed(|cx, _window_id| {
                // the notes window closing must not quit the app
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            cx.activate(true);
        });
}
