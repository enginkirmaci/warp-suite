//! Versioned JSON documents.
//!
//! Every settings, UI-state and cache file the suite owns is one object with a
//! top-level `schema_version`. `Store<T>` adds and checks that key around a
//! typed body, replaces the file atomically, and can serialize writers through
//! a sidecar lock. `SharedDoc` is the same contract for the shared `ui.json`,
//! where several apps each own one key and must not drop the others.
//!
//! The store never rewrites keys. A file written by an older suite is an
//! importer's input, not something the live store normalizes.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// The reserved top-level key carrying the document's schema version.
pub const VERSION_KEY: &str = "schema_version";

/// The schema version this build writes.
pub const CURRENT_VERSION: u32 = 1;

/// Normalize one legacy key to the new-app spelling. Brands stay one word
/// (`GitHub` -> `github`, `OpenCode` -> `opencode`, `SnapIt` -> `snapit`);
/// the rest splits on camel/Pascal boundaries (`AutoAllowMin` ->
/// `auto_allow_min`). An already-snake key is returned unchanged.
pub fn snake_key(key: &str) -> String {
    if !key.bytes().any(|byte| byte.is_ascii_uppercase()) {
        return key.to_string();
    }
    let key = key
        .replace("VSCode", "Vscode")
        .replace("OpenCode", "Opencode")
        .replace("DeepSeek", "Deepseek")
        .replace("SnapIt", "Snapit")
        .replace("GitHub", "Github")
        .replace("AzureDevOps", "AzureDevops");
    let chars: Vec<char> = key.chars().collect();
    let mut out = String::with_capacity(key.len() + 4);
    for (index, ch) in chars.iter().enumerate() {
        if !ch.is_ascii_uppercase() {
            out.push(*ch);
            continue;
        }
        let previous = index.checked_sub(1).map(|i| chars[i]);
        let next = chars.get(index + 1).copied();
        let boundary = index > 0
            && (previous.is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
                || (previous.is_some_and(|c| c.is_ascii_uppercase())
                    && next.is_some_and(|c| c.is_ascii_lowercase())));
        if boundary {
            out.push('_');
        }
        out.push(ch.to_ascii_lowercase());
    }
    out
}

/// Rename every object key under `value` with [`snake_key`]. Keys in
/// `opaque` are renamed but their object values are not walked: those keys
/// are user data (session ids, folder paths, monitor names).
///
/// Returns true when at least one key changed. A collision is refused
/// (`Err`) so an importer never silently keeps one of two values.
pub fn rename_legacy_keys(value: &mut Value, opaque: &[&str]) -> Result<bool, String> {
    fn walk(value: &mut Value, opaque: &[&str], changed: &mut bool) -> Result<(), String> {
        match value {
            Value::Object(object) => {
                let old = std::mem::take(object);
                let mut renamed = Map::with_capacity(old.len());
                for (key, mut child) in old {
                    let next = snake_key(&key);
                    if next != key {
                        *changed = true;
                    }
                    if renamed.contains_key(&next) {
                        return Err(format!("key collision: {key} and {next}"));
                    }
                    if !opaque.contains(&next.as_str()) {
                        walk(&mut child, opaque, changed)?;
                    }
                    renamed.insert(next, child);
                }
                *object = renamed;
                Ok(())
            }
            Value::Array(items) => {
                for item in items {
                    walk(item, opaque, changed)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    let mut changed = false;
    walk(value, opaque, &mut changed)?;
    Ok(changed)
}

/// A versioned document failure.
#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Json(serde_json::Error),
    /// The document is not a JSON object.
    NotAnObject,
    /// The document has no `schema_version` (a pre-versioning file).
    Unversioned,
    /// The document was written by an older schema.
    Older {
        found: u64,
        supported: u32,
    },
    /// The document was written by a newer schema; leave it alone.
    Newer {
        found: u64,
        supported: u32,
    },
}

impl Error {
    /// True when the file predates versioning or uses an older schema, so an
    /// importer may translate it.
    pub fn needs_import(&self) -> bool {
        matches!(
            self,
            Error::Unversioned | Error::Older { .. } | Error::NotAnObject
        )
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(err) => write!(f, "{err}"),
            Error::Json(err) => write!(f, "{err}"),
            Error::NotAnObject => f.write_str("the document is not a JSON object"),
            Error::Unversioned => f.write_str("the document has no schema_version"),
            Error::Older { found, supported } => {
                write!(f, "schema_version {found} is older than {supported}")
            }
            Error::Newer { found, supported } => write!(
                f,
                "schema_version {found} is newer than this build's {supported}"
            ),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(err) => Some(err),
            Error::Json(err) => Some(err),
            _ => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Error::Io(err)
    }
}

impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Error::Json(err)
    }
}

/// A typed, versioned JSON document at one path.
pub struct Store<T> {
    path: PathBuf,
    version: u32,
    lock: bool,
    marker: PhantomData<fn() -> T>,
}

impl<T> Store<T>
where
    T: Serialize + DeserializeOwned + Default,
{
    /// A version-1 store at `path`, without a sidecar lock.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            version: CURRENT_VERSION,
            lock: false,
            marker: PhantomData,
        }
    }

    /// Serialize writers through `<path>.json.lock` (or `.lock` for a file
    /// without a `.json` suffix). Shared files use this; per-app files have
    /// one owner and do not need it.
    pub fn with_lock(mut self) -> Self {
        self.lock = true;
        self
    }

    /// Override the schema version this store writes and accepts.
    pub fn with_version(mut self, version: u32) -> Self {
        self.version = version;
        self
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Load the document. A missing file is the type's default; an old or
    /// unversioned file is an error so the caller can run its importer first.
    pub fn load(&self) -> Result<T, Error> {
        match fs::read_to_string(&self.path) {
            Ok(text) => self.parse(&text),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
            Err(err) => Err(Error::Io(err)),
        }
    }

    /// Load the document, falling back to the default when it is missing or
    /// still in the pre-versioning shape. Corrupt JSON is still an error.
    pub fn load_or_default(&self) -> Result<T, Error> {
        match self.load() {
            Ok(value) => Ok(value),
            Err(err) if err.needs_import() => Ok(T::default()),
            Err(err) => Err(err),
        }
    }

    /// Write the document with `schema_version` added.
    pub fn save(&self, value: &T) -> Result<(), Error> {
        self.ensure_writable()?;
        let mut value = serde_json::to_value(value)?;
        let object = value.as_object_mut().ok_or(Error::NotAnObject)?;
        object.insert(VERSION_KEY.to_string(), Value::from(self.version));
        let text = serde_json::to_string_pretty(&value)?;
        self.write(&text)
    }

    /// Load, mutate and save as one transition.
    pub fn update(&self, mutate: impl FnOnce(&mut T)) -> Result<T, Error> {
        let mut value = self.load()?;
        mutate(&mut value);
        self.save(&value)?;
        Ok(value)
    }

    fn parse(&self, text: &str) -> Result<T, Error> {
        let mut value: Value = serde_json::from_str(text)?;
        let object = value.as_object_mut().ok_or(Error::NotAnObject)?;
        let found = match object.remove(VERSION_KEY) {
            Some(version) => version.as_u64().ok_or(Error::Unversioned)?,
            None => 0,
        };
        if found == 0 {
            return Err(Error::Unversioned);
        }
        if found > u64::from(self.version) {
            return Err(Error::Newer {
                found,
                supported: self.version,
            });
        }
        if found < u64::from(self.version) {
            return Err(Error::Older {
                found,
                supported: self.version,
            });
        }
        serde_json::from_value(value).map_err(Error::Json)
    }

    /// Never replace a document written by a newer build.
    fn ensure_writable(&self) -> Result<(), Error> {
        let Ok(text) = fs::read_to_string(&self.path) else {
            return Ok(());
        };
        let Ok(value) = serde_json::from_str::<Value>(&text) else {
            return Ok(());
        };
        let Some(found) = value.get(VERSION_KEY).and_then(Value::as_u64) else {
            return Ok(());
        };
        if found > u64::from(self.version) {
            return Err(Error::Newer {
                found,
                supported: self.version,
            });
        }
        Ok(())
    }

    fn write(&self, text: &str) -> Result<(), Error> {
        if self.lock {
            with_lock(&self.path, || {
                crate::io::write_atomically(&self.path, text)
                    .map_err(|err| Error::Io(std::io::Error::other(format!("{err:#}"))))
            })
        } else {
            crate::io::write_atomically(&self.path, text)
                .map_err(|err| Error::Io(std::io::Error::other(format!("{err:#}"))))
        }
    }
}

/// The shared `ui.json`: an additive map with a schema version, where every
/// app owns one key and unknown keys survive a write.
pub struct SharedDoc {
    path: PathBuf,
    version: u32,
}

impl SharedDoc {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            version: CURRENT_VERSION,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Load the map without the reserved version key. A missing file is an
    /// empty map; a newer schema is an error, an older one is readable.
    pub fn load(&self) -> Result<Map<String, Value>, Error> {
        let text = match fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Map::new()),
            Err(err) => return Err(Error::Io(err)),
        };
        let mut value: Value = serde_json::from_str(&text)?;
        let object = value.as_object_mut().ok_or(Error::NotAnObject)?;
        let found = match object.remove(VERSION_KEY) {
            Some(version) => version.as_u64().ok_or(Error::Unversioned)?,
            None => 0,
        };
        if found > u64::from(self.version) {
            return Err(Error::Newer {
                found,
                supported: self.version,
            });
        }
        Ok(object.clone())
    }

    /// Write the map with `schema_version` added, under the shared lock.
    pub fn save(&self, map: &Map<String, Value>) -> Result<(), Error> {
        self.ensure_writable()?;
        with_lock(&self.path, || self.write_unlocked(map))
    }

    fn ensure_writable(&self) -> Result<(), Error> {
        let Ok(text) = fs::read_to_string(&self.path) else {
            return Ok(());
        };
        let Ok(value) = serde_json::from_str::<Value>(&text) else {
            return Ok(());
        };
        let Some(found) = value.get(VERSION_KEY).and_then(Value::as_u64) else {
            return Ok(());
        };
        if found > u64::from(self.version) {
            return Err(Error::Newer {
                found,
                supported: self.version,
            });
        }
        Ok(())
    }

    fn write_unlocked(&self, map: &Map<String, Value>) -> Result<(), Error> {
        let mut object = map.clone();
        object.insert(VERSION_KEY.to_string(), Value::from(self.version));
        let text = serde_json::to_string_pretty(&object)?;
        crate::io::write_atomically(&self.path, &text)
            .map_err(|err| Error::Io(std::io::Error::other(format!("{err:#}"))))
    }

    /// Read-modify-write one map in one locked transition.
    pub fn update(
        &self,
        mutate: impl FnOnce(&mut Map<String, Value>),
    ) -> Result<Map<String, Value>, Error> {
        with_lock(&self.path, || {
            let mut map = self.load()?;
            mutate(&mut map);
            self.write_unlocked(&map)?;
            Ok(map)
        })
    }

    /// Like [`update`](Self::update), but a corrupt file is replaced instead
    /// of blocking the write. A newer schema is still left alone.
    pub fn update_repairing(
        &self,
        mutate: impl FnOnce(&mut Map<String, Value>),
    ) -> Result<Map<String, Value>, Error> {
        with_lock(&self.path, || {
            let mut map = match self.load() {
                Ok(map) => map,
                Err(Error::Json(_)) => Map::new(),
                Err(err) => return Err(err),
            };
            mutate(&mut map);
            self.write_unlocked(&map)?;
            Ok(map)
        })
    }

    /// Add the version key to an existing pre-versioning file without
    /// changing any of its keys.
    pub fn ensure_version(&self) -> Result<(), Error> {
        if !self.path.exists() {
            return Ok(());
        }
        let text = fs::read_to_string(&self.path)?;
        let value: Value = serde_json::from_str(&text)?;
        let object = value.as_object().ok_or(Error::NotAnObject)?;
        if object.contains_key(VERSION_KEY) {
            return Ok(());
        }
        self.save(&object.clone())
    }
}

/// A map whose keys are user data (session ids, folder paths, monitor names).
/// The transparent newtype documents that no casing pass may touch them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct KeyMap<V>(pub BTreeMap<String, V>);

impl<V> KeyMap<V> {
    pub fn new() -> Self {
        Self(BTreeMap::new())
    }

    pub fn get(&self, key: &str) -> Option<&V> {
        self.0.get(key)
    }

    pub fn insert(&mut self, key: String, value: V) -> Option<V> {
        self.0.insert(key, value)
    }

    pub fn remove(&mut self, key: &str) -> Option<V> {
        self.0.remove(key)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &V)> {
        self.0.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl<V> Deref for KeyMap<V> {
    type Target = BTreeMap<String, V>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<V> DerefMut for KeyMap<V> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<V> FromIterator<(String, V)> for KeyMap<V> {
    fn from_iter<T: IntoIterator<Item = (String, V)>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

/// Run `write` under the sidecar lock beside `path`. The lock is best effort:
/// a filesystem that refuses it still gets the atomic write, matching the
/// shared `ui.json` writer this replaces.
fn with_lock<T>(path: &Path, write: impl FnOnce() -> Result<T, Error>) -> Result<T, Error> {
    let lock_path = path.with_extension("json.lock");
    let _ = fs::create_dir_all(lock_path.parent().unwrap_or(Path::new(".")));
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&lock_path)
        .ok();
    if let Some(lock) = &lock {
        let _ = lock.lock();
    }
    let out = write();
    if let Some(lock) = &lock {
        let _ = lock.unlock();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir =
            std::env::temp_dir().join(format!("warp-config-{name}-{}-{nanos}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Doc {
        name: String,
        sessions: KeyMap<bool>,
    }

    #[test]
    fn save_adds_the_version_and_round_trips() {
        let dir = scratch("round-trip");
        let path = dir.join("settings.json");
        let store = Store::<Doc>::new(path.clone());
        let mut doc = Doc {
            name: "alpha".into(),
            ..Doc::default()
        };
        doc.sessions.insert("ses_UPPER".into(), true);
        store.save(&doc).unwrap();

        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"schema_version\": 1"), "{text}");
        assert!(text.contains("\"ses_UPPER\""), "the opaque key stays as-is");
        assert_eq!(store.load().unwrap(), doc);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_file_is_default() {
        let dir = scratch("missing");
        let store = Store::<Doc>::new(dir.join("settings.json"));
        assert_eq!(store.load().unwrap(), Doc::default());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn unversioned_and_older_documents_need_an_importer() {
        let dir = scratch("legacy");
        let path = dir.join("settings.json");
        fs::write(&path, r#"{"name":"alpha"}"#).unwrap();
        let store = Store::<Doc>::new(path.clone());
        assert!(matches!(store.load(), Err(Error::Unversioned)));
        assert!(store.load().unwrap_err().needs_import());
        fs::write(&path, r#"{"schema_version":0,"name":"alpha"}"#).unwrap();
        assert!(matches!(store.load(), Err(Error::Unversioned)));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_newer_document_is_left_alone() {
        let dir = scratch("newer");
        let path = dir.join("settings.json");
        let original = r#"{"schema_version":9,"name":"future"}"#;
        fs::write(&path, original).unwrap();
        let store = Store::<Doc>::new(path.clone());
        let err = store.load().unwrap_err();
        assert!(matches!(err, Error::Newer { found: 9, .. }), "{err}");
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_refuses_to_replace_a_newer_document() {
        let dir = scratch("newer-write");
        let path = dir.join("settings.json");
        let original = r#"{"schema_version":9,"name":"future"}"#;
        fs::write(&path, original).unwrap();
        let store = Store::<Doc>::new(path.clone());
        assert!(matches!(
            store.save(&Doc::default()),
            Err(Error::Newer { found: 9, .. })
        ));
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn strict_types_reject_unknown_keys() {
        let dir = scratch("strict");
        let path = dir.join("settings.json");
        fs::write(
            &path,
            r#"{"schema_version":1,"name":"alpha","surprise":true}"#,
        )
        .unwrap();
        let store = Store::<Doc>::new(path.clone());
        assert!(matches!(store.load(), Err(Error::Json(_))));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn shared_doc_preserves_other_writers_keys() {
        let dir = scratch("shared");
        let path = dir.join("ui.json");
        fs::write(
            &path,
            r#"{"text_size":"small","palette":"blue","foreign":{"UPPER":1}}"#,
        )
        .unwrap();
        let doc = SharedDoc::new(path.clone());
        let map = doc
            .update(|map| {
                map.insert("text_size".into(), "large".into());
            })
            .unwrap();
        assert_eq!(map.get("palette").and_then(Value::as_str), Some("blue"));
        let saved: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(saved["text_size"], "large");
        assert_eq!(saved["palette"], "blue");
        assert_eq!(saved["foreign"]["UPPER"], 1);
        assert_eq!(saved[VERSION_KEY], 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn shared_doc_adds_a_version_without_touching_keys() {
        let dir = scratch("shared-version");
        let path = dir.join("ui.json");
        fs::write(&path, r#"{"text_size":"small","palette":"blue"}"#).unwrap();
        let doc = SharedDoc::new(path.clone());
        doc.ensure_version().unwrap();
        let saved: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(saved[VERSION_KEY], 1);
        assert_eq!(saved["text_size"], "small");
        assert_eq!(saved["palette"], "blue");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn locked_store_creates_a_sidecar_and_writes() {
        let dir = scratch("lock");
        let path = dir.join("settings.json");
        Store::<Doc>::new(path.clone())
            .with_lock()
            .save(&Doc::default())
            .unwrap();
        assert!(dir.join("settings.json.lock").is_file());
        assert!(path.is_file());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn snake_key_keeps_brands_and_splits_the_rest() {
        for (old, new) in [
            ("GitHubExecutable", "github_executable"),
            ("VSCodeProfile", "vscode_profile"),
            ("OpenCode", "opencode"),
            ("DeepSeek", "deepseek"),
            ("SnapIt", "snapit"),
            ("AzureDevOpsPat", "azure_devops_pat"),
            (
                "AutoAllowDefaultMinSeconds",
                "auto_allow_default_min_seconds",
            ),
            ("DeactivedScreens", "deactived_screens"),
            ("XButton1", "x_button1"),
            ("text_size", "text_size"),
        ] {
            assert_eq!(snake_key(old), new, "{old}");
        }
    }

    #[test]
    fn renaming_skips_opaque_map_keys_and_refuses_collisions() {
        let mut value = serde_json::json!({
            "Pins": {"ses_UPPER": true},
            "Workspaces": [{"Path": "/x"}]
        });
        let changed = rename_legacy_keys(&mut value, &["pins"]).unwrap();
        assert!(changed);
        assert_eq!(value["pins"]["ses_UPPER"], true);
        assert_eq!(value["workspaces"][0]["path"], "/x");

        let mut collision = serde_json::json!({"Name": 1, "name": 2});
        assert!(rename_legacy_keys(&mut collision, &[]).is_err());
    }
}
