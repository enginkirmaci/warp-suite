//! Optional "don't round corners on new windows" feature
//! (`EnableAutomaticWindowCornering`).
//!
//! The WinEvent callback must return fast, so it only forwards the HWND to a
//! worker that waits 100 ms (windows often appear before their final size) and
//! then applies DWM's `DWMWCP_DONOTROUND`, deduplicating for 30 seconds like
//! the .NET `WindowEventService`.

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
use windows::Win32::UI::WindowsAndMessaging::{
    EVENT_OBJECT_SHOW, OBJID_WINDOW, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS,
};

use crate::log;
use crate::settings::{self, ExcludedApplication};
use crate::win32;

static EVENT_SENDER: OnceLock<mpsc::Sender<isize>> = OnceLock::new();

pub struct Cornering {
    hook: HWINEVENTHOOK,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Cornering {
    pub fn start(excluded: Vec<ExcludedApplication>) -> Option<Self> {
        let (sender, receiver) = mpsc::channel();
        if EVENT_SENDER.set(sender).is_err() {
            return None;
        }

        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let thread = thread::Builder::new()
            .name("snapit-cornering".into())
            .spawn(move || worker(receiver, thread_stop, excluded))
            .ok()?;

        // SAFETY: `win_event_proc` is a static fn that lives for the whole
        // process, and the hook is unhooked in Drop on this thread.
        let hook = unsafe {
            SetWinEventHook(
                EVENT_OBJECT_SHOW,
                EVENT_OBJECT_SHOW,
                None,
                Some(win_event_proc),
                0,
                0,
                WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
            )
        };

        if hook.0.is_null() {
            log::line("SetWinEventHook failed; window cornering disabled");
            stop.store(true, Ordering::Release);
            if let Some(sender) = EVENT_SENDER.get() {
                let _ = sender.send(0);
            }
            let _ = thread.join();
            return None;
        }

        log::line("window cornering monitoring started");
        Some(Self {
            hook,
            stop,
            thread: Some(thread),
        })
    }
}

impl Drop for Cornering {
    fn drop(&mut self) {
        // SAFETY: `self.hook` is the hook SetWinEventHook returned, unhooked
        // exactly once here before the worker thread is joined.
        unsafe {
            let _ = UnhookWinEvent(self.hook);
        }
        self.stop.store(true, Ordering::Release);
        if let Some(sender) = EVENT_SENDER.get() {
            let _ = sender.send(0);
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// # Safety
///
/// Only installed via `SetWinEventHook` (WINEVENT_OUTOFCONTEXT): Windows calls
/// it on the thread that pumps the installing thread's message loop, with the
/// parameters owned by the OS for the duration of the call.
unsafe extern "system" fn win_event_proc(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    id_object: i32,
    _id_child: i32,
    _event_thread: u32,
    _event_time: u32,
) {
    if event != EVENT_OBJECT_SHOW || id_object != OBJID_WINDOW.0 || hwnd.0.is_null() {
        return;
    }

    if let Some(sender) = EVENT_SENDER.get() {
        let _ = sender.send(hwnd.0 as isize);
    }
}

fn worker(
    receiver: mpsc::Receiver<isize>,
    stop: Arc<AtomicBool>,
    excluded: Vec<ExcludedApplication>,
) {
    let mut seen: HashMap<isize, Instant> = HashMap::new();

    while !stop.load(Ordering::Acquire) {
        let raw = match receiver.recv_timeout(Duration::from_millis(250)) {
            Ok(raw) => raw,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };

        // A zero handle is the wake-up sentinel used during shutdown.
        if raw == 0 {
            continue;
        }

        let now = Instant::now();
        seen.retain(|_, seen_at| now.duration_since(*seen_at) < Duration::from_secs(30));
        if seen.contains_key(&raw) {
            continue;
        }
        seen.insert(raw, now);

        thread::sleep(Duration::from_millis(100));

        let hwnd = HWND(raw as *mut c_void);
        if !win32::is_window_visible(hwnd) {
            continue;
        }

        let title = win32::window_text(hwnd);
        if title.trim().is_empty() || title == "Program Manager" {
            continue;
        }

        if settings::is_excluded(&excluded, &title) {
            continue;
        }

        if win32::window_rect(hwnd).is_none_or(|rect| rect.is_empty()) {
            continue;
        }

        win32::set_no_round_corners(hwnd);
    }
}
