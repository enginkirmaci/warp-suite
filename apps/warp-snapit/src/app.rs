//! Process startup: DPI awareness, single instance, window classes, hooks and
//! the message loop.

use std::ffi::c_void;
use std::sync::Arc;
use std::sync::atomic::{AtomicIsize, Ordering};

use anyhow::{Context, Result};
use windows::Win32::Foundation::{
    CloseHandle, E_ACCESSDENIED, ERROR_ALREADY_EXISTS, ERROR_SUCCESS, GetLastError, HANDLE, HWND,
    LPARAM, SetLastError, WPARAM,
};
use windows::Win32::System::Console::SetConsoleCtrlHandler;
use windows::Win32::System::Threading::{CreateMutexW, OpenMutexW, SYNCHRONIZATION_SYNCHRONIZE};
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DefWindowProcW, GWLP_USERDATA, GetWindowLongPtrW, PostMessageW, SetProcessDPIAware,
    SetWindowLongPtrW, UnregisterClassW,
};
use windows::core::{BOOL, PCWSTR, w};

use crate::cli::Cli;
use crate::engine::{self, Engine};
use crate::hooks::{Runtime, WM_APP_SHUTDOWN};
use crate::log;
use crate::parent_watch;
use crate::settings::Config;
use crate::win32;
use crate::zones::MonitorZones;

static ENGINE_WINDOW: AtomicIsize = AtomicIsize::new(0);

/// Keeps the named mutex alive for the lifetime of the process. The name is
/// deliberately different from the .NET companion's mutex so both builds can
/// be started side by side while testing.
struct InstanceGuard(HANDLE);

impl InstanceGuard {
    fn acquire() -> Result<Option<Self>> {
        // CreateMutexW only sets the last error when the name already exists;
        // clear it first so a stale value cannot look like a second instance.
        // SAFETY: SetLastError only writes the thread's last-error slot.
        unsafe {
            SetLastError(ERROR_SUCCESS);
        }
        // SAFETY: no security descriptor, and `w!` is a NUL-terminated static
        // name; the returned handle is owned below.
        let handle = unsafe { CreateMutexW(None, false, w!("Local\\DevTools.SnapIt.Rust")) }
            .context("create the SnapIt instance mutex")?;

        // SAFETY: reads the last error set by the CreateMutexW above, with no
        // intervening Win32 call.
        if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            // SAFETY: `handle` is the mutex handle returned above, closed
            // exactly once on this early-exit path.
            unsafe {
                let _ = CloseHandle(handle);
            }
            return Ok(None);
        }

        Ok(Some(Self(handle)))
    }
}

impl Drop for InstanceGuard {
    fn drop(&mut self) {
        // SAFETY: the handle was created in acquire, stored exclusively in
        // this newtype, and Drop runs exactly once.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

/// Whether the retired .NET companion holds its single-instance mutex.
///
/// `SnapIt.Common/Applications/AppInstance.cs` keeps the mutex named below for
/// the life of the process. Both companions install global mouse hooks and
/// place the same windows, so one drag is resolved twice and the two overlays
/// fight. Nothing here can stop the other process; the log line is what makes
/// the collision visible.
fn legacy_companion_running() -> bool {
    // The .NET name is a bare GUID, so it lives in this session's namespace
    // next to the named mutex the Rust app already took.
    const NAME: &str = "{FF1FFB1E-5D42-4B8F-B42A-52DA1A1964B7}";
    let name: Vec<u16> = NAME.encode_utf16().chain(std::iter::once(0)).collect();

    // SAFETY: `OpenMutexW` only reads the NUL-terminated name; the returned
    // handle, if any, is closed immediately below.
    match unsafe { OpenMutexW(SYNCHRONIZATION_SYNCHRONIZE, false, PCWSTR(name.as_ptr())) } {
        Ok(handle) => {
            // SAFETY: `handle` came from OpenMutexW and is closed once here.
            unsafe {
                let _ = CloseHandle(handle);
            }
            true
        }
        // An elevated companion owns the mutex with an ACL this process may
        // not enter; access denied still means the process is there.
        Err(error) if error.code() == E_ACCESSDENIED => true,
        Err(_) => false,
    }
}

pub fn run(cli: Cli) -> Result<()> {
    enable_dpi_awareness();

    let config = Config::load();

    if cli.dump_zones {
        dump_zones(&config);
        return Ok(());
    }

    let Some(_instance) = InstanceGuard::acquire()? else {
        log::line("another snapit instance is already running");
        return Ok(());
    };

    if legacy_companion_running() {
        log::line(
            "the .NET SnapIt companion is running too: both hook the mouse and \
             place windows, so point Warp Station's SnapIt executable at \
             warp-snapit.exe and close the old companion",
        );
    }

    let runtime = Runtime::new(config.settings.clone());

    let engine_class = w!("SnapItRustEngine");
    win32::register_class(engine_class, Some(engine::window_proc))
        .context("register the SnapIt engine window class")?;
    win32::register_class(w!("SnapItRustOverlay"), Some(overlay_proc))
        .context("register the SnapIt overlay window class")?;

    let engine_window = win32::create_message_window(engine_class)?;
    ENGINE_WINDOW.store(engine_window.0 as isize, Ordering::Release);

    let mut engine = match Engine::new(engine_window, config, Arc::clone(&runtime)) {
        Ok(engine) => Box::new(engine),
        Err(error) => {
            ENGINE_WINDOW.store(0, Ordering::Release);
            win32::destroy_window(engine_window);
            return Err(error);
        }
    };

    runtime.set_engine(engine_window);

    // Publish the engine pointer before installing hooks so an event that
    // arrives immediately is not dispatched to DefWindowProc.
    let engine_ptr = engine.as_mut() as *mut Engine;
    // SAFETY: the window was just created on this thread and the pointer stays
    // valid until it is cleared or reclaimed below.
    unsafe {
        SetWindowLongPtrW(engine_window, GWLP_USERDATA, engine_ptr as isize);
    }

    // Watch the parent only once the window can process WM_APP_SHUTDOWN, so a
    // parent that exits during startup cannot leave an orphaned companion.
    if let Some(pid) = cli.parent
        && !parent_watch::watch(pid, engine_window)
    {
        log::line(&format!("parent process {pid} not found; exiting"));
        // SAFETY: the window exists and is not pumped yet; clearing the engine
        // pointer before the box is dropped below keeps any message from
        // reaching freed memory.
        unsafe {
            SetWindowLongPtrW(engine_window, GWLP_USERDATA, 0);
        }
        drop(engine);
        ENGINE_WINDOW.store(0, Ordering::Release);
        win32::destroy_window(engine_window);
        return Ok(());
    }

    if let Err(error) = engine.start_hooks() {
        // SAFETY: the window exists and is not pumped yet; clearing the engine
        // pointer before the box is dropped below keeps any message from
        // reaching freed memory.
        unsafe {
            SetWindowLongPtrW(engine_window, GWLP_USERDATA, 0);
        }
        drop(engine);
        ENGINE_WINDOW.store(0, Ordering::Release);
        win32::destroy_window(engine_window);
        return Err(error);
    }

    // SAFETY: console_ctrl stays registered until it is removed after the
    // message loop ends, and it only touches the atomic and PostMessageW.
    unsafe {
        let _ = SetConsoleCtrlHandler(Some(console_ctrl), true);
    }

    // The engine pointer is already stored in GWLP_USERDATA; keep ownership
    // raw until the message loop ends and we reclaim it below.
    let _ = Box::into_raw(engine);

    log::line("snapit started");
    win32::message_loop();

    // SAFETY: the message loop has ended, so no window_proc can run anymore;
    // this block removes the ctrl handler, reclaims the raw engine box and
    // tears the window/classes down in the reverse of the setup order.
    unsafe {
        let _ = SetConsoleCtrlHandler(Some(console_ctrl), false);

        let stored = GetWindowLongPtrW(engine_window, GWLP_USERDATA) as *mut Engine;
        SetWindowLongPtrW(engine_window, GWLP_USERDATA, 0);
        if !stored.is_null() {
            drop(Box::from_raw(stored));
        }

        win32::destroy_window(engine_window);
        if let Ok(module) = win32::module_handle() {
            let _ = UnregisterClassW(engine_class, Some(module));
            let _ = UnregisterClassW(w!("SnapItRustOverlay"), Some(module));
        }
    }

    ENGINE_WINDOW.store(0, Ordering::Release);
    log::line("snapit stopped");
    Ok(())
}

fn enable_dpi_awareness() {
    // SAFETY: both calls only set this process's DPI awareness mode; the
    // context constant is the documented pseudo-handle.
    unsafe {
        if SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2).is_err() {
            let _ = SetProcessDPIAware();
        }
    }
}

/// # Safety
///
/// Only registered as the `WNDPROC` of the overlay window class: Windows owns
/// the parameter values for the duration of the call and they are not valid
/// outside it.
unsafe extern "system" fn overlay_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    // SAFETY: the parameters are the ones the OS passed to this wndproc.
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

/// # Safety
///
/// Only registered via `SetConsoleCtrlHandler`: Windows calls it on its own
/// handler thread with `_ctrl_type` owned by the OS, at any point in `run`,
/// which the atomic read + PostMessageW body tolerates.
unsafe extern "system" fn console_ctrl(_ctrl_type: u32) -> BOOL {
    let raw = ENGINE_WINDOW.load(Ordering::Acquire);
    if raw == 0 {
        return BOOL(0);
    }

    // SAFETY: `raw` is the engine window handle published by run(); posting
    // to it is safe whether or not the window still exists.
    let posted = unsafe {
        PostMessageW(
            Some(HWND(raw as *mut c_void)),
            WM_APP_SHUTDOWN,
            WPARAM(0),
            LPARAM(0),
        )
    };

    if posted.is_ok() { BOOL(1) } else { BOOL(0) }
}

fn dump_zones(config: &Config) {
    let monitors = win32::monitors();
    if monitors.is_empty() {
        println!("No monitors found.");
        return;
    }

    for monitor in monitors {
        let keys = monitor.keys();
        let primary = if monitor.primary { " (primary)" } else { "" };
        println!(
            "{}{}: bounds {}x{} at ({}, {}), work area {}x{} at ({}, {}), dpi {}",
            monitor.device,
            primary,
            monitor.bounds.width(),
            monitor.bounds.height(),
            monitor.bounds.left,
            monitor.bounds.top,
            monitor.work.width(),
            monitor.work.height(),
            monitor.work.left,
            monitor.work.top,
            monitor.dpi,
        );

        if !monitor.device_path.is_empty() {
            println!("  interface: {}", monitor.device_path);
        }

        if config.is_deactivated(&keys) {
            println!("  deactivated");
            continue;
        }

        let layout = config.layout_for(&keys);
        match layout {
            Some(layout) => println!("  layout: {} ({})", layout.name, layout.guid),
            None => println!("  layout: none (full screen)"),
        }

        let zones = MonitorZones::build_scaled(
            monitor.work.width(),
            monitor.work.height(),
            layout,
            monitor.dpi as f32 / 96.0,
        );
        for (index, area) in zones.areas.iter().enumerate() {
            println!(
                "  area {index}: {}x{} at ({}, {})",
                area.width(),
                area.height(),
                area.left,
                area.top,
            );
        }
        for (index, overlay) in zones.overlays.iter().enumerate() {
            println!(
                "  overlay {index}: full {}x{} at ({}, {}), mini {}x{} at ({}, {})",
                overlay.full.width(),
                overlay.full.height(),
                overlay.full.left,
                overlay.full.top,
                overlay.mini.width(),
                overlay.mini.height(),
                overlay.mini.left,
                overlay.mini.top,
            );
        }
    }
}
