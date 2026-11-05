//! OS autostart registration, reconciled to the StartAtBoot setting on every
//! settings save: an XDG autostart entry on Linux (pointing at the running
//! executable), the per-user registry Run key on Windows (via reg.exe — no
//! extra dependency).

pub fn reconcile(enable: bool) {
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let dir = xdg_autostart_dir();
        let entry = dir.join("warp-station.desktop");
        if enable {
            let exe = std::env::current_exe()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| "warp-station".into());
            let contents = format!(
                "[Desktop Entry]\nType=Application\nName=Warp Station\nExec={exe}\nTerminal=false\nX-GNOME-Autostart-enabled=true\n"
            );
            // a save that did not touch StartAtBoot must not rewrite the file;
            // reconcile is called after every commit and the write is a no-op
            // only when the entry already says exactly this
            let unchanged =
                std::fs::read_to_string(&entry).is_ok_and(|current| current == contents);
            if !unchanged {
                let _ = std::fs::create_dir_all(&dir);
                let _ = std::fs::write(&entry, contents);
            }
        } else if entry.exists() {
            let _ = std::fs::remove_file(&entry);
        }
    }
    #[cfg(windows)]
    {
        let exe = match std::env::current_exe() {
            Ok(path) => path.to_string_lossy().to_string(),
            Err(_) => return,
        };
        use std::os::windows::process::CommandExt;
        use std::process::Command;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let mut command = Command::new("reg");
        if enable {
            // the Run value carries embedded quotes: at logon an unquoted
            // path with spaces parses as program-plus-arguments
            let quoted = format!("\"{exe}\"");
            command.args([
                "add",
                "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run",
                "/v",
                "Warp Station",
                "/t",
                "REG_SZ",
                "/d",
                quoted.as_str(),
                "/f",
            ]);
        } else {
            command.args([
                "delete",
                "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run",
                "/v",
                "Warp Station",
                "/f",
            ]);
        }
        command.creation_flags(CREATE_NO_WINDOW);
        let result = command.output();
        if let Ok(output) = result {
            if !output.status.success() && enable {
                tracing::warn!(
                    "autostart registration failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        let _ = enable; // launchd agent left for a later pass
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn xdg_autostart_dir() -> std::path::PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            let home = std::env::var_os("HOME").unwrap_or_default();
            std::path::PathBuf::from(home).join(".config")
        })
        .join("autostart")
}
