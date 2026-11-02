//! The system-wide Ctrl+Shift+V for the stored clipboard password. Wayland
//! clients cannot grab keys, so Hyprland gets a runtime bind that execs this
//! exe in ping mode; Windows owns the combo via RegisterHotKey. The copy
//! itself runs on the trigger thread (wl-copy / raw SetClipboardData) because
//! gpui's Wayland clipboard write is focus-gated and a global hotkey fires
//! exactly when no Warp Station window has focus.

/// The stored password decoded from the settings file — the net thread reads
/// disk directly (it has no store entity), which matches what the store
/// persists on every save.
fn stored_password_from_settings() -> Option<String> {
    crate::settings::SettingsService::open()
        .get()
        .clipboard_password
        .encrypted_password
        .as_deref()
        .filter(|stored| !stored.is_empty())
        .and_then(crate::clipboard_password::decode)
}

#[cfg(target_os = "linux")]
mod linux {
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;
    use std::path::{Path, PathBuf};

    /// The argv that marks the short-lived bind payload process.
    pub const COPY_PASSWORD_ARG: &str = "--copy-clipboard-password";

    /// $XDG_RUNTIME_DIR/warp_station-clipboard.sock — the ping target.
    pub fn clipboard_socket_path() -> Option<PathBuf> {
        std::env::var_os("XDG_RUNTIME_DIR")
            .map(|dir| PathBuf::from(dir).join("warp-station-clipboard.sock"))
    }

    /// Adds the runtime bind `CTRL SHIFT V → <this exe> --copy-clipboard-password`.
    /// Runtime binds are not persisted by Hyprland, so every app launch
    /// re-adds the bind; it survives until the compositor restarts. Stock
    /// Hyprland takes a `keyword bind`; the Omarchy/dms Lua builds reject
    /// `keyword` ("use eval") and expose the config's own o.bind through the
    /// `eval` (Lua) request instead — verified live on Hyprland 0.56.2 dms.
    pub fn register_hyprland_bind() -> anyhow::Result<()> {
        let socket = hyprland_socket_path()
            .ok_or_else(|| anyhow::anyhow!("no Hyprland session socket (not on Hyprland?)"))?;
        let exe = std::env::current_exe()?.to_string_lossy().to_string();
        let command = format!("{exe} {COPY_PASSWORD_ARG}");

        // stock: execr splits the command on spaces and the bind grammar
        // splits on commas — neither may appear in the exe path
        if !exe.contains(' ') && !exe.contains(',') {
            let _ = ipc(&socket, "unbind CTRL SHIFT V");
            let reply = ipc(
                &socket,
                &format!("keyword bind CTRL SHIFT V, execr, {command}"),
            )?;
            if reply == "ok" {
                return Ok(());
            }
        }

        // Lua build: the command rides inside a quoted Lua literal, so
        // quotes and backslashes are the characters to refuse
        if !exe.contains('"') && !exe.contains('\\') {
            let _ = ipc(&socket, "eval hl.unbind(\"CTRL + SHIFT + V\")");
            let reply = ipc(
                &socket,
                &format!(
                    "eval o.bind(\"CTRL + SHIFT + V\", \"Warp Station clipboard password\", \"{command}\")"
                ),
            )?;
            if reply == "ok" {
                return Ok(());
            }
            anyhow::bail!("Hyprland rejected the bind: {reply}")
        }
        anyhow::bail!("exe path {exe} cannot be expressed as a Hyprland bind")
    }

    /// CLI side of the bind: tell the running instance to copy, then exit.
    pub fn ping_copy_password() -> anyhow::Result<()> {
        let path =
            clipboard_socket_path().ok_or_else(|| anyhow::anyhow!("XDG_RUNTIME_DIR is not set"))?;
        let mut stream = connect(&path)?;
        stream.write_all(b"copy-password\n")?;
        let mut reply = String::new();
        let _ = stream.read_to_string(&mut reply);
        Ok(())
    }

    /// wl-copy is its own Wayland client, so it takes clipboard ownership no
    /// matter which window has focus — unlike a gpui write from this process.
    pub async fn copy_password_to_clipboard() -> &'static str {
        use tokio::io::AsyncWriteExt;
        let Some(password) = super::stored_password_from_settings() else {
            return "No password stored";
        };
        let child = tokio::process::Command::new("wl-copy")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
        match child {
            Ok(mut child) => {
                if let Some(mut stdin) = child.stdin.take() {
                    let _ = stdin.write_all(password.as_bytes()).await;
                    // dropping stdin signals EOF; wl-copy then lingers as the
                    // clipboard owner until something else takes over — tokio
                    // reaps it when that happens
                }
                "Password copied to clipboard"
            }
            Err(_) => "Clipboard copy failed (wl-copy not installed)",
        }
    }

    fn hyprland_socket_path() -> Option<PathBuf> {
        let signature = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE")?;
        let runtime = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from);
        let socket = match runtime {
            Some(dir) => dir.join("hypr").join(signature).join(".socket.sock"),
            None => Path::new("/tmp/hypr").join(signature).join(".socket.sock"),
        };
        socket.exists().then_some(socket)
    }

    fn connect(socket: &Path) -> anyhow::Result<UnixStream> {
        let stream = UnixStream::connect(socket)?;
        // a wedged compositor must not hang the net thread on a silent socket
        stream.set_read_timeout(Some(std::time::Duration::from_secs(2)))?;
        stream.set_write_timeout(Some(std::time::Duration::from_secs(2)))?;
        Ok(stream)
    }

    fn ipc(socket: &Path, request: &str) -> anyhow::Result<String> {
        let mut stream = connect(socket)?;
        stream.write_all(request.as_bytes())?;
        let _ = stream.shutdown(std::net::Shutdown::Write);
        let mut reply = String::new();
        stream.read_to_string(&mut reply)?;
        Ok(reply.trim().to_string())
    }
}

#[cfg(target_os = "linux")]
pub use linux::*;

#[cfg(windows)]
mod windows {
    use std::thread;

    const MOD_CONTROL: u32 = 0x0002;
    const MOD_SHIFT: u32 = 0x0004;
    const MOD_NOREPEAT: u32 = 0x4000;
    const WM_HOTKEY: u32 = 0x0312;
    const HOTKEY_ID: i32 = 0x5151;
    const VK_V: u32 = 0x56;

    #[repr(C)]
    struct Point {
        x: i32,
        y: i32,
    }

    #[repr(C)]
    struct Msg {
        hwnd: isize,
        message: u32,
        wparam: usize,
        lparam: isize,
        time: u32,
        pt: Point,
        lprivate: usize,
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        fn RegisterHotKey(hwnd: isize, id: i32, modifiers: u32, vk: u32) -> i32;
        fn GetMessageW(msg: *mut Msg, hwnd: isize, filter_min: u32, filter_max: u32) -> i32;
        fn OpenClipboard(hwnd: isize) -> i32;
        fn CloseClipboard() -> i32;
        fn EmptyClipboard() -> i32;
        fn SetClipboardData(format: u32, handle: isize) -> isize;
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GlobalAlloc(flags: u32, bytes: usize) -> isize;
        fn GlobalLock(handle: isize) -> *mut core::ffi::c_void;
        fn GlobalUnlock(handle: isize) -> i32;
        fn GlobalFree(handle: isize) -> isize;
    }

    const GMEM_MOVEABLE: u32 = 0x0002;
    const CF_UNICODETEXT: u32 = 13;

    /// Any-thread clipboard write: the hotkey fires on a worker thread with
    /// no window, and SetClipboardData accepts a null-hwnd association.
    fn set_clipboard_text(text: &str) -> anyhow::Result<()> {
        let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY: documented FFI — OpenClipboard(0) is the null-hwnd
        // any-thread form; the closure below hands every allocation either to
        // SetClipboardData (system-owned) or GlobalFree before CloseClipboard
        unsafe {
            if OpenClipboard(0) == 0 {
                anyhow::bail!("OpenClipboard failed (held by another window?)");
            }
            // every path below must leave the clipboard closed again
            let outcome = (|| -> anyhow::Result<()> {
                if EmptyClipboard() == 0 {
                    anyhow::bail!("EmptyClipboard failed");
                }
                let handle = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2);
                if handle == 0 {
                    anyhow::bail!("GlobalAlloc failed");
                }
                let locked = GlobalLock(handle);
                if locked.is_null() {
                    let _ = GlobalFree(handle);
                    anyhow::bail!("GlobalLock failed");
                }
                std::ptr::copy_nonoverlapping(
                    wide.as_ptr().cast::<u8>(),
                    locked.cast(),
                    wide.len() * 2,
                );
                let _ = GlobalUnlock(handle);
                // on success the system owns `handle`
                if SetClipboardData(CF_UNICODETEXT, handle) == 0 {
                    let _ = GlobalFree(handle);
                    anyhow::bail!("SetClipboardData failed");
                }
                Ok(())
            })();
            let _ = CloseClipboard();
            outcome
        }
    }

    /// Copies the stored password on the hotkey thread and returns the toast
    /// text — gpui's write_to_clipboard is not used because it must run on
    /// the UI thread.
    pub fn copy_password_to_clipboard() -> &'static str {
        match super::stored_password_from_settings() {
            Some(password) => match set_clipboard_text(&password) {
                Ok(()) => "Password copied to clipboard",
                Err(err) => {
                    tracing::warn!("clipboard write failed: {err:#}");
                    "Clipboard copy failed"
                }
            },
            None => "No password stored",
        }
    }

    /// Owns Ctrl+Shift+V for the process lifetime. A null-hwnd registration
    /// posts WM_HOTKEY to the registering thread's own queue, so the message
    /// loop must run on the very thread that called RegisterHotKey.
    pub fn spawn_ctrl_shift_v(on_hotkey: impl Fn() + Send + 'static) -> anyhow::Result<()> {
        let (registered_tx, registered_rx) = std::sync::mpsc::channel::<bool>();
        thread::Builder::new()
            .name("ctrl-shift-v".into())
            .spawn(move || {
                // SAFETY: documented FFI — a null hwnd registers a
                // thread-specific hotkey, and the GetMessageW loop below
                // runs on this same thread as required
                let taken = unsafe {
                    RegisterHotKey(0, HOTKEY_ID, MOD_CONTROL | MOD_SHIFT | MOD_NOREPEAT, VK_V)
                } == 0;
                let _ = registered_tx.send(!taken);
                if taken {
                    return;
                }
                // SAFETY: Msg is a plain C struct of integers, so all-zero is
                // its documented zero state
                let mut msg: Msg = unsafe { std::mem::zeroed() };
                loop {
                    // SAFETY: msg is initialized and stays on this thread
                    let ret = unsafe { GetMessageW(&mut msg, 0, 0, 0) };
                    // 0 = WM_QUIT, -1 = hard error; anything else is a message
                    if ret <= 0 {
                        break;
                    }
                    if msg.message == WM_HOTKEY && msg.wparam == HOTKEY_ID as usize {
                        on_hotkey();
                    }
                }
            })?;
        if !registered_rx.recv().unwrap_or(false) {
            anyhow::bail!("Ctrl+Shift+V is registered by another application");
        }
        Ok(())
    }
}

#[cfg(windows)]
pub use windows::*;
