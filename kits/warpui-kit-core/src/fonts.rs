//! The family's type: the bundled faces, font resolution and the text-size
//! token ladder.
//!
//! The family is set in Inter, with JetBrains Mono for code, paths and branches:
//! both have the tall x-height grayscale text needs on Linux. gpui loads font
//! files, not webfonts, so the TTFs ship inside this crate and are handed to the text system before
//! anything resolves: a machine without them installed still draws the family's
//! type. Naming a missing family makes gpui fall back per text run and skips
//! the kit's own probe (an explicitly-set family is used as-is), so each stack
//! still resolves once at startup against the real font list, with the
//! installed families behind the bundled one.

use std::borrow::Cow;

use gpui::App;

/// The bundled faces (SIL OFL, see `assets/fonts/OFL-*.txt`). The italics are not
/// optional: gpui's Linux text system draws an italic run upright when a family
/// has no italic face, it does not slant one.
const BUNDLED_FACES: [&[u8]; 9] = [
    include_bytes!("../assets/fonts/Inter-Regular.ttf"),
    include_bytes!("../assets/fonts/Inter-Italic.ttf"),
    include_bytes!("../assets/fonts/Inter-Medium.ttf"),
    include_bytes!("../assets/fonts/Inter-MediumItalic.ttf"),
    include_bytes!("../assets/fonts/Inter-SemiBold.ttf"),
    include_bytes!("../assets/fonts/Inter-SemiBoldItalic.ttf"),
    include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf"),
    include_bytes!("../assets/fonts/JetBrainsMono-Italic.ttf"),
    include_bytes!("../assets/fonts/JetBrainsMono-Medium.ttf"),
];

pub const UI_FONT_CANDIDATES: &[&str] = &[
    "Inter",
    "Adwaita Sans",
    "Segoe UI",
    "SF Pro Text",
    "Cantarell",
    "DejaVu Sans",
];
pub const CODE_FONT_CANDIDATES: &[&str] = &[
    "JetBrains Mono",
    "JetBrainsMono Nerd Font",
    "DejaVu Sans Mono",
    "Noto Sans Mono",
    "Liberation Mono",
    "Cascadia Mono",
    "Menlo",
    "Consolas",
    "Monaco",
];
/// Virtual family gpui always resolves; the last resort for both stacks.
const SYSTEM_FONT: &str = ".SystemUIFont";

pub const CODE_FONT: &str = "JetBrains Mono";
pub const FONT_UI: &str = "Inter";

static FACES_ADDED: std::sync::Once = std::sync::Once::new();
static UI_FONT_RESOLVED: std::sync::OnceLock<String> = std::sync::OnceLock::new();
static CODE_FONT_RESOLVED: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// The bundled faces as font data, for `TextSystem::add_fonts`.
pub fn bundled_fonts() -> Vec<Cow<'static, [u8]>> {
    BUNDLED_FACES
        .iter()
        .map(|face| Cow::Borrowed(*face))
        .collect()
}

/// Add the bundled faces, then pick the first installed family per stack; call
/// at startup, before any text is laid out. Only the first call does anything:
/// the kit mapping re-runs on every settings save, and re-adding would stack
/// duplicate faces.
pub fn resolve_fonts(cx: &App) {
    FACES_ADDED.call_once(|| {
        if let Err(err) = cx.text_system().add_fonts(bundled_fonts()) {
            tracing::warn!("bundled fonts failed to load: {err}");
        }
    });
    let installed = cx.text_system().all_font_names();
    let pick = |candidates: &[&str]| {
        candidates
            .iter()
            .find(|c| installed.iter().any(|name| name == *c))
            .map(|c| c.to_string())
            .unwrap_or_else(|| SYSTEM_FONT.to_string())
    };
    let _ = UI_FONT_RESOLVED.set(pick(UI_FONT_CANDIDATES));
    let _ = CODE_FONT_RESOLVED.set(pick(CODE_FONT_CANDIDATES));
    tracing::info!("UI font {:?}, code font {:?}", ui_font(), code_font());
}

/// The UI family to lay everything out in.
pub fn ui_font() -> &'static str {
    UI_FONT_RESOLVED
        .get()
        .map(|s| s.as_str())
        .unwrap_or(FONT_UI)
}

/// The mono family for code chips, patches and diffs.
pub fn code_font() -> &'static str {
    CODE_FONT_RESOLVED
        .get()
        .map(|s| s.as_str())
        .unwrap_or(CODE_FONT)
}

// ---- font scale ------------------------------------------------------------

/// The settings text-size values, smallest first: "small" | "normal" |
/// "large" | "larger". Small multiplies the ladder by 0.875; the 12px floor
/// still holds, because every step is clamped in `scaled_by`, so a small
/// setting tightens chrome and spacing without dropping text under the floor.
/// The old "compact" spelling reads as normal.
pub const FONT_SMALL: &str = "small";
pub const FONT_NORMAL: &str = "normal";
pub const FONT_LARGE: &str = "large";
pub const FONT_LARGER: &str = "larger";

pub fn normalize_font_scale(scale: Option<&str>) -> &'static str {
    match scale {
        Some(s) if s == FONT_SMALL => FONT_SMALL,
        Some(s) if s == FONT_LARGE => FONT_LARGE,
        Some(s) if s == FONT_LARGER => FONT_LARGER,
        _ => FONT_NORMAL,
    }
}

/// The family's type scale; everything multiplies the user factor (settings)
/// and the system factor (gsettings text-scaling).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FontTokens {
    /// Rail labels and captions: the floor, nothing is smaller.
    pub font_micro: f32,
    /// Mono metadata: paths, hashes, shortcuts.
    pub font_mono_meta: f32,
    /// A list row's second line.
    pub font_row_meta: f32,
    /// Buttons, pills and tabs.
    pub font_control: f32,
    /// Captions over a field.
    pub font_label: f32,
    /// The line under a heading.
    pub font_sub: f32,
    /// Hints and quiet metadata.
    pub font_meta: f32,
    /// The workhorse: rows, menus and field text.
    pub font_body: f32,
    /// Headings, and running text in a panel.
    pub font_heading: f32,
    /// The product name in a titlebar, and what the user typed.
    pub font_brand: f32,
    /// A list row's name: the first thing read.
    pub font_name: f32,
    /// A note's text; its headings and code scale from it.
    pub font_note: f32,
    /// A list's title.
    pub font_list_title: f32,
    /// A page's title.
    pub font_screen_title: f32,
    pub icon_nano: f32,
    pub icon_tiny: f32,
    pub icon_small: f32,
    pub icon_medium: f32,
    pub icon_large: f32,
    pub icon_xl: f32,
}

/// The base ladder, floor 12: 12 caption / 13 every metadata step / 14 body and
/// controls / 15 names and brand / 16 headings and notes / 20 list titles /
/// 22 page titles.
pub(crate) const BASE_TOKENS: FontTokens = FontTokens {
    font_micro: 12.0,
    font_mono_meta: 13.0,
    font_row_meta: 13.0,
    font_control: 14.0,
    font_label: 13.0,
    font_sub: 13.0,
    font_meta: 13.0,
    font_body: 14.0,
    font_heading: 16.0,
    font_brand: 15.0,
    font_name: 15.0,
    font_note: 16.0,
    font_list_title: 20.0,
    font_screen_title: 22.0,
    icon_nano: 12.0,
    icon_tiny: 14.0,
    icon_small: 16.0,
    icon_medium: 18.0,
    icon_large: 20.0,
    icon_xl: 22.0,
};

fn variant_factor(scale: Option<&str>) -> f32 {
    match normalize_font_scale(scale) {
        FONT_SMALL => 0.875,
        FONT_LARGE => 1.12,
        FONT_LARGER => 1.25,
        _ => 1.0,
    }
}

/// 0 = factor 1.0, set from settings at startup.
static USER_FACTOR: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
/// 0 = factor 1.0, gsettings text-scaling.
static SYSTEM_FACTOR: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn bits(v: u32) -> f32 {
    if v == 0 { 1.0 } else { f32::from_bits(v) }
}

/// The combined scale factor: settings variant × desktop text scaling.
pub fn font_factor() -> f32 {
    bits(USER_FACTOR.load(std::sync::atomic::Ordering::Relaxed))
        * bits(SYSTEM_FACTOR.load(std::sync::atomic::Ordering::Relaxed))
}

/// Applies a text-size variant to this process. Apps go through
/// [`crate::scale`], which also keeps the choice in the shared file.
pub fn set_font_scale(scale: Option<&str>) {
    USER_FACTOR.store(
        f32::to_bits(variant_factor(scale)),
        std::sync::atomic::Ordering::Relaxed,
    );
}

/// The settings text-size variant in force.
pub fn font_scale() -> &'static str {
    let user = bits(USER_FACTOR.load(std::sync::atomic::Ordering::Relaxed));
    [FONT_LARGE, FONT_LARGER]
        .into_iter()
        .find(|variant| variant_factor(Some(variant)) == user)
        .unwrap_or(FONT_NORMAL)
}

/// Feeds the desktop text-scaling factor (gsettings text-scaling-factor);
/// callers re-run the kit theme so every window rescales.
pub fn set_system_factor(factor: f32) {
    SYSTEM_FACTOR.store(f32::to_bits(factor), std::sync::atomic::Ordering::Relaxed);
}

/// The desktop text-scaling factor; 1.0 until something feeds it.
pub fn system_factor() -> f32 {
    bits(SYSTEM_FACTOR.load(std::sync::atomic::Ordering::Relaxed))
}

/// The smallest text the family ever draws. Small multiplies the ladder down,
/// so the floor is applied here, in the one place every step passes through:
/// a step that would land under it lands on it.
pub const TEXT_FLOOR: f32 = 12.0;

fn scaled_by(factor: f32, v: f32) -> f32 {
    let scaled = (v * factor * 100.0).round() / 100.0;
    if v >= TEXT_FLOOR {
        scaled.max(TEXT_FLOOR)
    } else {
        scaled
    }
}

fn tokens_at(factor: f32) -> FontTokens {
    let scaled = |v: f32| scaled_by(factor, v);
    FontTokens {
        font_micro: scaled(BASE_TOKENS.font_micro),
        font_mono_meta: scaled(BASE_TOKENS.font_mono_meta),
        font_row_meta: scaled(BASE_TOKENS.font_row_meta),
        font_control: scaled(BASE_TOKENS.font_control),
        font_label: scaled(BASE_TOKENS.font_label),
        font_sub: scaled(BASE_TOKENS.font_sub),
        font_meta: scaled(BASE_TOKENS.font_meta),
        font_body: scaled(BASE_TOKENS.font_body),
        font_heading: scaled(BASE_TOKENS.font_heading),
        font_brand: scaled(BASE_TOKENS.font_brand),
        font_name: scaled(BASE_TOKENS.font_name),
        font_note: scaled(BASE_TOKENS.font_note),
        font_list_title: scaled(BASE_TOKENS.font_list_title),
        font_screen_title: scaled(BASE_TOKENS.font_screen_title),
        icon_nano: scaled(BASE_TOKENS.icon_nano),
        icon_tiny: scaled(BASE_TOKENS.icon_tiny),
        icon_small: scaled(BASE_TOKENS.icon_small),
        icon_medium: scaled(BASE_TOKENS.icon_medium),
        icon_large: scaled(BASE_TOKENS.icon_large),
        icon_xl: scaled(BASE_TOKENS.icon_xl),
    }
}

/// The scaled token ladder for one (settings variant, system factor) pair.
pub fn font_tokens(font_scale: Option<&str>, system_factor: f32) -> FontTokens {
    tokens_at(variant_factor(font_scale) * system_factor)
}

/// The ladder at the live factors.
pub fn active_tokens() -> FontTokens {
    tokens_at(font_factor())
}

// ---- token getters ---------------------------------------------------------
// The live ladder for views that read one step at render time.

fn scaled(v: f32) -> f32 {
    scaled_by(font_factor(), v)
}

pub fn font_micro() -> f32 {
    scaled(BASE_TOKENS.font_micro)
}

pub fn font_mono_meta() -> f32 {
    scaled(BASE_TOKENS.font_mono_meta)
}

pub fn font_row_meta() -> f32 {
    scaled(BASE_TOKENS.font_row_meta)
}

pub fn font_control() -> f32 {
    scaled(BASE_TOKENS.font_control)
}

pub fn font_label() -> f32 {
    scaled(BASE_TOKENS.font_label)
}

pub fn font_sub() -> f32 {
    scaled(BASE_TOKENS.font_sub)
}

pub fn font_meta() -> f32 {
    scaled(BASE_TOKENS.font_meta)
}

pub fn font_body() -> f32 {
    scaled(BASE_TOKENS.font_body)
}

pub fn font_heading() -> f32 {
    scaled(BASE_TOKENS.font_heading)
}

pub fn font_brand() -> f32 {
    scaled(BASE_TOKENS.font_brand)
}

pub fn font_name() -> f32 {
    scaled(BASE_TOKENS.font_name)
}

pub fn font_note() -> f32 {
    scaled(BASE_TOKENS.font_note)
}

pub fn font_list_title() -> f32 {
    scaled(BASE_TOKENS.font_list_title)
}

pub fn font_screen_title() -> f32 {
    scaled(BASE_TOKENS.font_screen_title)
}

pub fn icon_tiny() -> f32 {
    scaled(BASE_TOKENS.icon_tiny)
}

pub fn icon_small() -> f32 {
    scaled(BASE_TOKENS.icon_small)
}

pub fn icon_medium() -> f32 {
    scaled(BASE_TOKENS.icon_medium)
}

pub fn icon_large() -> f32 {
    scaled(BASE_TOKENS.icon_large)
}

// ---- kit fields ------------------------------------------------------------

/// The share of a custom size the kit sets a field's text at; height and
/// padding stay the small control's.
const FIELD_TEXT_SHARE: f32 = 0.875;

/// A kit field whose text sits at `text` pixels. Fields size their text in rem
/// otherwise, which puts them on no step of the ladder.
pub fn field_size_at(text: f32) -> gpui_kit::component::Size {
    gpui_kit::component::Size::Size(gpui::px(text / FIELD_TEXT_SHARE))
}

/// A kit field at the body step.
pub fn field_size() -> gpui_kit::component::Size {
    field_size_at(font_body())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_bundled_face_is_a_truetype_file() {
        for face in bundled_fonts() {
            assert!(face.starts_with(&[0, 1, 0, 0]), "not an sfnt font");
        }
    }

    #[test]
    fn the_variants_rescale_every_step() {
        let normal = font_tokens(None, 1.0);
        assert_eq!(font_tokens(Some(FONT_NORMAL), 1.0), normal);
        assert_eq!(font_tokens(Some(FONT_LARGE), 1.0).font_body, 15.68);
        assert_eq!(font_tokens(Some(FONT_LARGER), 1.0).font_body, 17.5);
        assert_eq!(font_tokens(None, 2.0).icon_medium, 2.0 * normal.icon_medium);
    }

    #[test]
    fn nothing_on_the_ladder_sits_under_the_floor() {
        let t = font_tokens(None, 1.0);
        let steps = [
            t.font_micro,
            t.font_mono_meta,
            t.font_row_meta,
            t.font_control,
            t.font_label,
            t.font_sub,
            t.font_meta,
            t.font_body,
            t.font_heading,
            t.font_brand,
            t.font_name,
            t.font_note,
            t.font_list_title,
            t.font_screen_title,
        ];
        assert!(steps.iter().all(|step| *step >= 12.0), "{steps:?}");
    }

    #[test]
    fn a_saved_compact_reads_as_normal() {
        assert_eq!(normalize_font_scale(Some("compact")), FONT_NORMAL);
    }

    #[test]
    fn a_field_size_lands_on_the_requested_text_size() {
        let gpui_kit::component::Size::Size(size) = field_size_at(11.5) else {
            panic!("a custom size carries its pixels");
        };
        assert!((f32::from(size) * FIELD_TEXT_SHARE - 11.5).abs() < 0.01);
    }
}

#[cfg(test)]
mod small_step_tests {
    use super::*;

    /// Small is a real step down, but no step may cross the floor.
    #[test]
    fn small_tightens_the_ladder_without_breaking_the_floor() {
        let small = font_tokens(Some(FONT_SMALL), 1.0);
        let normal = font_tokens(Some(FONT_NORMAL), 1.0);
        assert!(small.font_body < normal.font_body, "small is smaller");
        assert_eq!(small.font_body, 12.25, "14 × 0.875");
        for step in [
            small.font_micro,
            small.font_mono_meta,
            small.font_row_meta,
            small.font_control,
            small.font_label,
            small.font_sub,
            small.font_meta,
            small.font_body,
        ] {
            assert!(step >= TEXT_FLOOR, "{step} is under the floor");
        }
        // the two steps that started at the floor cannot go under it
        assert_eq!(small.font_micro, TEXT_FLOOR);
    }

    /// A user who already runs the desktop scale up keeps the small step
    /// useful: the two factors multiply, and the floor still holds.
    #[test]
    fn small_survives_a_desktop_scale() {
        let small = font_tokens(Some(FONT_SMALL), 1.1667);
        assert!(small.font_body >= TEXT_FLOOR);
        assert!(small.font_body > 12.0);
    }

    #[test]
    fn the_old_compact_spelling_is_normal() {
        assert_eq!(normalize_font_scale(Some("compact")), FONT_NORMAL);
        assert_eq!(normalize_font_scale(Some(FONT_SMALL)), FONT_SMALL);
    }
}
