//! Start/stop the SnapIt companion that snaps dragged windows into zone
//! layouts; it watches this process (--parent) and exits when Warp Station dies.
//! The binary is the packaged Tools.SnapIt.exe next to the app unless a
//! configured executable (settings, like the warp-agent one) resolves.

#[cfg(windows)]
use std::path::PathBuf;
#[cfg(windows)]
use std::process::{Child, Command};
#[cfg(windows)]
use std::sync::Mutex;

#[cfg(windows)]
const COMPANION_EXE: &str = "Tools.SnapIt.exe";

#[cfg(windows)]
static CHILD: Mutex<Option<Child>> = Mutex::new(None);

/// The configured executable when set (absolute path or bare name), else the
/// packaged companion next to the app.
#[cfg(windows)]
fn companion_path(configured: &str) -> Option<PathBuf> {
    let configured = configured.trim();
    if !configured.is_empty() {
        return crate::exe::locate(configured);
    }
    let dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    let path = dir.join(COMPANION_EXE);
    path.is_file().then_some(path)
}

#[cfg(windows)]
pub fn is_running() -> bool {
    let Ok(mut slot) = CHILD.lock() else {
        return false;
    };
    let Some(child) = slot.as_mut() else {
        return false;
    };
    matches!(child.try_wait(), Ok(None))
}

#[cfg(windows)]
pub fn start(configured: &str) -> Result<(), String> {
    let mut slot = CHILD
        .lock()
        .map_err(|_| "SnapIt state poisoned".to_string())?;
    if let Some(child) = slot.as_mut() {
        if matches!(child.try_wait(), Ok(None)) {
            return Ok(());
        }
    }
    let configured = configured.trim();
    let path = companion_path(configured).ok_or_else(|| {
        if configured.is_empty() {
            format!("{COMPANION_EXE} not found next to the app")
        } else {
            format!("SnapIt executable not found: {configured}")
        }
    })?;

    let mut command = Command::new(&path);
    command.arg("--parent").arg(std::process::id().to_string());
    // no stray console window behind the companion
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);

    *slot = Some(
        command
            .spawn()
            .map_err(|err| format!("could not start SnapIt: {err}"))?,
    );
    Ok(())
}

#[cfg(windows)]
pub fn stop() -> Result<(), String> {
    let mut slot = CHILD
        .lock()
        .map_err(|_| "SnapIt state poisoned".to_string())?;
    if let Some(mut child) = slot.take() {
        child
            .kill()
            .map_err(|err| format!("could not stop SnapIt: {err}"))?;
        let _ = child.wait();
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn is_running() -> bool {
    false
}

#[cfg(not(windows))]
pub fn start(_configured: &str) -> Result<(), String> {
    Err("SnapIt is Windows-only".to_string())
}

#[cfg(not(windows))]
pub fn stop() -> Result<(), String> {
    Ok(())
}

/// One layout the engine reads from `~/.warp/snapit/layouts`, reduced to what a
/// preview draws: its design size and its split lines.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SnapLayout {
    pub name: String,
    pub width: f32,
    pub height: f32,
    pub lines: Vec<SnapLine>,
}

/// A split: zero wide or zero high, in the layout's own units.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SnapLine {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// `value` is "x,y" as the engine writes it.
fn pair(value: Option<&serde_json::Value>) -> Option<(f32, f32)> {
    let (first, second) = value?.as_str()?.split_once(',')?;
    Some((first.trim().parse().ok()?, second.trim().parse().ok()?))
}

fn field<'a>(value: &'a serde_json::Value, names: &[&str]) -> Option<&'a serde_json::Value> {
    names.iter().find_map(|name| value.get(*name))
}

/// The file's shape belongs to warp-snapit, and a line it can't read is skipped
/// rather than losing the layout. It reads the native snake case and the old
/// .NET PascalCase shape; the .NET app wrote these with a BOM.
fn parse_layout(json: &str) -> Option<SnapLayout> {
    let value: serde_json::Value =
        serde_json::from_str(json.trim_start_matches('\u{feff}')).ok()?;
    let (width, height) = pair(field(&value, &["size", "Size"]))?;
    let lines = field(&value, &["layout_lines", "LayoutLines"])
        .and_then(|lines| lines.as_array())
        .into_iter()
        .flatten()
        .filter_map(|line| {
            let (x, y) = pair(field(line, &["point", "Point"]))?;
            let (width, height) = pair(field(line, &["size", "Size"]))?;
            Some(SnapLine {
                x,
                y,
                width,
                height,
            })
        })
        .collect();
    Some(SnapLayout {
        name: field(&value, &["name", "Name"])?.as_str()?.to_string(),
        width,
        height,
        lines,
    })
}

/// Every layout in `folder`, by name.
pub fn layouts_in(folder: &std::path::Path) -> Vec<SnapLayout> {
    let mut layouts: Vec<SnapLayout> = std::fs::read_dir(folder)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
                .filter_map(|path| std::fs::read_to_string(path).ok())
                .filter_map(|json| parse_layout(&json))
                .collect()
        })
        .unwrap_or_default();
    layouts.sort_by(|a, b| a.name.cmp(&b.name));
    layouts
}

/// The layouts folder warp-snapit reads.
pub fn layouts_folder() -> std::path::PathBuf {
    crate::paths::AppPaths::user_data_root()
        .join("snapit")
        .join("layouts")
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAYOUT: &str = "\u{feff}{
        \"Name\": \"3 Part Horizontal\",
        \"Size\": \"2752,1104\",
        \"LayoutLines\": [
            {\"Point\": \"917.5,0\", \"Size\": \"0,1104\", \"SplitDirection\": \"Vertical\"},
            {\"Point\": \"oops\", \"Size\": \"0,1104\"}
        ]
    }";

    #[test]
    fn a_layout_reads_its_size_and_lines() {
        let layout = parse_layout(LAYOUT).expect("a layout");

        assert_eq!(layout.name, "3 Part Horizontal");
        assert_eq!((layout.width, layout.height), (2752.0, 1104.0));
        assert_eq!(layout.lines.len(), 1, "the unreadable line is skipped");
        assert_eq!(layout.lines[0].x, 917.5);
    }

    #[test]
    fn a_native_snake_layout_reads_too() {
        let layout = parse_layout(
            r#"{
                "schema_version": 1,
                "name": "3 Part Horizontal",
                "size": "2752,1104",
                "layout_lines": [
                    {"point": "917.5,0", "size": "0,1104", "split_direction": "vertical"}
                ]
            }"#,
        )
        .expect("a layout");
        assert_eq!(layout.name, "3 Part Horizontal");
        assert_eq!((layout.width, layout.height), (2752.0, 1104.0));
        assert_eq!(layout.lines.len(), 1);
        assert_eq!(layout.lines[0].x, 917.5);
    }

    #[test]
    fn a_file_without_a_size_is_not_a_layout() {
        assert_eq!(parse_layout(r#"{"Name": "x"}"#), None);
    }
}
