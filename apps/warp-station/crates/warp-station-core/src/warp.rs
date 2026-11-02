//! The warp-agent door client. Warp Station' AI surface (wands, summary, model
//! catalog) runs in warp-agent over its token-authenticated loopback door —
//! Warp Station never spawns an opencode server itself anymore. A missing
//! warp-agent turns the AI features off; a merely closed one is launched on
//! demand and the run continues.

use anyhow::{Context as _, Result, anyhow};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

use crate::opencode::{CatalogHarness, CatalogModel, ModelCatalog, default_harnesses};
use crate::settings::AppSettings;

/// Names warp-agent's built-in General workspace on the wire.
pub const GENERAL: &str = "general";

/// The harness/model/variant every Warp Station AI run asks warp-agent for. One
/// selection drives the wands, commit messages and the daily summary.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModelChoice {
    pub harness: Option<String>,
    pub model: Option<String>,
    pub variant: Option<String>,
}

impl ModelChoice {
    /// The persisted selection. An empty model leaves warp-agent's default
    /// in place; a variant without a model has nothing to attach to, so it is
    /// dropped.
    pub fn from_settings(settings: &AppSettings) -> Self {
        let model = settings.opencode.model.trim().to_string();
        let model = (!model.is_empty()).then_some(model);
        let variant = model.as_ref().and_then(|_| {
            settings
                .warp_agent
                .variant
                .as_deref()
                .map(str::trim)
                .filter(|variant| !variant.is_empty())
                .map(str::to_string)
        });
        Self {
            harness: Some(settings.warp_agent.selected_harness().to_string()),
            model,
            variant,
        }
    }

    /// The door fields shared by new-chat and run-prompt.
    fn apply(&self, params: &mut Value) {
        if let Some(harness) = self.harness.as_deref().filter(|value| !value.is_empty()) {
            params["harness"] = json!(harness);
        }
        if let Some(model) = self.model.as_deref().filter(|value| !value.is_empty()) {
            params["model"] = json!(model);
        }
        if let Some(variant) = self
            .variant
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            params["variant"] = json!(variant);
        }
    }
}

/// How long a door command may take to answer (run-prompt waits for the
/// turn itself and is exempt on the server side).
const ANSWER_TIMEOUT: Duration = Duration::from_secs(60);
/// How long a launch-and-connect attempt keeps polling the door.
const LAUNCH_DEADLINE: Duration = Duration::from_secs(15);

pub fn port_file() -> PathBuf {
    config_dir().join("ipc.port")
}

pub fn token_file() -> PathBuf {
    config_dir().join("ipc.token")
}

fn config_dir() -> PathBuf {
    crate::paths::home().join(".warp-agent")
}

/// The door endpoint as published by a running warp-agent.
pub fn read_endpoint() -> Option<(u16, String)> {
    let port = std::fs::read_to_string(port_file())
        .ok()?
        .trim()
        .parse()
        .ok()?;
    let token = std::fs::read_to_string(token_file())
        .ok()?
        .trim()
        .to_string();
    if token.is_empty() {
        return None;
    }
    Some((port, token))
}

/// One authenticated door connection. Line frames: requests out, responses
/// and server pushes in, matched by id.
pub struct Client {
    reader: BufReader<TcpStream>,
    next_id: u64,
}

impl Client {
    pub async fn connect() -> Result<Self> {
        Self::connect_in(&config_dir()).await
    }

    /// Test seam for [`connect`]: the endpoint files live in `dir`.
    pub async fn connect_in(dir: &Path) -> Result<Self> {
        let read = |file: &str| std::fs::read_to_string(dir.join(file));
        let port: u16 = read("ipc.port")
            .ok()
            .and_then(|text| text.trim().parse().ok())
            .ok_or_else(|| anyhow!("warp-agent is not running (no door endpoint)"))?;
        let token = read("ipc.token").ok().unwrap_or_default();
        let token = token.trim().to_string();
        if token.is_empty() {
            return Err(anyhow!("warp-agent is not running (no door token)"));
        }
        let stream = TcpStream::connect(("127.0.0.1", port))
            .await
            .context("warp-agent door connect failed")?;
        stream.set_nodelay(true).ok();
        let mut client = Self {
            reader: BufReader::new(stream),
            next_id: 1,
        };
        client.write(&json!({"auth": token})).await?;
        let welcome = client.read_frame().await?;
        if !welcome.get("ok").and_then(Value::as_bool).unwrap_or(false) {
            return Err(anyhow!("warp-agent door rejected the token"));
        }
        Ok(client)
    }

    /// True when a running warp-agent answered a ping.
    pub async fn probe() -> bool {
        match Self::connect().await {
            Ok(mut client) => client.request("ping", json!({})).await.is_ok(),
            Err(_) => false,
        }
    }

    async fn write(&mut self, frame: &Value) -> Result<()> {
        let raw = self.reader.get_mut();
        raw.write_all(format!("{frame}\n").as_bytes()).await?;
        raw.flush().await?;
        Ok(())
    }

    /// Reads one NDJSON frame; an oversized line drops the connection (the
    /// door promises the same cap on its side).
    async fn read_frame(&mut self) -> Result<Value> {
        let mut limited = (&mut self.reader).take(8 * 1024 * 1024 + 1);
        let mut line = String::new();
        if limited.read_line(&mut line).await? == 0 {
            return Err(anyhow!("warp-agent closed the door"));
        }
        if line.len() > 8 * 1024 * 1024 {
            return Err(anyhow!("oversized frame from warp-agent"));
        }
        Ok(serde_json::from_str(line.trim())?)
    }

    /// Reads the next raw frame (a push event or an answer); the status
    /// watcher rides this until the door drops.
    pub async fn next_frame(&mut self) -> Result<Value> {
        self.read_frame().await
    }

    /// Sends one command and waits for its answer, ignoring pushes.
    pub async fn request(&mut self, cmd: &str, params: Value) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        let mut frame = json!({"id": id, "cmd": cmd});
        if let (Some(target), Some(extra)) = (frame.as_object_mut(), params.as_object()) {
            for (key, value) in extra {
                target.insert(key.clone(), value.clone());
            }
        }
        self.write(&frame).await?;
        let deadline = tokio::time::sleep(ANSWER_TIMEOUT);
        tokio::pin!(deadline);
        loop {
            let answer = tokio::select! {
                _ = &mut deadline => return Err(anyhow!("warp-agent did not answer {cmd} in time")),
                frame = self.read_frame() => frame?,
            };
            if answer.get("id").and_then(Value::as_u64) != Some(id) {
                continue;
            }
            if answer.get("ok").and_then(Value::as_bool).unwrap_or(false) {
                return Ok(answer);
            }
            let error = answer
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("unknown error");
            return Err(anyhow!("{error}"));
        }
    }

    pub async fn open_workspace(&mut self, path: &Path) -> Result<()> {
        self.request("open-workspace", json!({"path": path.to_string_lossy()}))
            .await
            .map(|_| ())
    }

    pub async fn new_chat(
        &mut self,
        workspace: &str,
        prefill: &str,
        auto_send: bool,
        choice: &ModelChoice,
    ) -> Result<()> {
        let mut params =
            json!({"workspace": workspace, "prefill": prefill, "auto_send": auto_send});
        choice.apply(&mut params);
        self.request("new-chat", params).await.map(|_| ())
    }

    /// The harness's model catalog with the variants each model offers. The
    /// door starts that harness when it has no catalog yet, so this can take
    /// as long as a server boot.
    pub async fn models(&mut self, harness: &str) -> Result<ModelCatalog> {
        let answer = self.request("models", json!({"harness": harness})).await?;
        Ok(catalog_from_answer(&answer, harness))
    }

    pub async fn status(&mut self) -> Result<Value> {
        self.request("status", json!({})).await.map(|answer| {
            answer
                .get("workspaces")
                .cloned()
                .unwrap_or(Value::Array(Vec::new()))
        })
    }

    /// Runs one prompt in a fresh warp-agent chat and returns the final
    /// assistant text. `on_log` receives the run's progress lines; a signal
    /// on `cancel` aborts the run server-side and settles as an error.
    pub async fn run_prompt(
        &mut self,
        workspace: &str,
        prompt: &str,
        choice: &ModelChoice,
        mut on_log: impl FnMut(String),
        cancel: async_channel::Receiver<()>,
    ) -> Result<String> {
        let id = self.next_id;
        self.next_id += 1;
        let mut params =
            json!({"id": id, "cmd": "run-prompt", "workspace": workspace, "prompt": prompt});
        choice.apply(&mut params);
        self.write(&params).await?;

        let deadline = tokio::time::sleep(crate::opencode::RUN_CAP);
        tokio::pin!(deadline);
        let mut cancelled = false;
        loop {
            let answer = tokio::select! {
                biased;
                _ = &mut deadline, if !cancelled => {
                    let _ = self.request("cancel", json!({"run": id})).await;
                    return Err(anyhow!("the wand run exceeded its time cap"));
                }
                stopped = cancel.recv(), if !cancelled => {
                    if stopped.is_ok() {
                        cancelled = true;
                        let cancel_frame = json!({"id": self.next_id, "cmd": "cancel", "run": id});
                        self.next_id += 1;
                        self.write(&cancel_frame).await?;
                    }
                    continue;
                }
                frame = self.read_frame() => frame?,
            };
            if answer.get("event").and_then(Value::as_str) == Some("run-log")
                && answer.get("run").and_then(Value::as_u64) == Some(id)
                && let Some(line) = answer.get("line").and_then(Value::as_str)
            {
                on_log(line.to_string());
                continue;
            }
            if answer.get("id").and_then(Value::as_u64) != Some(id) {
                continue;
            }
            if answer.get("ok").and_then(Value::as_bool).unwrap_or(false) {
                let text = answer
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let text = text.trim().to_string();
                if text.is_empty() {
                    return Err(anyhow!("the model returned no text"));
                }
                return Ok(text);
            }
            let error = answer
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("unknown error");
            return Err(anyhow!("{error}"));
        }
    }
}

/// Parses one `models` answer. A newer door sends `harnesses` + `catalog`;
/// an older one only sends the flat `models` key list, which still maps to
/// the requested harness with no variants instead of emptying the pickers.
fn catalog_from_answer(answer: &Value, harness: &str) -> ModelCatalog {
    let mut harnesses: Vec<CatalogHarness> = answer
        .get("harnesses")
        .and_then(|value| serde_json::from_value(value.clone()).ok())
        .unwrap_or_default();
    if harnesses.is_empty() {
        harnesses = default_harnesses();
    }
    let mut models: Vec<CatalogModel> = answer
        .get("catalog")
        .and_then(|value| serde_json::from_value(value.clone()).ok())
        .unwrap_or_default();
    if models.is_empty() {
        models = answer
            .get("models")
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .filter_map(Value::as_str)
                    .map(|key| CatalogModel {
                        key: key.to_string(),
                        harness: harness.to_string(),
                        variants: Vec::new(),
                        context_limit: 0,
                    })
                    .collect()
            })
            .unwrap_or_default();
    }
    for model in &mut models {
        model.key = model.key.trim().to_string();
        if model.harness.trim().is_empty() {
            model.harness = harness.to_string();
        }
    }
    models.retain(|model| !model.key.is_empty() && !model.key.starts_with('('));
    ModelCatalog { harnesses, models }
}

/// Connects to the door, launching warp-agent when it is merely closed. The
/// launch opens the binary with `folder` so the workspace is right there.
/// Err means: not installed, or it never came up.
pub async fn connect_or_launch(exe: &Path, folder: Option<&Path>) -> Result<Client> {
    if let Ok(client) = Client::connect().await {
        return Ok(client);
    }
    let mut command = std::process::Command::new(exe);
    if let Some(folder) = folder {
        command.arg(folder);
    }
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    hide_console(&mut command);
    command
        .spawn()
        .with_context(|| format!("'{}' failed to start", exe.display()))?;

    let deadline = std::time::Instant::now() + LAUNCH_DEADLINE;
    loop {
        if std::time::Instant::now() >= deadline {
            return Err(anyhow!("warp-agent did not open its door within 15 s"));
        }
        if let Ok(client) = Client::connect().await {
            return Ok(client);
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
}

fn hide_console(command: &mut std::process::Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    {
        let _ = command;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    /// A door-speaking mock: auth, then canned command answers.
    async fn mock_door(answers: Vec<(&'static str, Value)>) -> (u16, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (read_half, mut write_half) = stream.into_split();
            let mut reader = BufReader::new(read_half);
            let mut line = String::new();
            reader.read_line(&mut line).await.unwrap();
            write_half
                .write_all(format!("{}\n", json!({"ok": true})).as_bytes())
                .await
                .unwrap();
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).await.unwrap() == 0 {
                    return;
                }
                let request: Value = serde_json::from_str(line.trim()).unwrap();
                let cmd = request
                    .get("cmd")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let id = request
                    .get("id")
                    .and_then(Value::as_u64)
                    .unwrap_or_default();
                let Some((_, answer)) = answers.iter().find(|(name, _)| *name == cmd) else {
                    continue;
                };
                let mut frame = answer.clone();
                frame["id"] = json!(id);
                frame["ok"] = json!(true);
                write_half
                    .write_all(format!("{frame}\n").as_bytes())
                    .await
                    .unwrap();
            }
        });
        (port, handle)
    }

    #[tokio::test]
    async fn client_roundtrips_commands() {
        let (port, door) = mock_door(vec![
            ("ping", json!({})),
            (
                "models",
                json!({
                    "harnesses": [
                        {"id": "opencode", "label": "opencode"},
                        {"id": "dsh", "label": "DeepSeek (dsh)"}
                    ],
                    "catalog": [
                        {"key": "a/x", "harness": "opencode", "variants": ["low", "high"]},
                        {"key": "b/y", "harness": "opencode", "variants": []}
                    ],
                    "models": ["a/x", "b/y"]
                }),
            ),
        ])
        .await;
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("ipc.token"), "test-token\n").unwrap();
        std::fs::write(dir.path().join("ipc.port"), format!("{port}\n")).unwrap();
        let mut client = Client::connect_in(dir.path()).await.unwrap();
        assert!(client.request("ping", json!({})).await.is_ok());
        let catalog = client.models("opencode").await.unwrap();
        assert_eq!(catalog.harnesses.len(), 2);
        assert_eq!(catalog.models.len(), 2);
        assert_eq!(catalog.variants_for("opencode", "a/x"), ["low", "high"]);
        let _ = client.reader.get_mut().shutdown().await;
        let _ = door.await;
    }

    #[test]
    fn legacy_models_answer_maps_to_the_requested_harness() {
        let catalog = catalog_from_answer(&json!({"models": ["a/x", "b/y"]}), "dsh");
        assert_eq!(catalog.models.len(), 2);
        assert!(catalog.models.iter().all(|model| model.harness == "dsh"));
        assert!(catalog.models.iter().all(|model| model.variants.is_empty()));
        assert_eq!(catalog.harnesses, default_harnesses());
    }

    #[test]
    fn model_choice_carries_the_one_harness_model_and_variant() {
        let settings: AppSettings = serde_json::from_str(
            r#"{
                "OpenCode": {"DefaultModel": "zai/glm"},
                "WarpAgent": {"Harness": "dsh", "Variant": "max"}
            }"#,
        )
        .unwrap();
        let choice = ModelChoice::from_settings(&settings);
        let mut params = json!({});
        choice.apply(&mut params);
        assert_eq!(
            params,
            json!({"harness": "dsh", "model": "zai/glm", "variant": "max"})
        );

        // an empty model drops the variant too: there is nothing to attach it
        // to, and the door keeps its own default in that case
        let empty = ModelChoice::from_settings(&AppSettings::default());
        assert_eq!(empty.harness.as_deref(), Some("opencode"));
        assert_eq!(empty.model, None);
        assert_eq!(empty.variant, None);
    }
}
