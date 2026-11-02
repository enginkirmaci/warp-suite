//! The NuGet Local watch service: watches a folder for new .nupkg build
//! outputs, clears the matching global-packages cache entry so consumer
//! projects fetch fresh DLLs, and copies every package into <watch>/nugets.
//! The polling loop runs as a spawned task on the caller's tokio runtime;
//! every state change streams a full [`NugetSnapshot`] back through `events`.

use crate::process::{self, ProcessRunOptions};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const COMMAND_TIMEOUT: Duration = Duration::from_secs(120);
const POLL_INTERVAL: Duration = Duration::from_millis(1500);
const COPY_ATTEMPTS: u32 = 3;
const COPY_RETRY_DELAY: Duration = Duration::from_millis(500);
const WALK_DEPTH: usize = 10;
const LOG_CAP: usize = 200;

/// Everything the drawer renders, rebuilt on every state change.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NugetSnapshot {
    pub watching: bool,
    pub count: u32,
    pub watch_folder: Option<String>,
    pub copy_folder: Option<String>,
    pub global_packages_folder: Option<String>,
    /// Newest-first activity log.
    pub activity: Vec<String>,
}

/// Watch parameters, captured from settings at press time — the service holds
/// no settings handle, the store stays the single owner.
#[derive(Clone, Debug)]
pub struct NugetConfig {
    pub enable: bool,
    pub watch_folder: Option<String>,
    pub filter: String,
    pub copy_delay: Duration,
    pub count_window: Duration,
}

pub struct NugetService {
    events: async_channel::Sender<NugetSnapshot>,
    state: Mutex<State>,
}

struct State {
    watching: bool,
    /// Stop arrived before the watch loop installed its channel.
    stop_requested: bool,
    count: u32,
    last_change: Option<std::time::Instant>,
    folder: Option<String>,
    copy_folder: Option<String>,
    global_packages: Option<PathBuf>,
    activity: Vec<String>,
    stop: Option<async_channel::Sender<()>>,
    config: Option<NugetConfig>,
}

impl NugetService {
    pub fn new(events: async_channel::Sender<NugetSnapshot>) -> Self {
        Self {
            events,
            state: Mutex::new(State {
                watching: false,
                stop_requested: false,
                count: 0,
                last_change: None,
                folder: None,
                copy_folder: None,
                global_packages: None,
                activity: Vec::new(),
                stop: None,
                config: None,
            }),
        }
    }

    /// Reserves the watch slot synchronously and spawns the loop, so a second
    /// start racing the first is a no-op.
    pub fn start(self: &Arc<Self>, config: NugetConfig) {
        {
            let mut state = self.state.lock().unwrap();
            if state.watching {
                return;
            }
            state.watching = true;
        }
        let this = self.clone();
        tokio::spawn(async move { this.run(config).await });
    }

    /// No-op when not watching; the "⏹ Stopped watching." line is logged here
    /// (once per stop) and the loop exits silently.
    pub fn stop(&self) {
        let mut state = self.state.lock().unwrap();
        if !state.watching {
            return;
        }
        state.watching = false;
        let sender = state.stop.take();
        state.stop_requested = sender.is_none();
        drop(state);
        if let Some(sender) = sender {
            let _ = sender.send_blocking(());
        }
        self.push_log("⏹ Stopped watching.");
    }

    /// Persists nothing — the store saved the folder; a running watch restarts
    /// against the new folder.
    pub fn set_folder(self: &Arc<Self>, folder: String) {
        let (watching, config) = {
            let mut state = self.state.lock().unwrap();
            state.folder = Some(folder.clone());
            state.copy_folder = compute_copy_folder(&folder);
            if let Some(config) = state.config.as_mut() {
                config.watch_folder = Some(folder.clone());
            }
            (state.watching, state.config.clone())
        };
        if watching {
            self.stop();
            if let Some(mut config) = config {
                config.watch_folder = Some(folder);
                self.start(config);
                return;
            }
        }
        self.emit();
    }

    pub fn register_source(self: &Arc<Self>) {
        let this = self.clone();
        tokio::spawn(async move { this.register_task().await });
    }

    /// Watch-slot release for the setup-failure paths (disabled in settings,
    /// invalid folder): silent, no error surface.
    fn release(&self) {
        {
            let mut state = self.state.lock().unwrap();
            state.watching = false;
            state.stop = None;
            state.stop_requested = false;
        }
        self.emit();
    }

    async fn run(self: Arc<Self>, config: NugetConfig) {
        if !config.enable {
            self.push_log("✗ NuGet is disabled in settings (EnableNuget).");
            self.release();
            return;
        }
        let Some(folder) = config
            .watch_folder
            .clone()
            .filter(|f| !f.trim().is_empty() && Path::new(f).is_dir())
        else {
            self.push_log("✗ Watch folder path is invalid or not set in settings.");
            self.release();
            return;
        };

        let global_packages = resolve_global_packages_folder().await;
        let copy_folder = compute_copy_folder(&folder);
        let (stop_tx, stop_rx) = async_channel::bounded::<()>(1);
        {
            let mut state = self.state.lock().unwrap();
            if state.stop_requested {
                // stop() ran while we were setting up
                state.stop_requested = false;
                state.watching = false;
                drop(state);
                self.emit();
                return;
            }
            state.folder = Some(folder.clone());
            state.copy_folder = copy_folder;
            state.global_packages = global_packages.map(PathBuf::from);
            state.stop = Some(stop_tx);
            state.config = Some(config.clone());
        }
        self.push_log(format!("▶ Watching '{folder}' for new packages..."));

        let watch_path = PathBuf::from(&folder);
        let mut known = scan_packages(&watch_path, &config.filter);
        let mut interval = tokio::time::interval(POLL_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut running = true;
        while running {
            tokio::select! {
                _ = interval.tick() => {
                    let current = scan_packages(&watch_path, &config.filter);
                    let new_packages: Vec<PathBuf> = current.difference(&known).cloned().collect();
                    known = current;
                    for package in new_packages {
                        if !self.process_package(&package, &config, &stop_rx).await {
                            running = false;
                            break;
                        }
                    }
                }
                _ = stop_rx.recv() => running = false,
            }
        }
        self.release();
    }

    /// One discovered .nupkg: clear its global cache entry, advance the
    /// activity counter, copy it into <watch>/nugets. Returns false when the
    /// stop signal interrupted the work.
    async fn process_package(
        &self,
        path: &Path,
        config: &NugetConfig,
        stop_rx: &async_channel::Receiver<()>,
    ) -> bool {
        enum Flow {
            Done,
            Cancelled,
        }
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let flow: Result<Flow, String> = async {
            // 1. clear the matching global-packages entry (parse/global failures
            //    only skip this step — the count and the copy still advance)
            if let Some((id, version)) = extract_package_info(&name) {
                let global = self.state.lock().unwrap().global_packages.clone();
                if let Some(global) = global {
                    let entry = global.join(id.to_lowercase()).join(version.to_lowercase());
                    if entry.is_dir() {
                        std::fs::remove_dir_all(&entry).map_err(|err| err.to_string())?;
                        self.push_log(format!("✓ Cleared cache for {id} {version}"));
                    } else {
                        self.push_log(format!("ℹ No cache found for {id} {version}"));
                    }
                } else {
                    self.push_log("⚠ Could not locate NuGet global packages folder");
                }
            } else {
                self.push_log(format!("⚠ Could not parse package info from {name}"));
            }

            // 2. sliding activity window
            {
                let mut state = self.state.lock().unwrap();
                let now = std::time::Instant::now();
                state.count = match state.last_change {
                    Some(previous) if now.duration_since(previous) < config.count_window => {
                        state.count + 1
                    }
                    _ => 1,
                };
                state.last_change = Some(now);
            }

            // 3. copy into the local cache folder
            let Some(target_dir) = self.state.lock().unwrap().copy_folder.clone() else {
                self.push_log("⚠ Copy skipped: target folder not available");
                return Ok(Flow::Done);
            };
            std::fs::create_dir_all(&target_dir).map_err(|err| err.to_string())?;
            let dest = Path::new(&target_dir).join(&name);
            for attempt in 0..COPY_ATTEMPTS {
                if !sleep_interruptible(stop_rx, config.copy_delay).await {
                    return Ok(Flow::Cancelled);
                }
                match std::fs::copy(path, &dest) {
                    Ok(_) => {
                        self.push_log(format!("✓ Copied to {}", dest.display()));
                        return Ok(Flow::Done);
                    }
                    Err(err) if attempt + 1 >= COPY_ATTEMPTS => return Err(err.to_string()),
                    Err(_) => {
                        if !sleep_interruptible(stop_rx, COPY_RETRY_DELAY).await {
                            return Ok(Flow::Cancelled);
                        }
                    }
                }
            }
            unreachable!("every attempt returns")
        }
        .await;
        match flow {
            Ok(Flow::Done) => {
                self.emit();
                true
            }
            Ok(Flow::Cancelled) => {
                self.release();
                false
            }
            Err(err) => {
                self.push_log(format!("ERROR processing {}: {err}", path.display()));
                true
            }
        }
    }

    async fn register_task(self: Arc<Self>) {
        let Some(folder) = self.state.lock().unwrap().copy_folder.clone() else {
            self.push_log("✗ Computed NuGet local cache folder is not available. Ensure a valid Watch Directory is set.");
            return;
        };
        if let Err(err) = std::fs::create_dir_all(&folder) {
            self.push_log(format!("✗ Failed to create cache folder: {err}"));
            self.emit();
            return;
        }
        self.push_log(format!("✓ Ensured cache folder exists: {folder}"));

        let folder_name = Path::new(&folder)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let source_name = format!("{folder_name}-local");
        let args = [
            "nuget".to_string(),
            "add".to_string(),
            "source".to_string(),
            folder.clone(),
            "--name".to_string(),
            source_name.clone(),
        ];
        let result = process::run(ProcessRunOptions {
            file_name: "dotnet",
            args: &args,
            timeout: Some(COMMAND_TIMEOUT),
            close_stdin: true,
            ..Default::default()
        })
        .await;
        match result {
            Ok(result) if result.timed_out => self.push_log(format!(
                "✗ Timed out after {}s registering NuGet source '{source_name}': {folder}",
                COMMAND_TIMEOUT.as_secs()
            )),
            Ok(result) if result.succeeded() => self.push_log(format!(
                "✓ Registered NuGet source '{source_name}': {folder}"
            )),
            Ok(result) if result.stdout.to_lowercase().contains("already been added") => self
                .push_log(format!(
                    "ℹ NuGet source '{source_name}' already registered: {folder}"
                )),
            Ok(result) => self.push_log(format!(
                "✗ Failed to register source: {}",
                result.stderr.trim()
            )),
            Err(err) => self.push_log(format!("✗ Execution error: {err}")),
        }
        self.emit();
    }

    fn push_log(&self, line: impl Into<String>) {
        {
            let mut state = self.state.lock().unwrap();
            state.activity.insert(0, line.into());
            state.activity.truncate(LOG_CAP);
        }
        self.emit();
    }

    fn emit(&self) {
        let snapshot = {
            let state = self.state.lock().unwrap();
            NugetSnapshot {
                watching: state.watching,
                count: state.count,
                watch_folder: state.folder.clone(),
                copy_folder: state.copy_folder.clone(),
                global_packages_folder: state
                    .global_packages
                    .as_ref()
                    .map(|path| path.to_string_lossy().to_string()),
                activity: state.activity.clone(),
            }
        };
        let _ = self.events.send_blocking(snapshot);
    }
}

async fn sleep_interruptible(stop_rx: &async_channel::Receiver<()>, delay: Duration) -> bool {
    tokio::select! {
        _ = tokio::time::sleep(delay) => true,
        _ = stop_rx.recv() => false,
    }
}

/// `dotnet nuget locals global-packages --list` → the printed folder. The exit
/// code is not consulted; the folder line on stdout is all that matters.
async fn resolve_global_packages_folder() -> Option<String> {
    let args = [
        "nuget".to_string(),
        "locals".to_string(),
        "global-packages".to_string(),
        "--list".to_string(),
    ];
    let result = process::run(ProcessRunOptions {
        file_name: "dotnet",
        args: &args,
        timeout: Some(COMMAND_TIMEOUT),
        close_stdin: true,
        ..Default::default()
    })
    .await
    .ok()?;
    let line = result
        .stdout
        .lines()
        .find(|line| line.trim_start().starts_with("global-packages:"))?;
    Some(line.split_once(':')?.1.trim().trim_matches('"').to_string())
}

fn compute_copy_folder(watch_folder: &str) -> Option<String> {
    if watch_folder.trim().is_empty() {
        return None;
    }
    // join collapses a trailing separator, so "/src/pkg/" → "/src/pkg/nugets"
    Some(
        Path::new(watch_folder)
            .join("nugets")
            .to_string_lossy()
            .to_string(),
    )
}

fn scan_packages(folder: &Path, filter: &str) -> BTreeSet<PathBuf> {
    let mut found = BTreeSet::new();
    walk(folder, filter, 0, &mut found);
    found
}

fn walk(dir: &Path, filter: &str, depth: usize, found: &mut BTreeSet<PathBuf>) {
    if depth > WALK_DEPTH {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        if file_type.is_dir() {
            walk(&path, filter, depth + 1, found);
        } else if file_type.is_file()
            && matches_filter(&entry.file_name().to_string_lossy(), filter)
        {
            found.insert(path);
        }
    }
}

/// "*.nupkg" style filter: `*`/empty matches everything, `*.ext` is a
/// case-insensitive extension check, anything else is a literal name.
fn matches_filter(name: &str, filter: &str) -> bool {
    let filter = filter.trim();
    if filter.is_empty() || filter == "*" {
        return true;
    }
    let lower_name = name.to_ascii_lowercase();
    match filter.strip_prefix("*.") {
        Some(ext) => lower_name.ends_with(&ext.to_ascii_lowercase()),
        None => lower_name == filter.to_ascii_lowercase(),
    }
}

/// (id, version) from "Id.Version.nupkg" — the leftmost dot whose remainder
/// is version-shaped.
pub fn extract_package_info(file_name: &str) -> Option<(String, String)> {
    let lower = file_name.to_ascii_lowercase();
    if !lower.ends_with(".nupkg") {
        return None;
    }
    let stem = &file_name[..file_name.len() - 6];
    for (index, _) in stem.match_indices('.') {
        let (id, version) = (&stem[..index], &stem[index + 1..]);
        if is_version(version) {
            return Some((id.to_string(), version.to_string()));
        }
    }
    None
}

fn is_version(candidate: &str) -> bool {
    let mut chars = candidate.chars();
    chars.next().is_some_and(|c| c.is_ascii_digit())
        && candidate
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_id_and_version() {
        assert_eq!(
            extract_package_info("MyLib.1.2.3.nupkg"),
            Some(("MyLib".into(), "1.2.3".into()))
        );
        assert_eq!(
            extract_package_info("Microsoft.Extensions.Logging.8.0.1.nupkg"),
            Some(("Microsoft.Extensions.Logging".into(), "8.0.1".into()))
        );
        assert_eq!(
            extract_package_info("Pkg.2.0.0-beta.1+build.nupkg"),
            Some(("Pkg".into(), "2.0.0-beta.1+build".into()))
        );
        assert_eq!(extract_package_info("readme.txt"), None);
        assert_eq!(extract_package_info("NoVersion.nupkg"), None);
    }

    #[test]
    fn filters_package_files() {
        assert!(matches_filter("a.NUPKG", "*.nupkg"));
        assert!(!matches_filter("a.zip", "*.nupkg"));
        assert!(matches_filter("anything.bin", "*"));
        assert!(matches_filter("exact.dll", "exact.dll"));
    }

    #[test]
    fn copy_folder_sits_next_to_the_watch_folder() {
        assert_eq!(
            compute_copy_folder("/src/packages").as_deref(),
            Some(Path::new("/src/packages").join("nugets").to_str().unwrap())
        );
        assert_eq!(compute_copy_folder("  "), None);
    }

    /// The full watch flow against a real temp folder: start, discover a
    /// dropped .nupkg, copy it into <watch>/nugets, count it, stop.
    #[tokio::test]
    async fn watch_discovers_copies_and_stops() {
        let temp = tempfile::tempdir().unwrap();
        let watch = temp.path().join("builds");
        std::fs::create_dir_all(&watch).unwrap();

        let (events, receiver) = async_channel::unbounded();
        let service = Arc::new(NugetService::new(events));
        service.start(NugetConfig {
            enable: true,
            watch_folder: Some(watch.to_string_lossy().to_string()),
            filter: "*.nupkg".into(),
            copy_delay: Duration::from_millis(10),
            count_window: Duration::from_secs(60),
        });

        // the "▶ Watching" line is logged after the baseline scan, so a
        // package dropped from here on is guaranteed to be discovered
        let mut started = false;
        for _ in 0..100 {
            while let Ok(snapshot) = receiver.try_recv() {
                started |= snapshot
                    .activity
                    .iter()
                    .any(|line| line.starts_with("▶ Watching"));
            }
            if started {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(started, "watch never started");

        std::fs::write(watch.join("MyLib.1.0.0.nupkg"), b"payload").unwrap();
        let mut latest = Default::default();
        for _ in 0..100 {
            while let Ok(snapshot) = receiver.try_recv() {
                latest = snapshot;
            }
            if latest.count >= 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(latest.count >= 1, "package never counted");
        assert!(
            latest
                .activity
                .iter()
                .any(|line| line.starts_with("✓ Copied to")),
            "copy never logged: {:?}",
            latest.activity
        );
        assert!(watch.join("nugets").join("MyLib.1.0.0.nupkg").exists());

        service.stop();
        let mut stopped = false;
        for _ in 0..100 {
            while let Ok(snapshot) = receiver.try_recv() {
                stopped |= !snapshot.watching
                    && snapshot
                        .activity
                        .iter()
                        .any(|line| line.contains("⏹ Stopped"));
                latest = snapshot;
            }
            if stopped {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(stopped, "stop never settled: {:?}", latest.activity);
    }
}
