//! The shared palette: Copper, the warp-suite-v3 design. One lifted warm ramp
//! whose darkest step is a recessed well, not the window, plus one hue per
//! entity. Surfaces step well < chrome < base < panel < raised < overlay; the
//! desktop clients name a few steps differently, so both spellings exist as
//! aliases of the same constants.

use gpui::{Hsla, Rgba, hsla, rgba};
use std::sync::Arc;

/// const-friendly rgba: gpui's `rgba()` is not const in this rev.
pub const fn rgba_hex(hex: u32) -> Rgba {
    Rgba {
        r: ((hex >> 24) & 0xFF) as f32 / 255.0,
        g: ((hex >> 16) & 0xFF) as f32 / 255.0,
        b: ((hex >> 8) & 0xFF) as f32 / 255.0,
        a: (hex & 0xFF) as f32 / 255.0,
    }
}

/// rgb hex (0xRRGGBB) → fully opaque Rgba. gpui's rgba() takes 0xRRGGBBAA,
/// so the alpha goes in the LOW byte — OR-ing it high scrambles the channels.
pub fn rgba_u32(rgb: u32) -> Rgba {
    rgba((rgb << 8) | 0xFF)
}

// ---- surfaces --------------------------------------------------------------
// One warm ramp, every step far enough apart that a panel reads by value alone:
// well < chrome < base < panel < raised < overlay. The design's names for the
// steps are the aliases under the constants.

pub const SURFACE: Rgba = rgba_hex(0x252321FF); // window canvas, main area
/// DevTools spelling of [`SURFACE`]: window canvas + page.
pub const BACKGROUND: Rgba = SURFACE;
pub const CARD: Rgba = rgba_hex(0x2B2926FF); // tile surface, table rows, cards
pub const WINDOW_CHROME: Rgba = rgba_hex(0x201E1CFF); // titlebar, sidebar, ring
/// DevTools spelling of [`WINDOW_CHROME`]: the titlebar.
pub const TOPBAR_BG: Rgba = WINDOW_CHROME;
/// warp-agent spelling of [`WINDOW_CHROME`]: settings sheet.
pub const SETTINGS_BG: Rgba = WINDOW_CHROME;
pub const SETTINGS_CARD: Rgba = CARD;
pub const POPUP: Rgba = rgba_hex(0x3A3733FF); // popovers, menus, tooltips
pub const INPUT: Rgba = rgba_hex(0x1C1A18FF); // text fields
pub const CHIP: Rgba = rgba_hex(0x34312EFF); // neutral pill background
pub const HAIRLINE: Rgba = rgba_hex(0x3B3834FF); // separators, card borders
pub const BORDER_LIGHT: Rgba = rgba_hex(0x2F2C29FF); // quiet dividers
/// warp-agent alias of [`BORDER_LIGHT`].
pub const TILE_HEADER_SEPARATOR: Rgba = BORDER_LIGHT;
/// Stronger separators: the rail's sync card and the tag pills.
pub const LINE_STRONG: Rgba = rgba_hex(0x4F4B46FF);
/// The selected list row: one accent-tinted step above the card ramp.
pub const SELECTED: Rgba = rgba_hex(0x34312EFF);
/// Inset code/diff/fold body: the recessed step under the card.
pub const DIFF_BG: Rgba = rgba_hex(0x1C1A18FF);
/// Hover wash for any surface step: the ramp's warm hue, about one step up
/// from whatever sits under it.
pub const HOVER: Hsla = hsla(0.08, 0.25, 0.85, 0.06);
/// Row/pill hover as an opaque fill, for lists on the canvas that want the
/// same step whatever sits under them.
pub const HOVER_BG: Rgba = CHIP;
/// The design's name for [`INPUT`]: fields, diffs and code, the darkest step.
pub const WELL: Rgba = INPUT;
/// The design's name for [`CARD`]: the detail pane and panels.
pub const PANEL: Rgba = CARD;
/// The design's name for [`CHIP`]: selected rows, hovers and active segments.
pub const RAISED: Rgba = CHIP;
/// The design's name for [`POPUP`]: dialogs and menus.
pub const OVERLAY: Rgba = POPUP;
/// The dim layer under dialogs.
pub const SCRIM: Rgba = rgba_hex(0x00000080);

// ---- chrome steps ----------------------------------------------------------
// The search field, ghost buttons, the segmented control's trough, the folder
// band, the sync card and the footer each sit on their own step of the ramp;
// sampling them out of the mockup keeps the layers identical.

pub const SEARCH_BG: Rgba = rgba_hex(0x1C1A18FF);
pub const SEARCH_BORDER: Rgba = rgba_hex(0x3B3834FF);
pub const GHOST_BG: Rgba = rgba_hex(0x2B2926FF);
pub const GHOST_BORDER: Rgba = rgba_hex(0x4F4B46FF);
pub const GHOST_BORDER_HOVER: Rgba = rgba_hex(0x625D56FF);
pub const SEG_BG: Rgba = rgba_hex(0x1C1A18FF);
pub const SEG_BORDER: Rgba = rgba_hex(0x3B3834FF);
pub const SEG_ACTIVE: Rgba = rgba_hex(0x34312EFF);
pub const SEG_ACTIVE_FG: Rgba = rgba_hex(0xF6F2EBFF);
pub const GROUP_BG: Rgba = rgba_hex(0x252321FF);
pub const SYNC_BG: Rgba = rgba_hex(0x1C1A18FF);
pub const FOOT_BG: Rgba = rgba_hex(0x201E1CFF);
pub const KEY_BG: Rgba = rgba_hex(0x1C1A18FF);
pub const KEY_BORDER: Rgba = rgba_hex(0x4F4B46FF);
pub const KEY_FG: Rgba = rgba_hex(0xD2CBC0FF);
pub const ROW_LINE: Rgba = rgba_hex(0x2F2C29FF);
pub const ROWS_RULE: Rgba = rgba_hex(0x2F2C29FF);
pub const MUTED_ICON: Rgba = rgba_hex(0xABA397FF);
/// The logo tile in the titlebar.
pub const BRAND_BG: Rgba = rgba_hex(0x43301FFF);
pub const BRAND_BORDER: Rgba = rgba_hex(0x5C4229FF);

// ---- text ------------------------------------------------------------------
// Three steps: hierarchy comes from weight and size, not from a grey ladder.

pub const TEXT: Rgba = rgba_hex(0xF6F2EBFF); // primary text
pub const TEXT_DIM: Rgba = rgba_hex(0xD2CBC0FF); // secondary body text
pub const TEXT_MUTED: Rgba = rgba_hex(0xABA397FF); // labels, metadata
/// The same step as [`TEXT_MUTED`]: the design has three text steps, and a
/// fourth grey was what made quiet text unreadable.
pub const TEXT_FAINT: Rgba = rgba_hex(0xABA397FF);

/// Text selection in fields: the accent at 25% alpha.
pub fn selection(accent: &AccentColors) -> Rgba {
    let a = accent.accent;
    Rgba {
        r: a.r,
        g: a.g,
        b: a.b,
        a: 0.25,
    }
}

// ---- accent ----------------------------------------------------------------

/// The default accent: copper, for the single filled action. Fills only: text
/// (the live markdown highlighter, links, active pills) reads [`ACCENT_INK`];
/// apps with a preset switch pass their [`AccentColors`] to
/// [`crate::kit::apply_kit_theme`].
pub const ACCENT: Rgba = rgba_hex(0xEE9A5FFF);
/// Dark ink: text on the copper accent.
pub const ON_ACCENT: Rgba = rgba_hex(0x24140AFF);
/// The accent as ink: links, focus rings, the favorite star.
pub const ACCENT_INK: Rgba = rgba_hex(0xF2A877FF);
/// The tinted tile behind the brand mark and an active zone.
pub const ACCENT_SOFT: Rgba = BRAND_BG;
pub const ACCENT_HOVER: Rgba = rgba_hex(0xF5AC78FF);
pub const ACCENT_BORDER: Rgba = rgba_hex(0xF5AC78FF);
/// A keycap's edge on a filled accent.
pub const ON_ACCENT_BORDER: Rgba = rgba_hex(0x00000038);

/// One named accent family. The first preset is the default
/// ([`normalize_accent`] fallback): copper, so a saved "blue" lands on it.
pub struct AccentPreset {
    pub key: &'static str,
    /// rgb hex, one shared by accent + success
    pub accent: u32,
    /// rgb hex of the accent as ink (text, links, focus ring): the fill
    /// itself when it clears AA on the ramp, else a lighter sibling
    pub text: u32,
    pub dim: u32,
    pub on: u32,
    pub success_dim: u32,
}

pub const ACCENT_PRESETS: [AccentPreset; 8] = [
    AccentPreset {
        key: "copper",
        accent: 0xEE9A5F,
        text: 0xF2A877,
        dim: 0x43301F,
        on: 0x24140A,
        success_dim: 0x43301F,
    },
    AccentPreset {
        key: "green",
        accent: 0x4AC265,
        text: 0x4AC265,
        dim: 0x223322,
        on: 0x0C140C,
        success_dim: 0x1D3122,
    },
    AccentPreset {
        key: "amber",
        accent: 0xE8A348,
        text: 0xE8A348,
        dim: 0x33270F,
        on: 0x171006,
        success_dim: 0x382B12,
    },
    AccentPreset {
        key: "salmon",
        accent: 0xF47067,
        text: 0xF68A82,
        dim: 0x38201D,
        on: 0x1A0B09,
        success_dim: 0x3B2523,
    },
    AccentPreset {
        key: "gray",
        accent: 0xA8A8A8,
        text: 0xA8A8A8,
        dim: 0x262626,
        on: 0x101010,
        success_dim: 0x2F2F2F,
    },
    AccentPreset {
        key: "silver",
        accent: 0xC9C9C9,
        text: 0xC9C9C9,
        dim: 0x303030,
        on: 0x141010,
        success_dim: 0x383838,
    },
    AccentPreset {
        key: "graphite",
        accent: 0x7A7A7A,
        text: 0xADADAD,
        dim: 0x1F1F1F,
        on: 0x0E0E0E,
        success_dim: 0x292929,
    },
    AccentPreset {
        key: "charcoal",
        accent: 0x505050,
        text: 0xADADAD,
        dim: 0x1A1A1A,
        on: 0x0C0C0C,
        success_dim: 0x232323,
    },
];

pub fn normalize_accent(accent: Option<&str>) -> &'static AccentPreset {
    ACCENT_PRESETS
        .iter()
        .find(|p| Some(p.key) == accent)
        .unwrap_or(&ACCENT_PRESETS[0])
}

/// The live accent family; swapped by the settings page.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AccentColors {
    pub accent: Rgba,
    pub text: Rgba,
    pub dim: Rgba,
    pub on: Rgba,
    pub success_dim: Rgba,
}

impl AccentColors {
    pub fn for_preset(preset: &AccentPreset) -> Self {
        Self {
            accent: rgba_hex((preset.accent << 8) | 0xFF),
            text: rgba_hex((preset.text << 8) | 0xFF),
            dim: rgba_hex((preset.dim << 8) | 0xFF),
            on: rgba_hex((preset.on << 8) | 0xFF),
            success_dim: rgba_hex((preset.success_dim << 8) | 0xFF),
        }
    }

    /// The default family: the copper preset.
    pub fn default_colors() -> Self {
        Self::for_preset(&ACCENT_PRESETS[0])
    }
}

// ---- fixed semantic colors (never follow the accent preset) ----------------
// One hue per meaning, the same in every app, always beside an icon or a word.
// Every hue clears AA up to the overlay step.

pub const DANGER: Rgba = rgba_hex(0xF88C86FF);
/// Text on a filled [`DANGER`]: the soft red needs dark ink, white sits at 2.4:1.
pub const ON_DANGER: Rgba = SURFACE;
pub const AMBER_BRIGHT: Rgba = rgba_hex(0xF2BC5BFF);
/// warp-agent alias of [`AMBER_BRIGHT`].
pub const WARN: Rgba = AMBER_BRIGHT;
pub const SUCCESS_BRIGHT: Rgba = rgba_hex(0x7FD69BFF);
pub const SUCCESS_DEEP: Rgba = rgba_hex(0x4FB277FF);
/// warp-agent alias of [`SUCCESS_BRIGHT`].
pub const ADD: Rgba = SUCCESS_BRIGHT;
/// warp-agent alias of [`DANGER`].
pub const DEL: Rgba = DANGER;
/// warp-agent alias of [`SUCCESS_BRIGHT`]: working dot + sidebar spinner.
pub const WORKING: Rgba = ADD;
/// warp-agent alias of [`SUCCESS_BRIGHT`]: sidebar running indicators.
pub const RUNNING: Rgba = ADD;
pub const PURPLE_BRIGHT: Rgba = rgba_hex(0xBCA6F7FF);
/// The old spelling of [`ACCENT_INK`]: links, focus rings, active pills.
pub const BLUE_BRIGHT: Rgba = ACCENT_INK;
/// warp-agent alias of [`ACCENT_INK`]: favorite star, active.
pub const GOLD_BRIGHT: Rgba = ACCENT_INK;
/// A branch: the icon only, the name stays [`TEXT_DIM`].
pub const BRANCH: Rgba = rgba_hex(0x6ED0BEFF);
/// Uncommitted work, and anything that needs a person.
pub const CHANGES: Rgba = AMBER_BRIGHT;
/// Pull requests.
pub const PR: Rgba = PURPLE_BRIGHT;
/// Issues.
pub const ISSUE: Rgba = rgba_hex(0xF795AEFF);
/// Commits to pull, and work that is running.
pub const BEHIND: Rgba = rgba_hex(0x86C8F2FF);
/// Commits to push.
pub const AHEAD: Rgba = rgba_hex(0xA3D977FF);
pub const STAR_MUTED: Rgba = rgba_hex(0xABA397FF); // favorite star, inactive
pub const STAR_ON: Rgba = rgba_hex(0xD2CBC0FF);

/// Tinted wash for a bright hue — the universal chip background and the
/// destructive/close hover.
pub fn tint(color: Rgba) -> Hsla {
    let h: Hsla = color.into();
    Hsla { a: 0.18, ..h }
}

// ---- code ------------------------------------------------------------------

pub const CODE_FG: Rgba = TEXT;

// ---- diffs -----------------------------------------------------------------

pub const DIFF_GUTTER: Rgba = TEXT_FAINT; // line-number gutter text
/// Full-width bands; the gutter cell over the line numbers paints the same
/// shade so an edited row reads as one flat band.
pub const DIFF_ADD_BG: Hsla = hsla(0.388, 0.515, 0.669, 0.13);
pub const DIFF_DEL_BG: Hsla = hsla(0.009, 0.892, 0.749, 0.13);
/// The hunk band: the muted text's warm hue, so it stays on the ramp.
pub const DIFF_HUNK_BG: Hsla = hsla(0.08, 0.12, 0.6, 0.10);

// ---- chat surfaces ---------------------------------------------------------
// User bubble = chip tone, assistant bubble = popup tone raised off the tile
// card; the shared ramp derives them.

pub const BUBBLE_USER: Rgba = CHIP;
pub const BUBBLE_ASSISTANT: Rgba = CARD;
pub const Q_OPT_SELECTED: Rgba = CHIP;
pub const BUBBLE_QUEUED: Rgba = WINDOW_CHROME;
pub const QUEUED_BORDER: Rgba = HAIRLINE;
/// The base step at 0.7 alpha over the chat.
pub const FLYOUT_SURFACE: Rgba = rgba_hex(0x252321B3);

// ---- syntax highlight theme ------------------------------------------------

/// Syntax token colors for markdown code fences and diff lines: the family
/// mapping on the ramp's hues (comment TEXT_FAINT italic, punctuation
/// TEXT_MUTED, string SUCCESS_BRIGHT, keyword PURPLE_BRIGHT, function
/// BEHIND, and the one amber for numbers, types and attributes). The
/// kit's HighlightTheme deserializes from the Zed theme JSON format —
/// ThemeStyle fields are private, so building the struct by hand is not
/// possible.
const HIGHLIGHT_THEME_JSON: &str = r##"{
  "name": "warpui-kit",
  "appearance": "dark",
  "style": {
    "editor.foreground": "#F6F2EB",
    "syntax": {
      "attribute": { "color": "#F2BC5B" },
      "boolean": { "color": "#F2BC5B" },
      "comment": { "color": "#ABA397", "font_style": "italic" },
      "comment.doc": { "color": "#ABA397", "font_style": "italic" },
      "constant": { "color": "#F2BC5B" },
      "constructor": { "color": "#F2BC5B" },
      "function": { "color": "#86C8F2" },
      "keyword": { "color": "#BCA6F7" },
      "number": { "color": "#F2BC5B" },
      "operator": { "color": "#ABA397" },
      "preproc": { "color": "#BCA6F7" },
      "property": { "color": "#F6F2EB" },
      "punctuation": { "color": "#ABA397" },
      "punctuation.bracket": { "color": "#ABA397" },
      "punctuation.delimiter": { "color": "#ABA397" },
      "punctuation.list_marker": { "color": "#ABA397" },
      "string": { "color": "#7FD69B" },
      "string.escape": { "color": "#F2BC5B" },
      "tag": { "color": "#F2BC5B" },
      "type": { "color": "#F2BC5B" },
      "variable": { "color": "#F6F2EB" },
      "variable.special": { "color": "#F2BC5B" }
    }
  }
}"##;

static HIGHLIGHT_THEME: std::sync::OnceLock<Arc<gpui_kit::component::highlighter::HighlightTheme>> =
    std::sync::OnceLock::new();

/// The installed token theme; fixed hues, so built once and pointer-stable
/// for the process lifetime (the highlight caches key on the Arc).
pub fn highlight_theme() -> Arc<gpui_kit::component::highlighter::HighlightTheme> {
    HIGHLIGHT_THEME
        .get_or_init(|| {
            serde_json::from_str(HIGHLIGHT_THEME_JSON).unwrap_or_else(|e| {
                tracing::warn!("highlight theme JSON unparsable ({e}); using kit default");
                gpui_kit::component::highlighter::HighlightTheme::default_dark()
            })
        })
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn luminance(color: Rgba) -> f32 {
        let linear = |v: f32| {
            if v <= 0.03928 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(color.r) + 0.7152 * linear(color.g) + 0.0722 * linear(color.b)
    }

    fn contrast(a: Rgba, b: Rgba) -> f32 {
        let (a, b) = (luminance(a), luminance(b));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    #[test]
    fn ink_holds_aa_on_every_surface() {
        let inks = [
            TEXT,
            TEXT_DIM,
            TEXT_MUTED,
            TEXT_FAINT,
            ACCENT_INK,
            AMBER_BRIGHT,
            SUCCESS_BRIGHT,
            DANGER,
            BRANCH,
            PR,
            ISSUE,
            BEHIND,
            AHEAD,
        ];
        for ink in inks {
            for surface in [WELL, WINDOW_CHROME, SURFACE, PANEL, RAISED, OVERLAY] {
                assert!(
                    contrast(ink, surface) >= 4.5,
                    "{ink:?} on {surface:?} is {:.2}:1",
                    contrast(ink, surface)
                );
            }
        }
    }

    #[test]
    fn filled_actions_keep_their_text_legible() {
        assert!(contrast(ON_ACCENT, ACCENT) >= 4.5);
        assert!(contrast(ON_DANGER, DANGER) >= 4.5);
    }

    #[test]
    fn every_preset_inks_legibly() {
        for preset in &ACCENT_PRESETS {
            let colors = AccentColors::for_preset(preset);
            assert!(
                contrast(colors.text, POPUP) >= 4.5,
                "the {} preset's text is {:.2}:1 on a popup",
                preset.key,
                contrast(colors.text, POPUP)
            );
        }
    }

    #[test]
    fn the_copper_preset_is_the_palette_accent() {
        let copper = AccentColors::default_colors();
        assert_eq!(copper.accent, ACCENT);
        assert_eq!(copper.text, ACCENT_INK);
        assert_eq!(copper.dim, ACCENT_SOFT);
        assert_eq!(copper.on, ON_ACCENT);
    }

    #[test]
    fn a_saved_blue_lands_on_copper() {
        assert_eq!(normalize_accent(Some("blue")).key, "copper");
    }

    #[test]
    fn the_ramp_lifts_from_the_well_to_the_overlay() {
        let ramp = [WELL, WINDOW_CHROME, SURFACE, PANEL, RAISED, OVERLAY];
        for pair in ramp.windows(2) {
            assert!(luminance(pair[0]) < luminance(pair[1]), "{pair:?}");
        }
    }

    /// The token theme is a JSON literal of hex values, so this pins them to
    /// the constants; the loader falls back quietly, so a broken literal would
    /// otherwise pass unnoticed.
    #[test]
    fn syntax_tokens_ride_the_palette() {
        let tokens: gpui_kit::component::highlighter::HighlightTheme =
            serde_json::from_str(HIGHLIGHT_THEME_JSON).expect("the token theme parses");
        let color = |name: &str| tokens.style(name).and_then(|style| style.color);

        assert_eq!(color("variable"), Some(TEXT.into()));
        assert_eq!(color("comment"), Some(TEXT_FAINT.into()));
        assert_eq!(color("punctuation"), Some(TEXT_MUTED.into()));
        assert_eq!(color("string"), Some(SUCCESS_BRIGHT.into()));
        assert_eq!(color("keyword"), Some(PURPLE_BRIGHT.into()));
        assert_eq!(color("function"), Some(BEHIND.into()));
        assert_eq!(color("number"), Some(AMBER_BRIGHT.into()));
        assert_eq!(color("type"), Some(AMBER_BRIGHT.into()));
    }
}
