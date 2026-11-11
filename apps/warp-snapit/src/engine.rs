//! The drag/snap state machine.
//!
//! This is a direct port of `MouseService` + `SnapManager` from the .NET app:
//! a drag is detected by the low-level hook, the engine identifies the window
//! under the pointer, shows one overlay per active monitor, tracks the hovered
//! zone, and finally places the window on the selected zone.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

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
use crate::settings::{
    self, Config, ExcludedApplication, HoldKeyBehaviour, Layout, MouseButton, SettingsStore,
};
use crate::win32;
use crate::zones::Hover;

/// One-shot timer used to defer the actual window move after a left-button
/// snap, matching the .NET app's `Task.Delay(100)`.
const TIMER_DELAYED_MOVE: usize = 1;
const DELAYED_MOVE_MS: u32 = 100;

/// Repeating timer that ends a drag whose button-up the hook never delivered.
const TIMER_DRAG_WATCH: usize = 2;
const DRAG_WATCH_MS: u32 = 250;
/// Consecutive polls with the button up before the watchdog ends the drag.
const WATCH_MISSES: u8 = 3;
/// The pointer must also have been quiet this long, so a device that reports
/// its drag without pressing the button cannot end a drag that is moving.
const WATCH_QUIET: Duration = Duration::from_millis(300);

/// `SPI_SETWORKAREA`: the taskbar/working-area part of `WM_SETTINGCHANGE`.
const SPI_SETWORKAREA: usize = 0x2f;

#[derive(Clone, Copy)]
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
    last_move: Option<Instant>,
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
    hover: Option<Hover>,
    drag: DragState,
    pending_move: Option<PendingMove>,
    watch_misses: u8,
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
            watch_misses: 0,
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
        self.stop_watch();
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
                Some(hovered) if hovered.window == index => Some(hovered.zone),
                _ => None,
            };
            if let Err(error) = self.overlays[index].paint(&self.settings.theme, hover) {
                log::line(&format!("overlay paint failed: {error:#}"));
            }
            self.overlays[index].show();
        }
        self.start_watch();
        log::line(&format!(
            "overlays shown on {} monitor(s)",
            self.overlays.len()
        ));
    }

    fn paint_overlay(&mut self, index: usize) {
        let hover = match self.hover {
            Some(hovered) if hovered.window == index => Some(hovered.zone),
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
        self.drag.last_move = Some(Instant::now());
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
            log::line(&format!(
                "window is fullscreen; snapping skipped: {}",
                window.title
            ));
            self.stop_listening();
            return;
        }

        if self.settings.disable_for_modal && !win32::is_allowed_window_style(window.hwnd) {
            log::line(&format!(
                "window style is not snappable; snapping skipped: {}",
                window.title
            ));
            self.stop_listening();
            return;
        }

        if self.settings.drag_by_title {
            let (caption, frame) = win32::system_caption_metrics();
            if window.rect.top + caption + 2 + frame * 2 < point.1 {
                log::line("drag started below the title bar; snapping skipped");
                self.stop_listening();
                return;
            }
        }

        log::line(&format!(
            "drag detected on window {:#x}: {}x{} at ({}, {})",
            window.hwnd.0 as usize,
            window.rect.width(),
            window.rect.height(),
            window.rect.left,
            window.rect.top,
        ));
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

            // The sticky hit test needs the merged zone's own index; the
            // overlay-window index is a different space.
            let current = self.hover.and_then(|hover| hover.armed_overlay(index));

            if let Some(zone) = overlay.hit_test(point.0, point.1, current) {
                if let Some(rect) = overlay.target(zone) {
                    found = Some((index, zone, rect, overlay.monitor.dpi));
                }
                break;
            }
        }

        let new_hover = found.as_ref().map(|(index, zone, _, _)| Hover {
            window: *index,
            zone: *zone,
        });
        let old_hover = self.hover;
        self.hover = new_hover;

        if old_hover != new_hover {
            log::line(&format!("hover: {new_hover:?}"));
            // Repaint the window that lost the hover, then the one that took
            // it; a hover inside one window only needs that one repaint.
            if let Some(old) = old_hover
                && new_hover.is_none_or(|new| new.window != old.window)
            {
                self.paint_overlay(old.window);
            }
            if let Some(new) = new_hover {
                self.paint_overlay(new.window);
            }
        }

        self.drag.target = found.map(|(_, _, rect, dpi)| Target { rect, dpi });
    }

    /// `x`/`y` are the button-up position, so the drop does not depend on how
    /// many moves the coalesced queue happened to process.
    fn drag_end(&mut self, x: i32, y: i32) {
        if !self.drag.listening {
            // A stale end must still not leave overlays painted.
            self.cancel();
            return;
        }

        // Mouse moves are coalesced, so the last position the engine saw can
        // be older than the release point. Resolve the drop at the point the
        // button came up on before committing the target.
        let previous = self.drag.target;
        if self.drag.detected && self.overlays.iter().any(Overlay::is_visible) {
            self.select((x, y));
            // The release point can land a pixel outside the zone, or in an
            // area-padding gap, after the last move selected it. Keep the
            // target that was actually highlighted while the pointer is
            // inside it.
            if self.drag.target.is_none() {
                self.drag.target = previous.filter(|target| target.rect.contains(x, y));
            }
        }

        let active = self.drag.active.take();
        let target = self.drag.target.take();

        self.drag = DragState::default();
        self.runtime.listening.store(false, Ordering::Release);
        self.hide_overlays();
        self.hover = None;

        let (Some(active), Some(target)) = (active, target) else {
            log::line(&format!(
                "drop at ({x}, {y}) selected no zone; window stays put"
            ));
            return;
        };
        if target.rect.is_empty() {
            log::line(&format!("drop at ({x}, {y}) selected an empty zone"));
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

        // The hook only posts an end for the configured button, so the left
        // button (whose drag needs the delayed placement) is a setting.
        let was_left = self.settings.mouse_button == MouseButton::Left;

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
        match timer {
            TIMER_DELAYED_MOVE => {
                // SAFETY: the same window/timer pair `schedule_move` started.
                unsafe {
                    let _ = KillTimer(Some(self.hwnd), TIMER_DELAYED_MOVE);
                }

                if let Some(pending) = self.pending_move.take() {
                    Self::perform_move(pending);
                }
            }
            TIMER_DRAG_WATCH => self.watch_drag(),
            _ => {}
        }
    }

    /// Starts the timer that watches the button while the overlays are up.
    ///
    /// A low-level hook can be removed or skipped (secure desktop, a driver
    /// that swallows the release), and then no button-up ever reaches the
    /// engine: the overlays stay painted over the desktop until something else
    /// happens to clear them. Polling the physical button state recovers from
    /// that.
    fn start_watch(&mut self) {
        self.watch_misses = 0;
        if !win32::button_state_is_readable(self.settings.mouse_button) {
            return;
        }

        // SAFETY: `self.hwnd` is this engine's window and None makes WM_TIMER
        // go to its window_proc with our timer id.
        let timer = unsafe { SetTimer(Some(self.hwnd), TIMER_DRAG_WATCH, DRAG_WATCH_MS, None) };
        if timer == 0 {
            log::line("could not start the drag watchdog; a missed button-up will stick");
        }
    }

    fn stop_watch(&mut self) {
        self.watch_misses = 0;
        // SAFETY: the timer belongs to this engine's window; killing an
        // already-dead timer is a no-op.
        unsafe {
            let _ = KillTimer(Some(self.hwnd), TIMER_DRAG_WATCH);
        }
    }

    fn watch_drag(&mut self) {
        if !self.drag.listening || self.overlays.iter().all(|overlay| !overlay.is_visible()) {
            self.stop_watch();
            return;
        }

        if win32::mouse_button_down(self.settings.mouse_button) {
            self.watch_misses = 0;
            return;
        }

        // Only a drag whose input has gone quiet can be recovered safely: a
        // pointer that still moves means the hook is alive and the release is
        // the next event, not a state to repair.
        if self
            .drag
            .last_move
            .is_some_and(|last| last.elapsed() < WATCH_QUIET)
        {
            self.watch_misses = 0;
            return;
        }

        self.watch_misses = self.watch_misses.saturating_add(1);
        if self.watch_misses < WATCH_MISSES {
            return;
        }

        let point = win32::cursor_position().unwrap_or_else(|| self.runtime.latest_point());
        log::line(&format!(
            "button-up hook was missed; finishing the drag at ({}, {})",
            point.0, point.1
        ));
        self.drag_end(point.0, point.1);
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
                // The point packed like every other engine message: x in
                // wparam, y in lparam.
                engine.drag_end(wparam.0 as u32 as i32, lparam.0 as u32 as i32);
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
