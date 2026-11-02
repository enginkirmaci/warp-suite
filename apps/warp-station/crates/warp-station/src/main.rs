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

    // migrations run before logging so the fresh ~/.warp/logs never collides
    // with the legacy one being absorbed
    // the data root moved to the shared ~/.warp suite root: absorb the
    // legacy ~/.devtools once
    warp_station_core::paths::migrate_legacy_data();

    // the prompts and the catalog cache shared one folder (`opencode`, briefly
    // `ai`) before the AI surface moved to warp-agent: split them out once
    warp_station_core::paths::migrate_legacy_ai_data();

    let _log_guard = logging::initialize();

    application()
        .with_assets(assets::EmbedAssets::new())
        .run(|cx: &mut App| {
            // kit components (input decorations, popovers) + our palette mapped
            // onto their theme tokens — must run before any kit component renders
            gpui_kit::init(cx);

            let (_net_handle, net_sender, event_rx) = warp_station::net::spawn_net_thread();
            let store = cx.new(|cx| Store::new(cx, net_sender));
            warpui_kit_core::adopt_text_size(
                store.read(cx).settings().general.font_scale.as_deref(),
            );
            warpui_kit_core::adopt_palette(None);
            warpui_kit_core::follow_palette(|cx: &mut gpui::App| {
                theme::apply_kit_theme(cx);
                cx.refresh_windows();
            });
            theme::apply_kit_theme(cx);
            warpui_kit_core::follow_ui_scale(cx, theme::apply_kit_theme);
            warpui_kit_core::motion::set_reduced(
                store.read(cx).settings().general.reduce_motion
                    || warpui_kit_core::motion::system_prefers_reduced(),
            );

            // the companion watches this process (--parent) and exits when Warp Station dies
            #[cfg(windows)]
            {
                let snap_it = store.read(cx).settings().snap_it;
                if snap_it.auto_start {
                    if let Err(err) = warp_station_core::snapit::start(&snap_it.executable) {
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

            // net events → store. Polled with a timer fallback: this gpui rev
            // does not service cross-thread channel wakeups on foreground tasks,
            // so a plain recv().await would sleep forever (async_channel recv is
            // cancel-safe, re-creating the future each lap is fine). A wake drains
            // the whole pending burst in one pass — one timer and one frame per
            // burst instead of one per event.
            let event_store = store.downgrade();
            cx.spawn(async move |cx| {
                loop {
                    let timeout = cx
                        .background_executor()
                        .timer(std::time::Duration::from_millis(200));
                    let mut recv = Box::pin(event_rx.recv());
                    match futures_util::future::select(recv.as_mut(), timeout).await {
                        futures_util::future::Either::Left((received, _)) => {
                            let Ok(first) = received else { break };
                            let Some(store) = event_store.upgrade() else {
                                break;
                            };
                            store.update(cx, |s, cx| s.on_net_event(first, cx));
                            let mut batch = 1;
                            while batch < 64 {
                                match event_rx.try_recv() {
                                    Ok(event) => {
                                        store.update(cx, |s, cx| s.on_net_event(event, cx));
                                        batch += 1;
                                    }
                                    Err(_) => break,
                                }
                            }
                        }
                        futures_util::future::Either::Right((_, _)) => continue,
                    }
                }
            })
            .detach();

            // toasts expire on a once-per-second beat; the same beat checks
            // the periodic fetch schedule
            let notice_store = store.downgrade();
            cx.spawn(async move |cx| {
                loop {
                    cx.background_executor()
                        .timer(std::time::Duration::from_secs(1))
                        .await;
                    if let Some(store) = notice_store.upgrade() {
                        store.update(cx, |s, cx| {
                            s.tick_notices(cx);
                            s.check_auto_fetch(cx);
                        });
                    } else {
                        break;
                    }
                }
            })
            .detach();

            let bounds = Bounds::centered(None, size(px(1280.), px(820.)), cx);
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
