//! SnapIt settings and layout files.
//!
//! The native files are versioned snake_case. The importer still accepts the
//! old .NET shape (`%USERPROFILE%\.warp\snapit\Settings.json`) with its
//! quirks: numbers may be strings, points/sizes are `"x,y"` strings, colours
//! are `"#AARRGGBB"`, and `AreaPadding`/`BorderThickness` may be a number or a
//! `Thickness`-style string.

use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use serde::de::{self, Error as _, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::config;
use crate::geometry::{Point, Size, SplitDirection};
use crate::log;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MouseButton {
    #[default]
    Left,
    Right,
    Middle,
    XButton1,
    XButton2,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HoldKey {
    #[default]
    Control,
    Alt,
    Shift,
    Win,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HoldKeyBehaviour {
    #[default]
    HoldToEnable,
    HoldToDisable,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchRule {
    #[default]
    Contains,
    Exact,
    Wildcard,
}

// ---------------------------------------------------------------------------
// Colour
// ---------------------------------------------------------------------------

/// An ARGB colour. The .NET app stores these as `"#AARRGGBB"`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color {
    pub a: u8,
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub const fn new(a: u8, r: u8, g: u8, b: u8) -> Self {
        Self { a, r, g, b }
    }

    /// Multiplies the alpha channel by `opacity` (the theme's window opacity).
    pub fn with_opacity(self, opacity: f64) -> Self {
        let a = (self.a as f64 * opacity).clamp(0.0, 255.0).round() as u8;
        Self { a, ..self }
    }

    fn parse(value: &str) -> Option<Self> {
        let hex = value.trim().trim_start_matches('#');
        // non-ASCII can never be hex, and the byte slicing below would panic
        // inside a multi-byte char — a user-edited "€aaaaa" must not abort
        // startup inside serde
        if !hex.is_ascii() {
            return None;
        }
        let (a, r, g, b) = match hex.len() {
            8 => (
                u8::from_str_radix(&hex[0..2], 16).ok()?,
                u8::from_str_radix(&hex[2..4], 16).ok()?,
                u8::from_str_radix(&hex[4..6], 16).ok()?,
                u8::from_str_radix(&hex[6..8], 16).ok()?,
            ),
            6 => (
                255,
                u8::from_str_radix(&hex[0..2], 16).ok()?,
                u8::from_str_radix(&hex[2..4], 16).ok()?,
                u8::from_str_radix(&hex[4..6], 16).ok()?,
            ),
            _ => return None,
        };
        Some(Self::new(a, r, g, b))
    }

    fn to_hex(self) -> String {
        format!("#{:02X}{:02X}{:02X}{:02X}", self.a, self.r, self.g, self.b)
    }
}

impl Default for Color {
    fn default() -> Self {
        Self::new(0, 0, 0, 0)
    }
}

impl Serialize for Color {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ColorVisitor;

        impl<'de> Visitor<'de> for ColorVisitor {
            type Value = Color;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a colour string like \"#AARRGGBB\" or [a, r, g, b]")
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<Color, E> {
                Color::parse(value).ok_or_else(|| E::custom(format!("invalid colour: {value}")))
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Color, A::Error> {
                let a = seq.next_element::<u8>()?.unwrap_or(255);
                let r = seq.next_element::<u8>()?.unwrap_or(0);
                let g = seq.next_element::<u8>()?.unwrap_or(0);
                let b = seq.next_element::<u8>()?.unwrap_or(0);
                Ok(Color::new(a, r, g, b))
            }
        }

        deserializer.deserialize_any(ColorVisitor)
    }
}

// ---------------------------------------------------------------------------
// Point / Size
// ---------------------------------------------------------------------------

fn pair_from_str<E: de::Error>(value: &str) -> Result<(f32, f32), E> {
    let mut parts = value.split(',');
    let first = parts
        .next()
        .ok_or_else(|| E::custom("expected \"x,y\""))?
        .trim()
        .parse::<f32>()
        .map_err(|_| E::custom(format!("invalid number in \"{value}\"")))?;
    let second = parts
        .next()
        .ok_or_else(|| E::custom("expected \"x,y\""))?
        .trim()
        .parse::<f32>()
        .map_err(|_| E::custom(format!("invalid number in \"{value}\"")))?;
    Ok((first, second))
}

impl Serialize for Point {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&format!("{},{}", self.x, self.y))
    }
}

impl<'de> Deserialize<'de> for Point {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct PointVisitor;

        impl<'de> Visitor<'de> for PointVisitor {
            type Value = Point;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a point string like \"10,20\"")
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<Point, E> {
                let (x, y) = pair_from_str(value)?;
                Ok(Point::new(x, y))
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Point, A::Error> {
                let x = seq.next_element::<f32>()?.unwrap_or(0.0);
                let y = seq.next_element::<f32>()?.unwrap_or(0.0);
                Ok(Point::new(x, y))
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Point, A::Error> {
                let mut x = 0.0;
                let mut y = 0.0;
                while let Some(key) = map.next_key::<String>()? {
                    match key.to_ascii_lowercase().as_str() {
                        "x" => x = map.next_value()?,
                        "y" => y = map.next_value()?,
                        _ => {
                            let _ = map.next_value::<de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(Point::new(x, y))
            }
        }

        deserializer.deserialize_any(PointVisitor)
    }
}

impl Serialize for Size {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&format!("{},{}", self.width, self.height))
    }
}

impl<'de> Deserialize<'de> for Size {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct SizeVisitor;

        impl<'de> Visitor<'de> for SizeVisitor {
            type Value = Size;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a size string like \"100,200\"")
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<Size, E> {
                let (width, height) = pair_from_str(value)?;
                Ok(Size::new(width, height))
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Size, A::Error> {
                let width = seq.next_element::<f32>()?.unwrap_or(0.0);
                let height = seq.next_element::<f32>()?.unwrap_or(0.0);
                Ok(Size::new(width, height))
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Size, A::Error> {
                let mut width = 0.0;
                let mut height = 0.0;
                while let Some(key) = map.next_key::<String>()? {
                    match key.to_ascii_lowercase().as_str() {
                        "width" => width = map.next_value()?,
                        "height" => height = map.next_value()?,
                        _ => {
                            let _ = map.next_value::<de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(Size::new(width, height))
            }
        }

        deserializer.deserialize_any(SizeVisitor)
    }
}

// ---------------------------------------------------------------------------
// Flexible numbers
// ---------------------------------------------------------------------------

/// Accepts a JSON number or a numeric string (System.Text.Json writes numbers
/// as strings in some SnapIt versions).
pub fn number_from_value<E: de::Error>(value: &serde_json::Value) -> Result<f64, E> {
    match value {
        serde_json::Value::Number(n) => n.as_f64().ok_or_else(|| E::custom("number out of range")),
        serde_json::Value::String(s) => s
            .trim()
            .parse::<f64>()
            .map_err(|_| E::custom(format!("invalid number: {s}"))),
        other => Err(E::custom(format!("expected a number, got {other}"))),
    }
}

fn de_f64_flexible<'de, D: Deserializer<'de>>(deserializer: D) -> Result<f64, D::Error> {
    let value = serde_json::Value::deserialize(deserializer)?;
    number_from_value(&value)
}

fn de_i32_flexible<'de, D: Deserializer<'de>>(deserializer: D) -> Result<i32, D::Error> {
    let value = serde_json::Value::deserialize(deserializer)?;
    number_from_value(&value).map(|n| n.round() as i32)
}

/// `Thickness` is written as `1`, `"1"`, `"1,2"` or `"1,2,3,4"`; only the
/// uniform value matters to SnapIt.
fn de_thickness<'de, D: Deserializer<'de>>(deserializer: D) -> Result<f64, D::Error> {
    let value = serde_json::Value::deserialize(deserializer)?;
    match &value {
        serde_json::Value::String(s) => {
            let first = s.split(',').next().unwrap_or(s).trim();
            first
                .parse::<f64>()
                .map_err(|_| D::Error::custom(format!("invalid thickness: {s}")))
        }
        _ => number_from_value(&value),
    }
}

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

/// `SnapAreaTheme` from the .NET app.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(default, rename_all = "snake_case")]
pub struct Theme {
    pub highlight_color: Color,
    pub overlay_color: Color,
    pub border_color: Color,
    #[serde(deserialize_with = "de_thickness")]
    pub border_thickness: f64,
    #[serde(deserialize_with = "de_f64_flexible")]
    pub opacity: f64,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            // SnapAreaTheme's constructor defaults.
            highlight_color: Color::new(200, 0, 0, 0),
            overlay_color: Color::new(150, 0, 0, 255),
            border_color: Color::new(200, 150, 150, 150),
            border_thickness: 1.0,
            opacity: 0.2,
        }
    }
}

/// `settings.json`.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, rename_all = "snake_case")]
pub struct Settings {
    pub enable_mouse: bool,
    pub drag_by_title: bool,
    pub mouse_button: MouseButton,
    #[serde(deserialize_with = "de_i32_flexible")]
    pub mouse_drag_delay: i32,
    pub enable_hold_key: bool,
    pub hold_key: HoldKey,
    pub hold_key_behaviour: HoldKeyBehaviour,
    pub disable_for_fullscreen: bool,
    pub disable_for_modal: bool,
    pub enable_automatic_window_cornering: bool,
    /// Kept for the old .NET setting and for a faithful round trip; the Rust
    /// engine reads it but the elevation prompt itself is the OS's.
    pub run_as_admin: bool,
    pub screens_layouts: HashMap<String, String>,
    pub deactivated_screens: Vec<String>,
    pub mouse_hover_animation: bool,
    pub theme: Theme,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            enable_mouse: true,
            drag_by_title: true,
            mouse_button: MouseButton::Left,
            mouse_drag_delay: 20,
            enable_hold_key: false,
            hold_key: HoldKey::Control,
            hold_key_behaviour: HoldKeyBehaviour::HoldToEnable,
            disable_for_fullscreen: true,
            disable_for_modal: true,
            enable_automatic_window_cornering: false,
            run_as_admin: false,
            screens_layouts: HashMap::new(),
            deactivated_screens: Vec::new(),
            mouse_hover_animation: false,
            theme: Theme::default(),
        }
    }
}

// ---------------------------------------------------------------------------
// Layouts
// ---------------------------------------------------------------------------

/// A `LayoutLines` entry.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, rename_all = "snake_case")]
pub struct LayoutLine {
    pub point: Point,
    pub size: Size,
    pub split_direction: SplitDirection,
}

impl Default for LayoutLine {
    fn default() -> Self {
        Self {
            point: Point::default(),
            size: Size::default(),
            split_direction: SplitDirection::Vertical,
        }
    }
}

/// A `LayoutOverlays` entry; `mini_overlay` is where the normal-state preview
/// is drawn, while `point`/`size` describe the merged zone itself.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "snake_case")]
pub struct LayoutOverlay {
    pub point: Point,
    pub size: Size,
    pub mini_overlay: Option<MiniOverlay>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "snake_case")]
pub struct MiniOverlay {
    pub point: Point,
    pub size: Size,
}

/// A layout file from `layouts/*.json`.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "snake_case")]
pub struct Layout {
    pub guid: String,
    pub name: String,
    pub size: Size,
    pub layout_lines: Vec<LayoutLine>,
    pub layout_overlays: Vec<LayoutOverlay>,
    #[serde(deserialize_with = "de_thickness")]
    pub area_padding: f64,
}

impl Layout {
    pub fn scaled_lines(&self, work_width: i32, work_height: i32) -> (f32, f32) {
        let fx = if self.size.width > 0.0 {
            work_width as f32 / self.size.width
        } else {
            1.0
        };
        let fy = if self.size.height > 0.0 {
            work_height as f32 / self.size.height
        } else {
            1.0
        };
        (fx, fy)
    }
}

// ---------------------------------------------------------------------------
// Excluded applications
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "snake_case")]
pub struct ExcludedApplication {
    pub keyword: String,
    pub match_rule: MatchRule,
    pub mouse: bool,
}

impl ExcludedApplication {
    fn new(keyword: &str, match_rule: MatchRule, mouse: bool) -> Self {
        Self {
            keyword: keyword.to_string(),
            match_rule,
            mouse,
        }
    }

    /// Mirrors `WindowsService.IsExcludedApplication`: `Contains` is
    /// case-insensitive, `Exact` is case-sensitive, and `Wildcard` is the
    /// unanchored, case-insensitive regex the .NET code builds from the glob.
    pub fn matches(&self, title: &str) -> bool {
        if self.keyword.trim().is_empty() {
            return false;
        }

        match self.match_rule {
            MatchRule::Contains => title.to_lowercase().contains(&self.keyword.to_lowercase()),
            MatchRule::Exact => title == self.keyword,
            MatchRule::Wildcard => wildcard_match(&self.keyword, title),
        }
    }
}

/// True when any mouse exclusion rule matches. The .NET app builds this list
/// once at startup (`Applications.Where(i => i.Mouse)`).
pub fn is_excluded(rules: &[ExcludedApplication], title: &str) -> bool {
    rules.iter().any(|rule| rule.mouse && rule.matches(title))
}

/// Glob matcher with `*` and `?`, case-insensitive. The pattern is implicitly
/// wrapped in `*...*` because `Regex.IsMatch` is unanchored.
pub fn wildcard_match(pattern: &str, input: &str) -> bool {
    let pattern: Vec<char> = format!("*{pattern}*").to_lowercase().chars().collect();
    let input: Vec<char> = input.to_lowercase().chars().collect();

    let mut pattern_index = 0;
    let mut input_index = 0;
    let mut star: Option<usize> = None;
    let mut star_input = 0;

    while input_index < input.len() {
        if pattern_index < pattern.len()
            && (pattern[pattern_index] == '?' || pattern[pattern_index] == input[input_index])
        {
            pattern_index += 1;
            input_index += 1;
        } else if pattern_index < pattern.len() && pattern[pattern_index] == '*' {
            star = Some(pattern_index);
            star_input = input_index;
            pattern_index += 1;
        } else if let Some(star_index) = star {
            pattern_index = star_index + 1;
            star_input += 1;
            input_index = star_input;
        } else {
            return false;
        }
    }

    while pattern_index < pattern.len() && pattern[pattern_index] == '*' {
        pattern_index += 1;
    }

    pattern_index == pattern.len()
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "snake_case")]
pub struct ExcludedApplicationSettings {
    pub applications: Vec<ExcludedApplication>,
}

// ---------------------------------------------------------------------------
// Paths and loading
// ---------------------------------------------------------------------------

/// `%USERPROFILE%\.warp` (falls back to `$HOME` for tests on Linux).
pub fn user_data_root() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".warp")
}

/// `%USERPROFILE%\.warp\snapit` — shared with Warp Station.
pub fn snapit_root() -> PathBuf {
    user_data_root().join("snapit")
}

/// The native files this build writes.
pub fn settings_file() -> PathBuf {
    snapit_root().join("settings.json")
}

pub fn excluded_applications_file() -> PathBuf {
    snapit_root().join("excluded_applications.json")
}

pub fn layouts_dir() -> PathBuf {
    snapit_root().join("layouts")
}

/// The .NET-shaped files an upgrade imports once.
fn legacy_settings_file() -> PathBuf {
    snapit_root().join("Settings.json")
}

fn legacy_excluded_applications_file() -> PathBuf {
    snapit_root().join("ExcludedApplicationSettings.json")
}

fn same_file(left: &Path, right: &Path) -> bool {
    left.exists() && right.exists() && fs::canonicalize(left).ok() == fs::canonicalize(right).ok()
}

fn retire_legacy(legacy: &Path, current: &Path) {
    if legacy.exists() && !same_file(legacy, current) {
        let _ = fs::remove_file(legacy);
    }
}

/// `<exe dir>\settings\snapit` — shipped defaults used to seed the user folder.
pub fn install_defaults_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("settings")
        .join("snapit")
}

fn copy_missing(source: &Path, destination: &Path) {
    if destination.exists() {
        return;
    }
    if let Some(parent) = destination.parent()
        && let Err(err) = fs::create_dir_all(parent)
    {
        log::line(&format!("could not create {}: {err}", parent.display()));
    }
    if let Err(err) = fs::copy(source, destination) {
        // without this the app runs on built-in defaults with no trace of why
        log::line(&format!("could not seed {}: {err}", destination.display()));
    }
}

/// One-time seed from the shipped defaults; existing user files win.
pub fn seed_from_install_defaults() {
    let source = install_defaults_dir();
    if !source.is_dir() {
        return;
    }

    if let Ok(entries) = fs::read_dir(&source) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "json")
                && let Some(name) = path.file_name()
            {
                // A .NET file is the user's real settings; the importer must
                // see it instead of a freshly seeded native default.
                let legacy = match name.to_str() {
                    Some("settings.json") => Some(legacy_settings_file()),
                    Some("excluded_applications.json") => Some(legacy_excluded_applications_file()),
                    _ => None,
                };
                if legacy.is_some_and(|path| path.exists()) {
                    continue;
                }
                copy_missing(&path, &snapit_root().join(name));
            }
        }
    }

    let source_layouts = source.join("layouts");
    if let Ok(entries) = fs::read_dir(source_layouts) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "json")
                && let Some(name) = path.file_name()
            {
                copy_missing(&path, &snapit_root().join("layouts").join(name));
            }
        }
    }
}

fn read_legacy_value(path: &Path) -> Option<serde_json::Value> {
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn import_settings(path: &Path) -> Option<Settings> {
    let mut value = read_legacy_value(path)?;
    config::rename_keys(&mut value, &["screens_layouts"]).ok()?;
    config::move_key(&mut value, "deactived_screens", "deactivated_screens");
    config::map_value(
        &mut value,
        "mouse_button",
        &[
            ("Left", "left"),
            ("Right", "right"),
            ("Middle", "middle"),
            ("XButton1", "x_button1"),
            ("XButton2", "x_button2"),
        ],
    );
    config::map_value(
        &mut value,
        "hold_key",
        &[
            ("Control", "control"),
            ("Alt", "alt"),
            ("Shift", "shift"),
            ("Win", "win"),
        ],
    );
    config::map_value(
        &mut value,
        "hold_key_behaviour",
        &[
            ("HoldToEnable", "hold_to_enable"),
            ("HoldToDisable", "hold_to_disable"),
        ],
    );
    serde_json::from_value(value).ok()
}

fn import_excluded(path: &Path) -> Option<ExcludedApplicationSettings> {
    let mut value = read_legacy_value(path)?;
    config::rename_keys(&mut value, &[]).ok()?;
    if let Some(applications) = value
        .get_mut("applications")
        .and_then(serde_json::Value::as_array_mut)
    {
        for application in applications {
            config::map_value(
                application,
                "match_rule",
                &[
                    ("Contains", "contains"),
                    ("Exact", "exact"),
                    ("Wildcard", "wildcard"),
                ],
            );
        }
    }
    serde_json::from_value(value).ok()
}

fn import_layout(path: &Path) -> Option<Layout> {
    let mut value = read_legacy_value(path)?;
    config::rename_keys(&mut value, &[]).ok()?;
    if let Some(lines) = value
        .get_mut("layout_lines")
        .and_then(serde_json::Value::as_array_mut)
    {
        for line in lines {
            config::map_value(
                line,
                "split_direction",
                &[("Vertical", "vertical"), ("Horizontal", "horizontal")],
            );
        }
    }
    serde_json::from_value(value).ok()
}

/// Replace `path` with `contents` atomically: a uniquely named temp file next
/// to the target, then rename over it (atomic on the same volume), so a crash
/// mid-write never leaves a truncated file behind. Both Warp Station and this
/// companion read these files; the notes app's writer is the family's
/// reference, duplicated here rather than reached across apps.
pub fn write_atomically(path: &Path, contents: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent()
        && !dir.as_os_str().is_empty()
    {
        fs::create_dir_all(dir)?;
    }
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".into());
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = path.with_file_name(format!("{file_name}.{nanos:x}-{}.tmp", std::process::id()));
    fs::write(&tmp, contents)?;
    if let Err(err) = fs::rename(&tmp, path) {
        // a destination held open without FILE_SHARE_DELETE (an editor) —
        // drop it and retry once, keeping the temp file (the only surviving
        // copy) if even that fails
        if fs::remove_file(path).is_ok() && fs::rename(&tmp, path).is_ok() {
            return Ok(());
        }
        return Err(err);
    }
    Ok(())
}

/// The engine's one live copy of `settings.json`. Reads deref to [`Settings`];
/// mutations go through [`update`](Self::update), which applies and persists
/// as one step — a mutation cannot forget its write, and a crash mid-write
/// cannot truncate the file both implementations read. The settings change
/// by user action only (rare clicks), so the write rides the mutation
/// straight through instead of a debounce timer.
pub struct SettingsStore {
    path: PathBuf,
    settings: Settings,
}

impl SettingsStore {
    /// Wraps an already-loaded settings value at the given path — the engine
    /// (Windows-only) takes the `Config`'s read, and the tests aim the store
    /// at temp files.
    #[allow(dead_code)]
    pub(crate) fn at(path: PathBuf, settings: Settings) -> Self {
        Self { path, settings }
    }

    /// Applies `mutate` to the live settings and writes the file before
    /// returning. An unwritable file leaves the in-memory value applied and
    /// the failure in the log: the engine keeps working this session.
    pub fn update(&mut self, mutate: impl FnOnce(&mut Settings)) {
        mutate(&mut self.settings);
        if let Err(err) = config::Store::<Settings>::new(self.path.clone()).save(&self.settings) {
            log::line(&format!("could not write {}: {err}", self.path.display()));
        }
    }
}

impl std::ops::Deref for SettingsStore {
    type Target = Settings;

    fn deref(&self) -> &Settings {
        &self.settings
    }
}

/// Reads `settings.json`; an old `Settings.json` is imported once, then
/// retired. A missing file gets the defaults written.
pub fn load_settings() -> Settings {
    let path = settings_file();
    let legacy = legacy_settings_file();
    let store = config::Store::<Settings>::new(path.clone());

    if !path.exists() {
        if legacy.exists() {
            if let Some(settings) = import_settings(&legacy)
                && store.save(&settings).is_ok()
            {
                retire_legacy(&legacy, &path);
                return settings;
            }
            log::line(&format!(
                "could not import {}; keeping it for a manual pass",
                legacy.display()
            ));
            return Settings::default();
        }
        let settings = Settings::default();
        let _ = store.save(&settings);
        return settings;
    }

    match store.load() {
        Ok(settings) => settings,
        Err(err) if err.needs_import() => {
            let source = if legacy.exists() && !same_file(&legacy, &path) {
                legacy.clone()
            } else {
                path.clone()
            };
            let Some(settings) = import_settings(&source) else {
                log::line(&format!(
                    "could not import {}; keeping it for a manual pass",
                    source.display()
                ));
                return Settings::default();
            };
            if store.save(&settings).is_ok() {
                retire_legacy(&legacy, &path);
            }
            settings
        }
        Err(err) => {
            log::line(&format!("could not read {}: {err}", path.display()));
            Settings::default()
        }
    }
}

pub fn load_excluded_applications() -> Vec<ExcludedApplication> {
    let path = excluded_applications_file();
    let legacy = legacy_excluded_applications_file();
    let store = config::Store::<ExcludedApplicationSettings>::new(path.clone());

    if !path.exists() {
        if legacy.exists() {
            match import_excluded(&legacy) {
                Some(settings) if store.save(&settings).is_ok() => {
                    retire_legacy(&legacy, &path);
                }
                _ => log::line(&format!(
                    "could not import {}; keeping it for a manual pass",
                    legacy.display()
                )),
            }
        } else {
            let _ = store.save(&ExcludedApplicationSettings::default());
        }
    } else {
        match store.load() {
            Ok(_) => {}
            Err(err) if err.needs_import() => {
                let source = if legacy.exists() && !same_file(&legacy, &path) {
                    legacy.clone()
                } else {
                    path.clone()
                };
                match import_excluded(&source) {
                    Some(settings) => {
                        if store.save(&settings).is_ok() {
                            retire_legacy(&legacy, &path);
                        }
                    }
                    None => log::line(&format!(
                        "could not import {}; keeping it for a manual pass",
                        source.display()
                    )),
                }
            }
            Err(err) => log::line(&format!("could not read {}: {err}", path.display())),
        }
    }

    let mut applications = store
        .load()
        .map(|settings| settings.applications)
        .unwrap_or_default();

    // Match the .NET SettingService: always exclude the desktop shell.
    if !applications
        .iter()
        .any(|app| app.keyword.eq_ignore_ascii_case("Program Manager"))
    {
        applications.push(ExcludedApplication::new(
            "Program Manager",
            MatchRule::Contains,
            true,
        ));
    }

    applications
}

/// Loads every parseable `layouts/*.json`, sorted by name. An old
/// PascalCase layout is imported in place and rewritten versioned.
pub fn load_layouts() -> Vec<Layout> {
    let folder = layouts_dir();
    let mut layouts = Vec::new();

    if let Ok(entries) = fs::read_dir(folder) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "json") {
                continue;
            }
            let store = config::Store::<Layout>::new(path.clone());
            match store.load() {
                Ok(layout) => layouts.push(layout),
                Err(err) if err.needs_import() => {
                    let Some(layout) = import_layout(&path) else {
                        log::line(&format!("could not import {}", path.display()));
                        continue;
                    };
                    let _ = store.save(&layout);
                    layouts.push(layout);
                }
                Err(err) => log::line(&format!("could not read {}: {err}", path.display())),
            }
        }
    }

    layouts.sort_by(|a, b| a.name.cmp(&b.name));
    layouts
}

/// Everything the engine needs from disk.
#[derive(Clone, Debug, Default)]
pub struct Config {
    pub settings: Settings,
    pub excluded_applications: Vec<ExcludedApplication>,
    pub layouts: Vec<Layout>,
}

impl Config {
    pub fn load() -> Self {
        seed_from_install_defaults();
        Self {
            settings: load_settings(),
            excluded_applications: load_excluded_applications(),
            layouts: load_layouts(),
        }
    }

    /// Resolves the layout for a monitor. `keys` are the device identifiers
    /// (`\\.\DISPLAY1` and the display-interface path); both are checked
    /// case-insensitively because the two implementations name screens
    /// slightly differently.
    pub fn layout_for(&self, keys: &[String]) -> Option<&Layout> {
        layout_for(&self.settings, &self.layouts, keys)
    }

    pub fn is_deactivated(&self, keys: &[String]) -> bool {
        is_deactivated(&self.settings, keys)
    }
}

pub fn layout_for<'a>(
    settings: &Settings,
    layouts: &'a [Layout],
    keys: &[String],
) -> Option<&'a Layout> {
    for key in keys {
        let Some(guid) = settings
            .screens_layouts
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(key.trim()))
            .map(|(_, guid)| guid.trim())
        else {
            continue;
        };

        // An empty mapping means "first layout"; a mapping to a missing
        // layout means "no layout", matching SettingService.GetSnapScreens.
        if guid.is_empty() {
            return layouts.first();
        }
        return layouts
            .iter()
            .find(|layout| layout.guid.eq_ignore_ascii_case(guid));
    }

    layouts.first()
}

pub fn is_deactivated(settings: &Settings, keys: &[String]) -> bool {
    settings.settings_deactivated(keys)
}

impl Settings {
    fn settings_deactivated(&self, keys: &[String]) -> bool {
        self.deactivated_screens.iter().any(|screen| {
            keys.iter()
                .any(|key| screen.eq_ignore_ascii_case(key.trim()))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_parse_takes_ascii_hex_only() {
        // 8 *bytes* of a multi-byte string must not slice inside a char
        assert_eq!(
            Color::parse("#80FFFFFF"),
            Some(Color::new(0x80, 0xFF, 0xFF, 0xFF))
        );
        assert_eq!(
            Color::parse("FFFFFF"),
            Some(Color::new(255, 0xFF, 0xFF, 0xFF))
        );
        assert_eq!(
            Color::parse("  #80ffffff "),
            Some(Color::new(0x80, 0xFF, 0xFF, 0xFF))
        );
        assert_eq!(Color::parse("€aaaaa"), None);
        assert_eq!(Color::parse("RRGG"), None);
        assert_eq!(Color::parse(""), None);
    }

    #[test]
    fn parses_shipped_settings() {
        let text = include_str!("../packaging/windows/settings/snapit/settings.json");
        let settings: Settings = serde_json::from_str(text).expect("settings parse");
        assert!(settings.enable_mouse);
        assert!(!settings.drag_by_title);
        assert_eq!(settings.mouse_button, MouseButton::Middle);
        assert_eq!(settings.mouse_drag_delay, 20);
        assert_eq!(settings.theme.opacity, 0.6);
        assert_eq!(
            settings.theme.overlay_color,
            Color::new(0x32, 0xFF, 0xFF, 0xFF)
        );
    }

    #[test]
    fn parses_shipped_layout() {
        let text =
            include_str!("../packaging/windows/settings/snapit/layouts/3PartHorizontal.json");
        let layout: Layout = serde_json::from_str(text).expect("layout parse");
        assert_eq!(layout.name, "3 Part Horizontal");
        assert_eq!(layout.size.width, 2752.0);
        assert_eq!(layout.layout_lines.len(), 4);
        assert_eq!(layout.layout_overlays.len(), 6);
        assert_eq!(
            layout.layout_lines[0].split_direction,
            SplitDirection::Vertical
        );
        assert!(layout.layout_overlays[0].mini_overlay.is_some());
    }

    #[test]
    fn writes_numbers_as_numbers_and_colours_as_hex() {
        let settings = Settings::default();
        let json = serde_json::to_string(&settings).expect("serialize");
        assert!(json.contains("\"mouse_drag_delay\":20"));
        assert!(json.contains("\"border_thickness\":1.0"));
        assert!(json.contains("\"highlight_color\":\"#C8000000\""));
    }

    #[test]
    fn exclusion_rules_match_like_the_dotnet_service() {
        let contains = ExcludedApplication::new("chrome", MatchRule::Contains, true);
        assert!(contains.matches("Google Chrome"));
        assert!(!contains.matches("Firefox"));

        let exact = ExcludedApplication::new("Google Chrome", MatchRule::Exact, true);
        assert!(exact.matches("Google Chrome"));
        assert!(!exact.matches("google chrome"));

        let wildcard = ExcludedApplication::new("chrome*tab", MatchRule::Wildcard, true);
        assert!(wildcard.matches("Google Chrome — New Tab"));
        assert!(!wildcard.matches("Google Chrome"));

        // Regex.IsMatch is unanchored, so a bare glob matches a substring.
        let bare = ExcludedApplication::new("taskmgr", MatchRule::Wildcard, true);
        assert!(bare.matches("Windows Taskmgr Window"));

        assert!(is_excluded(
            std::slice::from_ref(&contains),
            "Google Chrome"
        ));
        assert!(!is_excluded(std::slice::from_ref(&contains), ""));

        let keyboard_only = ExcludedApplication::new("chrome", MatchRule::Contains, false);
        assert!(!is_excluded(
            std::slice::from_ref(&keyboard_only),
            "Google Chrome"
        ));
    }

    fn layout(guid: &str, name: &str) -> Layout {
        Layout {
            guid: guid.to_string(),
            name: name.to_string(),
            ..Layout::default()
        }
    }

    #[test]
    fn layout_lookup_matches_dotnet_semantics() {
        let layouts = vec![layout("aaa", "First"), layout("bbb", "Second")];
        let mut settings = Settings::default();
        let display = "\\\\.\\DISPLAY1".to_string();

        // No mapping at all: the first layout is used.
        assert_eq!(
            layout_for(&settings, &layouts, std::slice::from_ref(&display))
                .map(|layout| layout.name.as_str()),
            Some("First")
        );

        // An empty mapping also means "first layout".
        settings
            .screens_layouts
            .insert(display.clone(), String::new());
        assert_eq!(
            layout_for(&settings, &layouts, std::slice::from_ref(&display))
                .map(|layout| layout.name.as_str()),
            Some("First")
        );

        // A mapping to an existing GUID selects it (case-insensitively).
        settings
            .screens_layouts
            .insert(display.clone(), "BBB".into());
        assert_eq!(
            layout_for(&settings, &layouts, std::slice::from_ref(&display))
                .map(|layout| layout.name.as_str()),
            Some("Second")
        );

        // A mapping to a missing layout yields no layout, not the first one.
        settings
            .screens_layouts
            .insert(display, "does-not-exist".into());
        assert!(layout_for(&settings, &layouts, &["\\\\.\\DISPLAY1".to_string()]).is_none());

        // Unknown monitors fall back to the first layout.
        assert_eq!(
            layout_for(&settings, &layouts, &["\\\\.\\DISPLAY9".to_string()])
                .map(|layout| layout.name.as_str()),
            Some("First")
        );
    }

    #[test]
    fn layout_lookup_checks_every_device_key() {
        let layouts = vec![layout("aaa", "First"), layout("bbb", "Second")];
        let mut settings = Settings::default();
        settings
            .screens_layouts
            .insert("\\\\?\\DISPLAY#DEL123#1".into(), "bbb".into());

        let keys = vec![
            "\\\\.\\DISPLAY1".to_string(),
            "\\\\?\\DISPLAY#DEL123#1".to_string(),
        ];
        assert_eq!(
            layout_for(&settings, &layouts, &keys).map(|layout| layout.name.as_str()),
            Some("Second")
        );
    }

    #[test]
    fn deactivated_screens_match_case_insensitively() {
        let mut settings = Settings::default();
        settings
            .deactivated_screens
            .push("\\\\.\\DISPLAY2".to_string());

        assert!(is_deactivated(&settings, &["\\\\.\\display2".to_string()]));
        assert!(!is_deactivated(&settings, &["\\\\.\\DISPLAY1".to_string()]));
    }

    #[test]
    fn atomic_write_replaces_and_leaves_no_temp_files() {
        let dir = std::env::temp_dir().join(format!("snapit-atomic-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let target = dir.join("note.json");
        write_atomically(&target, "first").unwrap();
        write_atomically(&target, "second").unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "second");
        let leftovers: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "no temp files left behind: {leftovers:?}"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn settings_store_updates_persist_and_reload() {
        let dir = std::env::temp_dir().join(format!("snapit-store-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");

        let mut store = SettingsStore::at(path.clone(), Settings::default());
        assert!(store.enable_mouse, "the store reads like the settings");
        store.update(|settings| {
            settings.enable_mouse = false;
            settings.mouse_button = MouseButton::Middle;
        });

        // the mutation is on disk before update returns, as versioned
        // snake_case JSON
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"enable_mouse\": false"), "{text}");
        assert!(text.contains("\"schema_version\": 1"), "{text}");
        let reread = SettingsStore::at(path, serde_json::from_str(&text).unwrap());
        assert!(!reread.enable_mouse);
        assert_eq!(reread.mouse_button, MouseButton::Middle);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_old_settings_file_imports_once() {
        let dir = std::env::temp_dir().join(format!("snapit-import-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("Settings.json");
        fs::write(
            &path,
            r##"{
                "EnableMouse": false,
                "MouseButton": "XButton1",
                "ScreensLayouts": {"\\\\.\\DISPLAY1": "guid"},
                "DeactivedScreens": ["\\\\.\\DISPLAY2"],
                "RunAsAdmin": true,
                "Theme": {"HighlightColor": "#C8000000", "BorderThickness": "1"}
            }"##,
        )
        .unwrap();

        let settings = import_settings(&path).expect("imports");
        assert!(!settings.enable_mouse);
        assert_eq!(settings.mouse_button, MouseButton::XButton1);
        assert_eq!(
            settings.screens_layouts.get("\\\\.\\DISPLAY1"),
            Some(&"guid".to_string())
        );
        assert_eq!(settings.deactivated_screens, vec!["\\\\.\\DISPLAY2"]);
        assert!(settings.run_as_admin);
        assert_eq!(settings.theme.border_thickness, 1.0);

        let json = serde_json::to_string(&settings).unwrap();
        assert!(json.contains("\"deactivated_screens\""));
        assert!(json.contains("\"x_button1\""));
        assert!(!json.contains("DeactivedScreens"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_old_layout_and_exclusions_import() {
        let dir = std::env::temp_dir().join(format!("snapit-import-layout-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let layout_path = dir.join("layout.json");
        fs::write(
            &layout_path,
            r#"{"Name": "x", "Size": "100,100",
                "LayoutLines": [{"Point": "1,2", "Size": "0,3", "SplitDirection": "Vertical"}]}"#,
        )
        .unwrap();
        let layout = import_layout(&layout_path).expect("layout imports");
        assert_eq!(layout.name, "x");
        assert_eq!(
            layout.layout_lines[0].split_direction,
            SplitDirection::Vertical
        );
        let json = serde_json::to_string(&layout).unwrap();
        assert!(json.contains("\"layout_lines\""));
        assert!(json.contains("\"vertical\""));

        let excluded_path = dir.join("excluded.json");
        fs::write(
            &excluded_path,
            r#"{"Applications": [{"Keyword": "chrome", "MatchRule": "Contains", "Mouse": true}]}"#,
        )
        .unwrap();
        let excluded = import_excluded(&excluded_path).expect("exclusions import");
        assert_eq!(excluded.applications[0].match_rule, MatchRule::Contains);
        let json = serde_json::to_string(&excluded).unwrap();
        assert!(json.contains("\"applications\""));
        assert!(json.contains("\"contains\""));
        let _ = fs::remove_dir_all(&dir);
    }
}
