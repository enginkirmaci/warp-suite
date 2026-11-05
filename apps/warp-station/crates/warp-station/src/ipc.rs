//! The single-instance door: the first Warp Station binds a token-
//! authenticated loopback endpoint under its data folder, and a second launch
//! forwards "focus" to it and exits. Two live instances would otherwise write
//! the same settings and repo cache, run duplicate background passes and fight
//! over the clipboard-ping socket (audit B14).
//!
//! The wire is the suite's shared door contract (`warpui_kit_platform::door`):
//! one auth line, then one `{"id", "cmd"}` request and one answer. Only
//! `focus` exists here; a missing endpoint simply lets the launch proceed.

use crate::net::UiEvent;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read as _, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt as _, AsyncWriteExt, BufReader as AsyncBufReader};
use tokio::net::TcpListener;
use tokio::net::tcp::OwnedReadHalf;
use warpui_kit_platform::door::{self, MAX_LINE, token_matches};

/// The one command the door answers: bring the running window forward.
pub const FOCUS: &str = "focus";

/// Where the endpoint lives. Tests (and a scratch home) can redirect it with
/// `WARP_STATION_IPC_DIR`, so a probe's net thread never publishes into the
/// developer's real `~/.warp/station`.
fn dir() -> PathBuf {
    std::env::var_os("WARP_STATION_IPC_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(warp_station_core::paths::AppPaths::app_dir)
}

/// The bound door; held by the net thread so shutdown can clear exactly our
/// files and not a newer instance's.
pub struct Door {
    pub port: u16,
    pub token: String,
    dir: PathBuf,
}

/// Binds the door and starts answering; a bind failure only logs at the
/// caller — the app keeps working without single-instance handoff.
pub async fn start(events: async_channel::Sender<UiEvent>) -> anyhow::Result<Door> {
    start_in(dir(), events).await
}

/// Test seam for [`start`]: publish into `dir` instead.
pub async fn start_in(
    dir: PathBuf,
    events: async_channel::Sender<UiEvent>,
) -> anyhow::Result<Door> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
    let port = listener.local_addr()?.port();
    let token = door::random_token();
    std::fs::create_dir_all(&dir)?;
    // token first: a reader that sees a port is entitled to its token
    let token_path = door::token_file_in(&dir);
    door::write_private(&token_path, &token)?;
    door::restrict_perms(&token_path);
    let port_path = door::port_file_in(&dir);
    door::write_private(&port_path, &format!("{port}\n"))?;
    door::restrict_perms(&port_path);
    tokio::spawn(accept_loop(listener, events, token.clone()));
    Ok(Door { port, token, dir })
}

/// Clears the published endpoint when the files still point at this door.
pub fn unpublish(door: &Door) {
    let ours = |file: &Path, value: &str| {
        std::fs::read_to_string(file).is_ok_and(|current| current.trim() == value)
    };
    if ours(&door::port_file_in(&door.dir), &door.port.to_string()) {
        let _ = std::fs::remove_file(door::port_file_in(&door.dir));
    }
    if ours(&door::token_file_in(&door.dir), &door.token) {
        let _ = std::fs::remove_file(door::token_file_in(&door.dir));
    }
}

async fn accept_loop(listener: TcpListener, events: async_channel::Sender<UiEvent>, token: String) {
    loop {
        let Ok((stream, _)) = listener.accept().await else {
            break;
        };
        tokio::spawn(handle_conn(stream, events.clone(), token.clone()));
    }
}

async fn handle_conn(
    stream: tokio::net::TcpStream,
    events: async_channel::Sender<UiEvent>,
    token: String,
) {
    let _ = stream.set_nodelay(true);
    let (read, mut write) = stream.into_split();
    let mut reader = AsyncBufReader::new(read);

    let Some(line) = read_line(&mut reader).await else {
        return;
    };
    let authed = serde_json::from_str::<Value>(line.trim())
        .ok()
        .and_then(|value| {
            value
                .get("auth")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .is_some_and(|provided| token_matches(&provided, &token));
    if !authed {
        let _ = write
            .write_all(b"{\"ok\":false,\"error\":\"auth\"}\n")
            .await;
        return;
    }
    if write.write_all(b"{\"ok\":true}\n").await.is_err() {
        return;
    }

    let Some(line) = read_line(&mut reader).await else {
        return;
    };
    let Ok(request) = serde_json::from_str::<Value>(line.trim()) else {
        return;
    };
    let id = request.get("id").and_then(Value::as_u64).unwrap_or(0);
    let cmd = request.get("cmd").and_then(Value::as_str).unwrap_or("");
    let result = if cmd == FOCUS {
        match events.send(UiEvent::FocusRequested).await {
            Ok(()) => Ok(json!({})),
            Err(_) => Err("app is shutting down".to_string()),
        }
    } else {
        Err(format!("unknown command: {cmd}"))
    };
    let frame = door::answer_frame(id, result);
    let _ = write.write_all(format!("{frame}\n").as_bytes()).await;
    let _ = write.flush().await;
}

/// One NDJSON line with the suite's size cap; a longer line drops the
/// connection (it is a protocol error, not a command).
async fn read_line(reader: &mut AsyncBufReader<OwnedReadHalf>) -> Option<String> {
    let mut limited = reader.take(MAX_LINE as u64 + 1);
    let mut line = String::new();
    match limited.read_line(&mut line).await {
        Ok(0) => None,
        Ok(_) if line.len() > MAX_LINE => None,
        Ok(_) => Some(line),
        Err(_) => None,
    }
}

// ---- second-launch handoff (blocking, before GPUI starts) -------------------

/// True when a live instance answered and was asked to come forward; the
/// caller then exits without starting a second app. A stale endpoint (dead
/// port) reads false and the launch proceeds normally.
pub fn try_forward() -> bool {
    try_forward_in(&dir())
}

/// Test seam for [`try_forward`]: the endpoint files live in `dir`.
pub fn try_forward_in(dir: &Path) -> bool {
    let Some((port, token)) = door::read_endpoint_in(dir) else {
        return false;
    };
    let Ok(stream) = TcpStream::connect(("127.0.0.1", port)) else {
        return false;
    };
    let _ = stream.set_nodelay(true);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
    let Ok(reader_half) = stream.try_clone() else {
        return false;
    };
    let mut reader = BufReader::new(reader_half);
    let mut writer = stream;

    if !write_line(&mut writer, &json!({"auth": token})) {
        return false;
    }
    let Some(welcome) = read_frame(&mut reader) else {
        return false;
    };
    if welcome.get("ok").and_then(Value::as_bool) != Some(true) {
        return false;
    }
    if !write_line(&mut writer, &json!({"id": 1, "cmd": FOCUS})) {
        return false;
    }
    read_frame(&mut reader).and_then(|frame| frame.get("ok").and_then(Value::as_bool)) == Some(true)
}

fn write_line(stream: &mut TcpStream, frame: &Value) -> bool {
    stream
        .write_all(format!("{frame}\n").as_bytes())
        .and_then(|()| stream.flush())
        .is_ok()
}

fn read_frame(reader: &mut BufReader<TcpStream>) -> Option<Value> {
    let mut limited = reader.take(MAX_LINE as u64 + 1);
    let mut line = String::new();
    if limited.read_line(&mut line).ok()? == 0 || line.len() > MAX_LINE {
        return None;
    }
    serde_json::from_str(line.trim()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_second_launch_forwards_focus_and_clears_its_endpoint() {
        let dir = tempfile::tempdir().unwrap();
        let (events, receiver) = async_channel::unbounded::<UiEvent>();
        let door = start_in(dir.path().to_path_buf(), events).await.unwrap();
        assert!(door::read_endpoint_in(dir.path()).is_some());

        let endpoint = dir.path().to_path_buf();
        let forwarded = tokio::task::spawn_blocking(move || try_forward_in(&endpoint))
            .await
            .unwrap();
        assert!(forwarded, "the live door answers the forward");
        assert!(matches!(receiver.recv().await, Ok(UiEvent::FocusRequested)));

        unpublish(&door);
        assert!(door::read_endpoint_in(dir.path()).is_none());
        assert!(!try_forward_in(dir.path()), "no endpoint, no forward");
    }

    #[tokio::test]
    async fn a_wrong_token_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let (events, _receiver) = async_channel::unbounded::<UiEvent>();
        let _door = start_in(dir.path().to_path_buf(), events).await.unwrap();
        // tamper with the published token: the handshake must fail, not focus
        std::fs::write(door::token_file_in(dir.path()), "not-the-token\n").unwrap();
        let endpoint = dir.path().to_path_buf();
        let forwarded = tokio::task::spawn_blocking(move || try_forward_in(&endpoint))
            .await
            .unwrap();
        assert!(!forwarded);
    }
}
