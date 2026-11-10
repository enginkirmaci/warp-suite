//! The drag/snap state machine.
//!
//! This is a direct port of `MouseService` + `SnapManager` from the .NET app:
//! a drag is detected by the low-level hook, the engine identifies the window
//! under the pointer, shows one overlay per active monitor, tracks the hovered
//! zone, and finally places the window on the selected zone.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use anyhow::Result;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    DefWindowProcW, GWLP_USERDATA, GetWindowLongPtrW, KillTimer, PostQuitMessage, SetTimer,
    WM_DISPLAYCHANGE, WM_SETTINGCHANGE, WM_TIMER,
};

use crate::cornering::Cornering;
use crate::geometry::Rect;
use crate::hooks::{
    Hooks, Runtime, WM_APP_CANCEL, WM_APP_DRAG_END, WM_APP_DRAG_START, WM_APP_MOUSE_MOVE,
    WM_APP_SHUTDOWN,
};
use crate::log;
use crate::overlay::Overlay;
use crate::settings::{self, Config, ExcludedApplication, HoldKeyBehaviour, Layout, SettingsStore};
use crate::win32;
use crate::zones::ZoneRef;

/// One-shot timer used to defer the actual window move after a left-button
/// snap, matching the .NET app's `Task.Delay(100)`.
const TIMER_DELAYED_MOVE: usize = 1;
const DELAYED_MOVE_MS: u32 = 100;

/// `SPI_SETWORKAREA`: the taskbar/working-area part of `WM_SETTINGCHANGE`.
const SPI_SETWORKAREA: usize = 0x2f;

struct Target {
    rect: Rect,
    dpi: u32,
}

struct ActiveWindow {
    hwnd: HWND,
    rect: Rect,
    dpi: u32,
}

struct PendingMove {
    window: HWND,
    rect: Rect,
    repeat: bool,
}

#[derive(Default)]
struct DragState {
    listening: bool,
    detected: bool,
    start: (i32, i32),
    active: Option<ActiveWindow>,
    target: Option<Target>,
}

pub struct Engine {
    hwnd: HWND,
    runtime: Arc<Runtime>,
    /// The live settings: reads deref, and any mutation goes through
    /// `SettingsStore::update`, which lands the change on disk atomically.
    settings: SettingsStore,
    layouts: Vec<Layout>,
    excluded: Vec<ExcludedApplication>,
    overlays: Vec<Overlay>,
    hover: Option<(usize, ZoneRef)>,
    drag: DragState,
    pending_move: Option<PendingMove>,
    hooks: Option<Hooks>,
    cornering: Option<Cornering>,
}

impl Engine {
    pub fn new(hwnd: HWND, config: Config, runtime: Arc<Runtime>) -> Result<Self> {
        let Config {
            settings,
            excluded_applications,
            layouts,
        } = config;

        let mut engine = Self {
            hwnd,
            runtime,
            settings: SettingsStore::at(settings::settings_file(), settings),
            layouts,
            excluded: excluded_applications,
            overlays: Vec::new(),
            hover: None,
            drag: DragState::default(),
            pending_move: None,
            hooks: None,
            cornering: None,
        };

        engine.rebuild_overlays();

        if engine.settings.enable_automatic_window_cornering {
            engine.cornering = Cornering::start(engine.excluded.clone());
        }

        Ok(engine)
    }

    /// Installs the global hooks. The .NET app only listens when `EnableMouse`
    /// is true; keep that behaviour so the companion can be disabled without
    /// uninstalling it.
    pub fn start_hooks(&mut self) -> Result<()> {
        if !self.settings.enable_mouse {
            log::line("EnableMouse is false; snap hooks are not installed");
            return Ok(());
        }

        self.hooks = Some(Hooks::install(Arc::clone(&self.runtime))?);
        log::line("snap hooks installed");
        Ok(())
    }

    fn rebuild_overlays(&mut self) {
        self.reset_drag();
        self.overlays.clear();

        let mut overlays = Vec::new();
        for monitor in win32::monitors() {
            let keys = monitor.keys();
            if settings::is_deactivated(&self.settings, &keys) {
                log::line(&format!("monitor {} is deactivated", monitor.device));
                continue;
            }

            let layout = settings::layout_for(&self.settings, &self.layouts, &keys);
            match Overlay::new(monitor.clone(), layout, &self.settings.theme) {
                Ok(overlay) => overlays.push(overlay),
                Err(error) => log::line(&format!(
                    "could not create overlay for {}: {error:#}",
                    monitor.device
                )),
            }
        }

        log::line(&format!("{} overlay window(s) ready", overlays.len()));
        self.overlays = overlays;
    }

    fn reset_drag(&mut self) {
        self.drag = DragState::default();
        self.runtime.listening.store(false, Ordering::Release);
        self.hide_overlays();
        self.hover = None;
    }

    fn stop_listening(&mut self) {
        self.drag.listening = false;
        self.drag.detected = false;
        self.drag.target = None;
        self.runtime.listening.store(false, Ordering::Release);
        self.hide_overlays();
        self.hover = None;
    }

    fn hide_overlays(&mut self) {
        for overlay in &mut self.overlays {
            overlay.hide();
        }
    }

    /// Shows the overlays on the first move after drag detection, then leaves
    /// them up. Selection runs on that same move: the hit test is arithmetic
    /// and does not need the windows to exist first, and skipping it lost the
    /// target when the pointer stopped on a zone and was released at once.
    fn show_overlays_if_needed(&mut self) {
        if self.overlays.iter().all(Overlay::is_visible) {
            return;
        }

        for index in 0..self.overlays.len() {
            let hover = match self.hover {
                Some((hovered, zone)) if hovered == index => Some(zone),
                _ => None,
            };
            if let Err(error) = self.overlays[index].paint(&self.settings.theme, hover) {
                log::line(&format!("overlay paint failed: {error:#}"));
            }
            self.overlays[index].show();
        }
    }

    fn paint_overlay(&mut self, index: usize) {
        let hover = match self.hover {
            Some((hovered, zone)) if hovered == index => Some(zone),
            _ => None,
        };
        if let Some(overlay) = self.overlays.get_mut(index)
            && let Err(error) = overlay.paint(&self.settings.theme, hover)
        {
            log::line(&format!("overlay paint failed: {error:#}"));
        }
    }

    fn drag_start(&mut self, x: i32, y: i32) {
        self.drag = DragState {
            listening: true,
            start: (x, y),
            ..DragState::default()
        };
        self.hover = None;
        self.runtime.listening.store(true, Ordering::Release);
    }

    fn holding_key_result(&mut self) -> bool {
        if !self.settings.enable_hold_key {
            return true;
        }

        let holding = self.runtime.holding_key.load(Ordering::Acquire);
        match self.settings.hold_key_behaviour {
            HoldKeyBehaviour::HoldToEnable => holding,
            HoldKeyBehaviour::HoldToDisable => {
                if holding {
                    self.cancel();
                    false
                } else {
                    true
                }
            }
        }
    }

    fn delay_done(&self, point: (i32, i32)) -> bool {
        if self.settings.enable_hold_key {
            return true;
        }

        let moved = (point.0 - self.drag.start.0).abs() + (point.1 - self.drag.start.1).abs();
        moved > self.settings.mouse_drag_delay
    }

    fn on_move(&mut self) {
        let point = self.runtime.take_move();
        if !self.drag.listening {
            return;
        }
        if !self.holding_key_result() {
            return;
        }
        if !self.delay_done(point) {
            return;
        }

        if !self.drag.detected {
            self.detect(point);
            return;
        }

        self.show_overlays_if_needed();
        self.select(point);
    }

    fn detect(&mut self, point: (i32, i32)) {
        // Mirrors MouseService.ProcessSingleMove: a detected snap counts as
        // "hold key used" so the Win key-up can be swallowed.
        self.runtime.hold_key_used.store(true, Ordering::Release);

        let Some(window) = win32::foreground_window() else {
            self.stop_listening();
            return;
        };

        let dpi = win32::dpi_at_point(point.0, point.1);

        if settings::is_excluded(&self.excluded, &window.title) {
            log::line(&format!("window excluded from snapping: {}", window.title));
            self.stop_listening();
            return;
        }

        if self.settings.disable_for_fullscreen && win32::is_fullscreen(&window) {
            self.stop_listening();
            return;
        }

        if self.settings.disable_for_modal && !win32::is_allowed_window_style(window.hwnd) {
            self.stop_listening();
            return;
        }

        if self.settings.drag_by_title {
            let (caption, frame) = win32::system_caption_metrics();
            if window.rect.top + caption + 2 + frame * 2 < point.1 {
                self.stop_listening();
                return;
            }
        }

        self.drag.detected = true;
        self.drag.active = Some(ActiveWindow {
            hwnd: window.hwnd,
            rect: window.rect,
            dpi,
        });
    }

    fn select(&mut self, point: (i32, i32)) {
        let mut found = None;

        for (index, overlay) in self.overlays.iter().enumerate() {
            if !overlay.is_visible() {
                continue;
            }

            let current = match self.hover {
                Some((hovered, _)) if hovered == index => Some(index),
                _ => None,
            };

            if let Some(zone) = overlay.hit_test(point.0, point.1, current) {
                if let Some(rect) = overlay.target(zone) {
                    found = Some((index, zone, rect, overlay.monitor.dpi));
                }
                break;
            }
        }

        let new_hover = found.as_ref().map(|(index, zone, _, _)| (*index, *zone));
        let old_hover = self.hover;
        self.hover = new_hover;

        if old_hover.map(|(index, _)| index) != new_hover.map(|(index, _)| index)
            && let Some((index, _)) = old_hover
        {
            self.paint_overlay(index);
        }
        if let Some((index, _)) = new_hover {
            self.paint_overlay(index);
        }

        self.drag.target = found.map(|(_, _, rect, dpi)| Target { rect, dpi });
    }

    fn drag_end(&mut self, was_left: bool) {
        if !self.drag.listening {
            return;
        }

        // Mouse moves are coalesced, so the last position the engine saw can
        // be older than the release point. Resolve the drop at the latest
        // pointer position before committing the target.
        if self.drag.detected && self.overlays.iter().any(Overlay::is_visible) {
            self.select(self.runtime.latest_point());
        }

        let active = self.drag.active.take();
        let target = self.drag.target.take();

        self.drag = DragState::default();
        self.runtime.listening.store(false, Ordering::Release);
        self.hide_overlays();
        self.hover = None;

        let (Some(active), Some(target)) = (active, target) else {
            return;
        };
        if target.rect.is_empty() {
            return;
        }

        let mut rect = target.rect;
        if let Some(with_margin) = win32::extended_frame_bounds(active.hwnd)
            && !with_margin.is_empty()
        {
            win32::expand_by_frame_margins(&mut rect, active.rect, with_margin);
        }

        // The target carries the DPI of the screen it was selected on; the
        // active window carries the DPI at the pointer when the drag started.
        // Windows needs a second placement when the window crosses a monitor
        // with a different scale factor.
        let repeat = target.dpi != active.dpi;

        if was_left {
            self.schedule_move(active.hwnd, rect, repeat);
        } else {
            Self::perform_move(PendingMove {
                window: active.hwnd,
                rect,
                repeat,
            });
        }
    }

    fn schedule_move(&mut self, window: HWND, rect: Rect, repeat: bool) {
        self.pending_move = Some(PendingMove {
            window,
            rect,
            repeat,
        });

        // SAFETY: `self.hwnd` is the engine window this engine belongs to and
        // None makes WM_TIMER go to its window_proc with our timer id.
        let timer = unsafe { SetTimer(Some(self.hwnd), TIMER_DELAYED_MOVE, DELAYED_MOVE_MS, None) };
        if timer == 0
            && let Some(pending) = self.pending_move.take()
        {
            Self::perform_move(pending);
        }
    }

    fn on_timer(&mut self, timer: usize) {
        if timer != TIMER_DELAYED_MOVE {
            return;
        }

        // SAFETY: the same window/timer pair `schedule_move` started.
        unsafe {
            let _ = KillTimer(Some(self.hwnd), TIMER_DELAYED_MOVE);
        }

        if let Some(pending) = self.pending_move.take() {
            Self::perform_move(pending);
        }
    }

    fn perform_move(pending: PendingMove) {
        win32::move_window(pending.window, pending.rect);
        if pending.repeat {
            win32::move_window(pending.window, pending.rect);
        }
    }

    fn cancel(&mut self) {
        self.drag = DragState::default();
        self.runtime.listening.store(false, Ordering::Release);
        self.hide_overlays();
        self.hover = None;
    }

    fn display_changed(&mut self) {
        log::line("display configuration changed; rebuilding overlays");
        self.rebuild_overlays();
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        // SAFETY: the engine window still exists while this box is alive
        // (run() destroys it only after reclaiming the pointer), so stopping
        // this engine's one timer is valid.
        unsafe {
            let _ = KillTimer(Some(self.hwnd), TIMER_DELAYED_MOVE);
        }
        self.hide_overlays();
        self.hooks = None;
        self.cornering = None;
    }
}

/// # Safety
///
/// Only usable as the `WNDPROC` of the engine window class: `hwnd`'s
/// `GWLP_USERDATA` must hold the `Box::into_raw` `Engine` pointer (or 0) for as
/// long as the window can receive messages, which `app::run` guarantees. The
/// OS owns the parameter values for the duration of the call.
pub unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // SAFETY: the window lives for the duration of the call and
    // GWLP_USERDATA stores our own pointer-sized engine handle.
    let raw = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) };

    if raw != 0 {
        // SAFETY: app::run stores the Box::into_raw pointer here before any
        // message can be dispatched and clears it before reclaiming the box,
        // so a nonzero value is a valid, unaliased Engine on this thread.
        let engine = unsafe { &mut *(raw as *mut Engine) };

        match message {
            WM_APP_DRAG_START => {
                engine.drag_start(wparam.0 as u32 as i32, lparam.0 as u32 as i32);
                return LRESULT(0);
            }
            WM_APP_MOUSE_MOVE => {
                engine.on_move();
                return LRESULT(0);
            }
            WM_APP_DRAG_END => {
                engine.drag_end(wparam.0 != 0);
                return LRESULT(0);
            }
            WM_APP_CANCEL => {
                engine.cancel();
                return LRESULT(0);
            }
            WM_APP_SHUTDOWN => {
                // SAFETY: PostQuitMessage only touches this thread's queue.
                unsafe { PostQuitMessage(0) };
                return LRESULT(0);
            }
            WM_TIMER => {
                engine.on_timer(wparam.0);
                return LRESULT(0);
            }
            WM_DISPLAYCHANGE => {
                engine.display_changed();
                return LRESULT(0);
            }
            WM_SETTINGCHANGE => {
                if wparam.0 == SPI_SETWORKAREA {
                    engine.display_changed();
                }
                return LRESULT(0);
            }
            _ => {}
        }
    }

    // SAFETY: the parameters are the ones the OS passed to this wndproc.
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}
