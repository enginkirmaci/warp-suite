//! Thin, typed wrappers over the Win32 calls the engine needs.
//!
//! Everything here runs on the engine thread unless noted; handles are plain
//! pointers and deliberately never cross threads.

use std::ffi::c_void;
use std::mem::size_of;

use anyhow::{Result, anyhow};
use windows::Win32::Foundation::{COLORREF, GetLastError, HINSTANCE, HWND, LPARAM, POINT, RECT};
use windows::Win32::Graphics::Dwm::{
    DWMWA_EXTENDED_FRAME_BOUNDS, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND,
    DwmGetWindowAttribute, DwmSetWindowAttribute,
};
use windows::Win32::Graphics::Gdi::{
    AC_SRC_ALPHA, AC_SRC_OVER, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION,
    CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DISPLAY_DEVICEW, DeleteDC, DeleteObject,
    EnumDisplayDevicesW, EnumDisplayMonitors, GetMonitorInfoW, HDC, HGDIOBJ, HMONITOR,
    MONITORINFOEXW, SelectObject,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, GetDpiForSystem, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VK_LBUTTON, VK_MBUTTON, VK_RBUTTON, VK_XBUTTON1, VK_XBUTTON2,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, EDD_GET_DEVICE_INTERFACE_NAME, GWL_STYLE, GetCursorPos,
    GetDesktopWindow, GetForegroundWindow, GetSystemMetrics, GetWindowLongW, GetWindowRect,
    GetWindowTextW, HWND_TOP, HWND_TOPMOST, RegisterClassW, SM_CYCAPTION, SM_CYFIXEDFRAME, SW_HIDE,
    SW_SHOWNOACTIVATE, SW_SHOWNORMAL, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW,
    SetWindowPos, ShowWindow, ULW_ALPHA, UpdateLayeredWindow, WINDOW_EX_STYLE, WINDOW_STYLE,
    WNDCLASS_STYLES, WNDCLASSW, WNDPROC, WS_EX_APPWINDOW, WS_EX_LAYERED, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};
use windows::core::BOOL;
use windows::core::{PCWSTR, w};

use crate::geometry::Rect;
use crate::settings::MouseButton;

// ---------------------------------------------------------------------------
// Strings
// ---------------------------------------------------------------------------

pub fn wide_to_string(buffer: &[u16]) -> String {
    let length = buffer
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..length])
}

// ---------------------------------------------------------------------------
// Monitors
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct Monitor {
    /// `\\.\DISPLAY1`
    pub device: String,
    /// Display-interface path from `EnumDisplayDevices`.
    pub device_path: String,
    pub bounds: Rect,
    pub work: Rect,
    pub primary: bool,
    pub dpi: u32,
}

impl Monitor {
    /// Identifiers checked against `screens_layouts` / `deactivated_screens`.
    ///
    /// The .NET app stores `Display.DevicePath` when available and only falls
    /// back to `DisplayName`, so the interface path is checked first here too.
    pub fn keys(&self) -> Vec<String> {
        let mut keys = Vec::with_capacity(2);
        if !self.device_path.is_empty() {
            keys.push(self.device_path.clone());
        }
        keys.push(self.device.clone());
        keys
    }
}

/// # Safety
///
/// Only called by `EnumDisplayMonitors` with the `LPARAM` it was given: `data`
/// must point to a valid `Vec<Monitor>` for the duration of that call, and the
/// OS owns the other parameters for the duration of the callback.
unsafe extern "system" fn monitor_callback(
    monitor: HMONITOR,
    _hdc: HDC,
    _rect: *mut RECT,
    data: LPARAM,
) -> BOOL {
    // SAFETY: `data` is the `&mut Vec<Monitor>` that `monitors()` passed as
    // the LPARAM; the callback only runs during that same-thread call, so the
    // pointer is valid and unaliased here.
    let monitors = unsafe { &mut *(data.0 as *mut Vec<Monitor>) };

    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
    // SAFETY: `monitor` is the HMONITOR handed to this callback and the
    // out-struct is a correctly sized MONITORINFOEXW (cbSize set above).
    if unsafe { GetMonitorInfoW(monitor, &mut info.monitorInfo as *mut _) }.as_bool() {
        let device = wide_to_string(&info.szDevice);

        let mut display = DISPLAY_DEVICEW {
            cb: size_of::<DISPLAY_DEVICEW>() as u32,
            ..Default::default()
        };
        // SAFETY: `szDevice` is a NUL-terminated buffer filled by
        // GetMonitorInfoW and `display` is an output struct with `cb` set.
        let device_path = if unsafe {
            EnumDisplayDevicesW(
                PCWSTR(info.szDevice.as_ptr()),
                0,
                &mut display,
                EDD_GET_DEVICE_INTERFACE_NAME,
            )
        }
        .as_bool()
        {
            wide_to_string(&display.DeviceID)
        } else {
            String::new()
        };

        let mut dpi_x = 96u32;
        let mut dpi_y = 96u32;
        // SAFETY: `monitor` is the HMONITOR from this callback; the two out
        // params are valid u32 locals.
        if unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) }.is_err()
        {
            // SAFETY: no parameters; the call only reads system state.
            dpi_x = unsafe { GetDpiForSystem() }.max(96);
        }

        let monitor_rect = info.monitorInfo.rcMonitor;
        let work_rect = info.monitorInfo.rcWork;

        monitors.push(Monitor {
            device,
            device_path,
            bounds: Rect::new(
                monitor_rect.left,
                monitor_rect.top,
                monitor_rect.right,
                monitor_rect.bottom,
            ),
            work: Rect::new(
                work_rect.left,
                work_rect.top,
                work_rect.right,
                work_rect.bottom,
            ),
            primary: info.monitorInfo.dwFlags & 1 != 0,
            dpi: dpi_x,
        });
    }

    windows::Win32::Foundation::TRUE
}

pub fn monitors() -> Vec<Monitor> {
    let mut monitors = Vec::new();
    // SAFETY: `monitor_callback` only touches the context pointer during this
    // call and `monitors` outlives it.
    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(monitor_callback),
            LPARAM(&mut monitors as *mut _ as isize),
        );
    }
    monitors
}

pub fn dpi_at_point(x: i32, y: i32) -> u32 {
    // SAFETY: MonitorFromPoint only reads, and the DPI queries take valid out
    // params or none at all.
    unsafe {
        // The .NET DpiHelper adds one pixel to avoid landing exactly on a
        // monitor boundary; keep the same behaviour.
        let monitor = windows::Win32::Graphics::Gdi::MonitorFromPoint(
            POINT { x: x + 1, y: y + 1 },
            windows::Win32::Graphics::Gdi::MONITOR_DEFAULTTONEAREST,
        );
        let mut dpi_x = 96u32;
        let mut dpi_y = 96u32;
        if GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y).is_err() {
            dpi_x = GetDpiForSystem().max(96);
        }
        dpi_x
    }
}

pub fn system_caption_metrics() -> (i32, i32) {
    // SAFETY: GetSystemMetrics takes no pointers.
    unsafe {
        (
            GetSystemMetrics(SM_CYCAPTION),
            GetSystemMetrics(SM_CYFIXEDFRAME),
        )
    }
}

pub fn cursor_position() -> Option<(i32, i32)> {
    let mut point = POINT::default();
    // SAFETY: `point` is a valid POINT output buffer.
    unsafe { GetCursorPos(&mut point) }.ok()?;
    Some((point.x, point.y))
}

/// Whether the engine can read this button's physical state.
///
/// `GetAsyncKeyState` documents the three main buttons; the extra buttons
/// rely on the driver and read as up on some systems, which would end a drag
/// that is still going.
pub fn button_state_is_readable(button: MouseButton) -> bool {
    matches!(
        button,
        MouseButton::Left | MouseButton::Right | MouseButton::Middle
    )
}

pub fn mouse_button_down(button: MouseButton) -> bool {
    let key = match button {
        MouseButton::Left => VK_LBUTTON,
        MouseButton::Right => VK_RBUTTON,
        MouseButton::Middle => VK_MBUTTON,
        MouseButton::XButton1 => VK_XBUTTON1,
        MouseButton::XButton2 => VK_XBUTTON2,
    };

    // SAFETY: GetAsyncKeyState only reads the system's key state.
    unsafe { GetAsyncKeyState(key.0 as i32) < 0 }
}

// ---------------------------------------------------------------------------
// Windows
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct WindowInfo {
    pub hwnd: HWND,
    pub title: String,
    pub rect: Rect,
}

pub fn foreground_window() -> Option<WindowInfo> {
    // SAFETY: no invariants; the returned handle is null-checked below.
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.0.is_null() {
        return None;
    }

    let rect = window_rect(hwnd)?;
    if rect.is_empty() {
        return None;
    }
    Some(WindowInfo {
        hwnd,
        title: window_text(hwnd),
        rect,
    })
}

pub fn window_text(hwnd: HWND) -> String {
    let mut buffer = [0u16; 512];
    // SAFETY: `buffer` is valid for GetWindowTextW to write and NUL-terminate,
    // and the returned count never exceeds its length.
    let length = unsafe { GetWindowTextW(hwnd, &mut buffer) };
    if length <= 0 {
        String::new()
    } else {
        String::from_utf16_lossy(&buffer[..length as usize])
    }
}

pub fn window_rect(hwnd: HWND) -> Option<Rect> {
    let mut rect = RECT::default();
    // SAFETY: `rect` is a valid output rectangle for this hwnd.
    unsafe { GetWindowRect(hwnd, &mut rect) }.ok()?;
    Some(Rect::new(rect.left, rect.top, rect.right, rect.bottom))
}

pub fn is_window_visible(hwnd: HWND) -> bool {
    // SAFETY: IsWindowVisible only reads the window's state.
    unsafe { windows::Win32::UI::WindowsAndMessaging::IsWindowVisible(hwnd).as_bool() }
}

pub fn desktop_rect() -> Option<Rect> {
    // SAFETY: GetDesktopWindow takes no parameters.
    window_rect(unsafe { GetDesktopWindow() })
}

/// Matches the .NET `IsAllowedWindowStyle` quirk: it reads `GWL_STYLE` and
/// masks with `WS_EX_APPWINDOW` (which is `WS_SIZEBOX` in the style bits).
/// This effectively disables snapping for fixed-size/modal windows.
pub fn is_allowed_window_style(hwnd: HWND) -> bool {
    // SAFETY: GetWindowLongW only reads the window's style dword.
    let style = unsafe { GetWindowLongW(hwnd, GWL_STYLE) };
    (style & WS_EX_APPWINDOW.0 as i32) != 0
}

pub fn is_fullscreen(info: &WindowInfo) -> bool {
    desktop_rect().is_some_and(|desktop| desktop == info.rect)
}

pub fn move_window(hwnd: HWND, rect: Rect) {
    if rect.width() <= 0 || rect.height() <= 0 {
        return;
    }
    // SAFETY: `hwnd` is a live top-level window the engine moves on the
    // caller's behalf; both calls only change position, size, visibility.
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOWNORMAL);
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOP),
            rect.left,
            rect.top,
            rect.width(),
            rect.height(),
            SWP_SHOWWINDOW | windows::Win32::UI::WindowsAndMessaging::SWP_ASYNCWINDOWPOS,
        );
    }
}

pub fn extended_frame_bounds(hwnd: HWND) -> Option<Rect> {
    let mut rect = RECT::default();
    // SAFETY: DWMWA_EXTENDED_FRAME_BOUNDS is a RECT and the buffer is exactly
    // that size.
    unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut rect as *mut _ as *mut c_void,
            size_of::<RECT>() as u32,
        )
    }
    .ok()?;
    Some(Rect::new(rect.left, rect.top, rect.right, rect.bottom))
}

/// Corrects a target rectangle for the invisible DWM frame margins so the
/// visible window frame lands on the zone (the .NET `WindowPlacement` helper).
pub fn expand_by_frame_margins(target: &mut Rect, outer: Rect, with_margin: Rect) {
    let margin_horizontal = (outer.width() - with_margin.width()) / 2;
    target.left -= margin_horizontal;
    target.right += margin_horizontal;
    target.bottom += outer.height() - with_margin.height();
}

pub fn set_no_round_corners(hwnd: HWND) {
    let preference = DWMWCP_DONOTROUND;
    // SAFETY: DWMWA_WINDOW_CORNER_PREFERENCE is a DWORD-sized value and
    // `preference` matches that size.
    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &preference as *const _ as *const c_void,
            size_of_val(&preference) as u32,
        );
    }
}

// ---------------------------------------------------------------------------
// Window classes / layered windows
// ---------------------------------------------------------------------------

pub fn module_handle() -> Result<HINSTANCE> {
    // SAFETY: None asks for this executable's module, which is always loaded.
    let module = unsafe { GetModuleHandleW(None)? };
    Ok(HINSTANCE(module.0))
}

pub fn register_class(name: PCWSTR, procedure: WNDPROC) -> Result<()> {
    let class = WNDCLASSW {
        style: WNDCLASS_STYLES(
            windows::Win32::UI::WindowsAndMessaging::CS_HREDRAW.0
                | windows::Win32::UI::WindowsAndMessaging::CS_VREDRAW.0,
        ),
        lpfnWndProc: procedure,
        hInstance: module_handle()?,
        lpszClassName: name,
        ..Default::default()
    };

    // SAFETY: `class` is fully initialized, `lpszClassName` points to a
    // NUL-terminated static string and the wndproc outlives the class.
    if unsafe { RegisterClassW(&class) } == 0 {
        return Err(anyhow!("RegisterClassW failed: {}", unsafe {
            // SAFETY: read immediately after the failed RegisterClassW, so no
            // other call has overwritten the thread's last error.
            GetLastError().0
        }));
    }
    Ok(())
}

pub fn create_message_window(class_name: PCWSTR) -> Result<HWND> {
    // SAFETY: the class is registered by the caller, every string is a
    // NUL-terminated static, and there is no parent or menu.
    unsafe {
        Ok(CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class_name,
            w!("SnapIt"),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            None,
            None,
            Some(module_handle()?),
            None,
        )?)
    }
}

pub fn create_overlay_window(class_name: PCWSTR, rect: Rect) -> Result<HWND> {
    let ex_style = WINDOW_EX_STYLE(
        WS_EX_LAYERED.0
            | WS_EX_TRANSPARENT.0
            | WS_EX_TOPMOST.0
            | WS_EX_TOOLWINDOW.0
            | WS_EX_NOACTIVATE.0,
    );

    // SAFETY: the class is registered by the caller, every string is a
    // NUL-terminated static, and there is no parent or menu.
    unsafe {
        Ok(CreateWindowExW(
            ex_style,
            class_name,
            w!("SnapIt Overlay"),
            WS_POPUP,
            rect.left,
            rect.top,
            rect.width(),
            rect.height(),
            None,
            None,
            Some(module_handle()?),
            None,
        )?)
    }
}

pub fn show_overlay(hwnd: HWND) {
    // SAFETY: `hwnd` is the overlay window created on this thread.
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    }
}

pub fn hide_overlay(hwnd: HWND) {
    // SAFETY: `hwnd` is the overlay window created on this thread.
    unsafe {
        let _ = ShowWindow(hwnd, SW_HIDE);
    }
}

pub fn destroy_window(hwnd: HWND) {
    // SAFETY: `hwnd` is a window created on the calling thread.
    unsafe {
        let _ = DestroyWindow(hwnd);
    }
}

// ---------------------------------------------------------------------------
// Layered-window surface (32-bpp premultiplied DIB)
// ---------------------------------------------------------------------------

pub struct LayerSurface {
    pub hwnd: HWND,
    pub origin: (i32, i32),
    pub width: i32,
    pub height: i32,
    memory_dc: HDC,
    bitmap: windows::Win32::Graphics::Gdi::HBITMAP,
    old_bitmap: HGDIOBJ,
    bits: *mut u8,
}

impl LayerSurface {
    pub fn new(hwnd: HWND, origin: (i32, i32), width: i32, height: i32) -> Result<Self> {
        let header = BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            // Negative height = top-down DIB, matching pixmap row order.
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        };
        let info = BITMAPINFO {
            bmiHeader: header,
            ..Default::default()
        };

        let mut bits: *mut c_void = std::ptr::null_mut();
        // SAFETY: `info` describes a valid 32-bpp top-down DIB and `bits` is
        // the out-pointer for its pixel memory.
        let bitmap = unsafe { CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0)? };
        if bits.is_null() {
            // SAFETY: `bitmap` is the handle CreateDIBSection just returned.
            unsafe {
                let _ = DeleteObject(HGDIOBJ(bitmap.0));
            }
            return Err(anyhow!("CreateDIBSection returned no pixels"));
        }

        // SAFETY: None creates a DC compatible with the current screen.
        let memory_dc = unsafe { CreateCompatibleDC(None) };
        if memory_dc.0.is_null() {
            // SAFETY: `bitmap` is the handle CreateDIBSection returned and has
            // not been selected into any DC.
            unsafe {
                let _ = DeleteObject(HGDIOBJ(bitmap.0));
            }
            return Err(anyhow!("CreateCompatibleDC failed"));
        }

        // SAFETY: both handles are valid and the previous selection is kept in
        // `old_bitmap` for Drop to restore.
        let old_bitmap = unsafe { SelectObject(memory_dc, HGDIOBJ(bitmap.0)) };

        Ok(Self {
            hwnd,
            origin,
            width,
            height,
            memory_dc,
            bitmap,
            old_bitmap,
            bits: bits as *mut u8,
        })
    }

    /// Copies premultiplied RGBA pixels into the BGRA DIB and pushes them to
    /// the layered window.
    pub fn present(&mut self, pixmap: &tiny_skia::Pixmap) -> Result<()> {
        if pixmap.width() != self.width as u32 || pixmap.height() != self.height as u32 {
            return Err(anyhow!("pixmap size does not match the layer surface"));
        }

        let source = pixmap.data();
        // SAFETY: `bits` is the CreateDIBSection pixel buffer, exactly
        // width*height*4 bytes, and the size check above guarantees the pixmap
        // matches, so `source.len()` bytes are in bounds.
        let destination = unsafe { std::slice::from_raw_parts_mut(self.bits, source.len()) };
        for (src, dst) in source
            .as_chunks::<4>()
            .0
            .iter()
            .zip(destination.as_chunks_mut::<4>().0)
        {
            dst[0] = src[2];
            dst[1] = src[1];
            dst[2] = src[0];
            dst[3] = src[3];
        }

        let destination_point = POINT {
            x: self.origin.0,
            y: self.origin.1,
        };
        let size = windows::Win32::Foundation::SIZE {
            cx: self.width,
            cy: self.height,
        };
        let source_point = POINT { x: 0, y: 0 };
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };

        // SAFETY: every POINT/SIZE/BLENDFUNCTION pointer outlives the call and
        // the memory DC has the DIB selected.
        unsafe {
            UpdateLayeredWindow(
                self.hwnd,
                None,
                Some(&destination_point as *const POINT),
                Some(&size as *const windows::Win32::Foundation::SIZE),
                Some(self.memory_dc),
                Some(&source_point as *const POINT),
                COLORREF(0),
                Some(&blend as *const BLENDFUNCTION),
                ULW_ALPHA,
            )?;
        }
        Ok(())
    }
}

impl Drop for LayerSurface {
    fn drop(&mut self) {
        // SAFETY: the DC, bitmap and old selection were created in `new` and
        // are released exactly once here, in restore-then-delete order.
        unsafe {
            SelectObject(self.memory_dc, self.old_bitmap);
            let _ = DeleteObject(HGDIOBJ(self.bitmap.0));
            let _ = DeleteDC(self.memory_dc);
        }
    }
}

// ---------------------------------------------------------------------------
// Message loop
// ---------------------------------------------------------------------------

/// The plain `GetMessageW` loop shared by the app thread and the hook threads:
/// `GetMessageW` returns -1 only on error with an invalid hwnd/filter (neither
/// happens here), so `> 0` is the run condition, and `0` (WM_QUIT) ends it.
pub(crate) fn message_loop() {
    use windows::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, GetMessageW, MSG, TranslateMessage,
    };

    let mut message = MSG::default();
    // SAFETY: `message` is a valid output MSG and the NULL hwnd filter means
    // "any window on this thread".
    while unsafe { GetMessageW(&mut message, None, 0, 0) }.0 > 0 {
        // SAFETY: `message` was just filled in by the GetMessageW above.
        unsafe {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}
