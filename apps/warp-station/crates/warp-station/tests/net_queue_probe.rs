//! The net loop runs its passes one at a time, so the reads the rail waits on
//! must not queue behind them. These drive the real net thread with a stand-in
//! `gh` whose timing they control.

#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use warp_station::net::{NetCommand, UiEvent, spawn_net_thread};

/// Keep the net thread's single-instance door out of the developer's real
/// `~/.warp/station`: every test in this binary shares the first scratch dir.
fn scratch_ipc_dir() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let dir =
            std::env::temp_dir().join(format!("warp-station-probe-ipc-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        // SAFETY: this test binary's tests all call this before spawning the
        // net thread; no other thread reads the variable before it is set.
        unsafe { std::env::set_var("WARP_STATION_IPC_DIR", &dir) };
    });
}

fn scratch_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "warp-station-probe-net-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

/// A `gh` that logs each run, then takes `seconds` to answer nothing.
fn fake_gh(dir: &Path, seconds: u32) -> String {
    let path = dir.join("gh");
    let log = dir.join("gh.log");
    std::fs::write(
        &path,
        format!(
            "#!/bin/sh\necho run >> \"{}\"\nsleep {seconds}\n",
            log.display()
        ),
    )
    .expect("fake gh");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("executable");
    path.display().to_string()
}

fn gh_runs(dir: &Path) -> usize {
    std::fs::read_to_string(dir.join("gh.log"))
        .map(|log| log.lines().count())
        .unwrap_or(0)
}

#[test]
fn a_changes_read_does_not_wait_out_a_pass_ahead_of_it() {
    scratch_ipc_dir();
    let dir = scratch_dir("pass");
    let gh_exe = fake_gh(&dir, 3);
    let (handle, cmd_tx, evt_rx) = spawn_net_thread();

    cmd_tx
        .send_blocking(NetCommand::GitHubPass {
            folders: vec![dir.clone()],
            gh_exe,
        })
        .expect("send");
    let deadline = Instant::now() + Duration::from_secs(2);
    while gh_runs(&dir) == 0 {
        assert!(Instant::now() < deadline, "the pass never started");
        std::thread::sleep(Duration::from_millis(10));
    }

    cmd_tx
        .send_blocking(NetCommand::LoadChanges {
            folder: dir.clone(),
        })
        .expect("send");

    loop {
        match evt_rx.recv_blocking() {
            Ok(UiEvent::ChangesLoaded { folder, .. }) => {
                assert_eq!(folder, dir.to_string_lossy());
                break;
            }
            Ok(UiEvent::GitHubSettled { .. }) => {
                panic!("the read queued behind the pass instead of overtaking it")
            }
            Ok(_) => continue,
            Err(err) => panic!("event channel closed: {err}"),
        }
    }

    drop(cmd_tx);
    let _ = handle.join();
}

/// ↑↓ through the list asks once per row, and `gh` is a network round trip per
/// call: only the row the burst lands on may reach it.
#[test]
fn a_burst_of_detail_requests_runs_gh_once() {
    scratch_ipc_dir();
    let dir = scratch_dir("burst");
    let gh_exe = fake_gh(&dir, 0);
    let folders: Vec<PathBuf> = ["one", "two", "three"]
        .iter()
        .map(|name| {
            let folder = dir.join(name);
            std::fs::create_dir_all(&folder).expect("scratch folder");
            folder
        })
        .collect();
    let (handle, cmd_tx, evt_rx) = spawn_net_thread();

    for folder in &folders {
        cmd_tx
            .send_blocking(NetCommand::GitHubDetails {
                folder: folder.clone(),
                gh_exe: gh_exe.clone(),
            })
            .expect("send");
    }

    loop {
        match evt_rx.recv_blocking() {
            Ok(UiEvent::GitHubDetailsLoaded { folder, .. }) => {
                assert_eq!(folder, folders[2].to_string_lossy(), "the last row answers");
                break;
            }
            Ok(_) => continue,
            Err(err) => panic!("event channel closed: {err}"),
        }
    }
    assert_eq!(gh_runs(&dir), 1, "the burst reaches gh once");

    drop(cmd_tx);
    let _ = handle.join();
}
