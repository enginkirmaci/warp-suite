//! The warp-agent door's wire contract, in one place: the server lives in
//! warp-agent (`src/ipc.rs`), the client in warp-station-core
//! (`src/warp.rs`), and before this module both hand-rolled the framing,
//! the endpoint files and the constants — the contract was a comment in
//! `apps.toml`. Command vocabulary rides [`cmd`]; the request/answer frame
//! builders keep the reserved-field rules on one side.

use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// The wire protocol version a `ping` answers with.
pub const PROTOCOL: u32 = 2;
/// Prompts ride whole on one line; repository trees inside them grow large.
/// Both sides drop the connection over this.
pub const MAX_LINE: usize = 8 * 1024 * 1024;
/// How long a door client waits for a command answer. run-prompt is exempt:
/// its answer lands when the turn idles, minutes later.
pub const ANSWER_TIMEOUT: Duration = Duration::from_secs(60);

/// The command vocabulary. Both sides spell requests through these — a typo
/// breaks the build, not the runtime.
pub mod cmd {
    pub const PING: &str = "ping";
    pub const OPEN_WORKSPACE: &str = "open-workspace";
    pub const NEW_CHAT: &str = "new-chat";
    pub const RUN_PROMPT: &str = "run-prompt";
    pub const CANCEL: &str = "cancel";
    pub const MODELS: &str = "models";
    pub const STATUS: &str = "status";
}

pub fn port_file_in(dir: &Path) -> PathBuf {
    dir.join("ipc.port")
}

pub fn token_file_in(dir: &Path) -> PathBuf {
    dir.join("ipc.token")
}

/// The door endpoint as currently published; None until an instance binds.
pub fn read_endpoint_in(dir: &Path) -> Option<(u16, String)> {
    let port = std::fs::read_to_string(port_file_in(dir))
        .ok()?
        .trim()
        .parse()
        .ok()?;
    let token = std::fs::read_to_string(token_file_in(dir))
        .ok()?
        .trim()
        .to_string();
    if token.is_empty() {
        return None;
    }
    Some((port, token))
}

/// Length-checked, byte-folded compare: no early exit on content, so the
/// auth response's timing carries no prefix information about the token.
pub fn token_matches(provided: &str, token: &str) -> bool {
    provided.len() == token.len()
        && provided
            .bytes()
            .zip(token.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

/// The client's request frame: `{"id", "cmd", **params}` — the params merge
/// over the framing keys' namespace, so a param named `cmd` would shadow the
/// command; the door's clients never send one.
pub fn request_frame(id: u64, cmd: &str, params: Value) -> Value {
    let mut frame = json!({"id": id, "cmd": cmd});
    if let (Some(target), Some(extra)) = (frame.as_object_mut(), params.as_object()) {
        for (key, value) in extra {
            target.insert(key.clone(), value.clone());
        }
    }
    frame
}

/// The server's answer frame: Ok merges the payload into `{"id","ok":true}`,
/// Err becomes `{"id","ok":false,"error"}`. `id`/`ok` are the reserved
/// framing fields — a payload that ever grew one would corrupt the answer
/// for every client, so they are dropped here.
pub fn answer_frame(id: u64, result: Result<Value, String>) -> Value {
    match result {
        Ok(payload) => {
            let mut frame = json!({"id": id, "ok": true});
            if let (Some(target), Some(extra)) = (frame.as_object_mut(), payload.as_object()) {
                for (key, value) in extra {
                    if key != "id" && key != "ok" {
                        target.insert(key.clone(), value.clone());
                    }
                }
            }
            frame
        }
        Err(error) => json!({"id": id, "ok": false, "error": error}),
    }
}

/// Creates `path` with 0o600 already in place on Unix — the mode is set on
/// the open, so no world-readable window exists before a later chmod.
#[cfg(unix)]
pub fn write_private(path: &Path, data: &str) -> std::io::Result<()> {
    use std::io::Write as _;
    use std::os::unix::fs::OpenOptionsExt as _;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(data.as_bytes())
}

#[cfg(not(unix))]
pub fn write_private(path: &Path, data: &str) -> std::io::Result<()> {
    std::fs::write(path, data)
}

#[cfg(unix)]
pub fn restrict_perms(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Err(err) = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)) {
        tracing::warn!("could not restrict {}: {err}", path.display());
    }
}

#[cfg(not(unix))]
pub fn restrict_perms(_path: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_compare_rejects_mismatches_without_early_exit() {
        let token = "aabbccdd";
        assert!(token_matches("aabbccdd", token));
        assert!(!token_matches("aabbccde", token));
        assert!(!token_matches("aabbccd", token));
        assert!(!token_matches("", token));
    }

    #[test]
    fn answer_frame_merges_payload_and_drops_reserved_keys() {
        let answer = answer_frame(7, Ok(json!({"workspaces": [], "id": 99, "ok": false})));
        assert_eq!(answer["id"], 7);
        assert_eq!(answer["ok"], true);
        assert_eq!(answer["workspaces"], json!([]));

        let failed = answer_frame(8, Err("timed out".into()));
        assert_eq!(failed, json!({"id": 8, "ok": false, "error": "timed out"}));
    }

    #[test]
    fn request_frame_merges_params() {
        let frame = request_frame(
            3,
            cmd::RUN_PROMPT,
            json!({"prompt": "hi", "background": true}),
        );
        assert_eq!(frame["id"], 3);
        assert_eq!(frame["cmd"], "run-prompt");
        assert_eq!(frame["prompt"], "hi");
        assert_eq!(frame["background"], true);
    }

    #[test]
    fn endpoint_roundtrips_through_the_files() {
        let dir = std::env::temp_dir().join(format!("warp-door-ep-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(read_endpoint_in(&dir), None);

        // a port without its token is no endpoint: the reader treats the
        // pair as one publication
        write_private(&port_file_in(&dir), "59991\n").unwrap();
        assert_eq!(read_endpoint_in(&dir), None);
        write_private(&token_file_in(&dir), "tok\n").unwrap();
        assert_eq!(read_endpoint_in(&dir), Some((59991, "tok".into())));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
