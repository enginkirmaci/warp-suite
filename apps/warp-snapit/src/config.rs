//! Versioned JSON documents, local to SnapIt because this crate deliberately
//! does not link `warpui-kit-*`. The contract matches the suite's platform
//! store: a top-level `schema_version`, atomic replacement, and an importer at
//! the edge for the old .NET-shaped files.

use std::fmt;
use std::fs;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub const VERSION_KEY: &str = "schema_version";
pub const CURRENT_VERSION: u32 = 1;

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Json(serde_json::Error),
    NotAnObject,
    Unversioned,
    Older { found: u64, supported: u32 },
    Newer { found: u64, supported: u32 },
}

impl Error {
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
            Error::Newer { found, supported } => {
                write!(f, "schema_version {found} is newer than {supported}")
            }
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

pub struct Store<T> {
    path: PathBuf,
    version: u32,
    marker: PhantomData<fn() -> T>,
}

impl<T> Store<T>
where
    T: Serialize + DeserializeOwned + Default,
{
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            version: CURRENT_VERSION,
            marker: PhantomData,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> Result<T, Error> {
        match fs::read_to_string(&self.path) {
            Ok(text) => self.parse(&text),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
            Err(err) => Err(Error::Io(err)),
        }
    }

    pub fn save(&self, value: &T) -> Result<(), Error> {
        self.ensure_writable()?;
        let mut value = serde_json::to_value(value)?;
        let object = value.as_object_mut().ok_or(Error::NotAnObject)?;
        object.insert(VERSION_KEY.to_string(), Value::from(self.version));
        let text = serde_json::to_string_pretty(&value)?;
        crate::settings::write_atomically(&self.path, &text).map_err(Error::Io)
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
}

/// Normalize one old key to the new spelling.
pub fn snake_key(key: &str) -> String {
    if !key.bytes().any(|byte| byte.is_ascii_uppercase()) {
        return key.to_string();
    }
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

/// Rename every object key, skipping the values of opaque maps.
pub fn rename_keys(value: &mut Value, opaque: &[&str]) -> Result<bool, String> {
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

/// Rename one top-level key when the target is absent.
pub fn move_key(value: &mut Value, old: &str, new: &str) {
    let Some(object) = value.as_object_mut() else {
        return;
    };
    let Some(moved) = object.remove(old) else {
        return;
    };
    if !object.contains_key(new) {
        object.insert(new.to_string(), moved);
    }
}

/// Rewrite a string field's value in place, when present.
pub fn map_value(value: &mut Value, key: &str, map: &[(&str, &str)]) {
    let Some(object) = value.as_object_mut() else {
        return;
    };
    let Some(Value::String(current)) = object.get(key) else {
        return;
    };
    if let Some((_, next)) = map.iter().find(|(old, _)| *old == current) {
        object.insert(key.to_string(), Value::String((*next).to_string()));
    }
}

/// A transparent map whose keys are user data. SnapIt keeps one today for
/// screen layout ids; the newtype documents that no casing pass may touch it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct KeyMap<V>(pub std::collections::BTreeMap<String, V>);

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
    struct Doc {
        name: String,
    }

    fn scratch(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "snapit-config-{name}-{}-{nanos}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir.join("settings.json")
    }

    #[test]
    fn saves_and_loads_with_a_version() {
        let path = scratch("round-trip");
        let store = Store::<Doc>::new(path.clone());
        store
            .save(&Doc {
                name: "alpha".into(),
            })
            .unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"schema_version\": 1"), "{text}");
        assert_eq!(
            store.load().unwrap(),
            Doc {
                name: "alpha".into()
            }
        );
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn keys_and_values_translate() {
        assert_eq!(snake_key("DeactivedScreens"), "deactived_screens");
        assert_eq!(snake_key("LayoutLines"), "layout_lines");
        let mut value = serde_json::json!({
            "ScreensLayouts": {"\\\\.\\DISPLAY1": "guid"},
            "DeactivedScreens": ["\\\\.\\DISPLAY2"],
            "MouseButton": "Left"
        });
        rename_keys(&mut value, &["screens_layouts"]).unwrap();
        map_value(
            &mut value,
            "mouse_button",
            &[("Left", "left"), ("Right", "right")],
        );
        assert_eq!(value["screens_layouts"]["\\\\.\\DISPLAY1"], "guid");
        assert_eq!(value["deactived_screens"][0], "\\\\.\\DISPLAY2");
        assert_eq!(value["mouse_button"], "left");
    }
}
