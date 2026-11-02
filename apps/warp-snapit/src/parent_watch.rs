//! Ties the companion's lifetime to Warp Station (`--parent <pid>`).

use std::ffi::c_void;
use std::thread;

use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM, WPARAM};
use windows::Win32::System::Threading::{
    INFINITE, OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

use crate::hooks::WM_APP_SHUTDOWN;

/// Watches `pid` on a background thread and posts a shutdown message to the
/// engine when it exits. Returns false when the parent is already gone.
pub fn watch(pid: u32, engine: HWND) -> bool {
    // SAFETY: SYNCHRONIZE is the only access right needed; the returned
    // handle is owned by the watcher thread, or closed below on spawn failure.
    let Ok(handle) = (unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, pid) }) else {
        return false;
    };

    let raw_engine = engine.0 as isize;
    let raw_handle = handle.0 as isize;
    let spawned = thread::Builder::new()
        .name("snapit-parent-watch".into())
        .spawn(move || {
            let handle = HANDLE(raw_handle as *mut c_void);
            // SAFETY: `handle` is owned by this thread alone and closed here;
            // the engine window may be gone, which only fails the post.
            unsafe {
                let _ = WaitForSingleObject(handle, INFINITE);
                let _ = PostMessageW(
                    Some(HWND(raw_engine as *mut c_void)),
                    WM_APP_SHUTDOWN,
                    WPARAM(0),
                    LPARAM(0),
                );
                let _ = CloseHandle(handle);
            }
        });

    if spawned.is_err() {
        // SAFETY: the watcher thread was not spawned, so this thread still
        // owns the handle and closes it here.
        unsafe {
            let _ = CloseHandle(handle);
        }
        return false;
    }

    true
}
