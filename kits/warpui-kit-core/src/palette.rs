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

// ---- the switchable ramp ---------------------------------------------------
// A palette is a swap of the ramp and the accent family, nothing else: every
// app draws through these, so switching one switches all of them. Copper is
// the default and the values here are exactly the constants below, which stay
// as the family's published reference.

/// One palette: the six surface steps, the three lines, the three inks and the
/// accent family. Entity hues (branch, changes, pr, …) are fixed across
/// palettes, so they are not part of it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ramp {
    pub key: &'static str,
    /// The preset's label for the settings control.
    pub label: &'static str,
    pub well: Rgba,
    pub chrome: Rgba,
    pub base: Rgba,
    pub panel: Rgba,
    pub raised: Rgba,
    pub overlay: Rgba,
    pub line_soft: Rgba,
    pub line: Rgba,
    pub line_strong: Rgba,
    pub text: Rgba,
    pub text_dim: Rgba,
    pub text_muted: Rgba,
    pub accent: Rgba,
    pub accent_hover: Rgba,
    pub accent_ink: Rgba,
    pub accent_soft: Rgba,
    pub on_accent: Rgba,
}

impl Ramp {
    /// The six steps bottom to top, for the contrast and ordering tests.
    pub fn surfaces(&self) -> [Rgba; 6] {
        [
            self.well,
            self.chrome,
            self.base,
            self.panel,
            self.raised,
            self.overlay,
        ]
    }

    /// The accent family as the preset system's [`AccentColors`], so a palette
    /// and an accent preset drive the same code.
    pub fn accent_colors(&self) -> AccentColors {
        AccentColors {
            accent: self.accent,
            text: self.accent_ink,
            dim: self.accent_soft,
            on: self.on_accent,
            success_dim: self.accent_soft,
        }
    }
}

const fn ramp(key: &'static str, label: &'static str, hexes: [u32; 17]) -> Ramp {
    Ramp {
        key,
        label,
        well: rgba_hex(hexes[0]),
        chrome: rgba_hex(hexes[1]),
        base: rgba_hex(hexes[2]),
        panel: rgba_hex(hexes[3]),
        raised: rgba_hex(hexes[4]),
        overlay: rgba_hex(hexes[5]),
        line_soft: rgba_hex(hexes[6]),
        line: rgba_hex(hexes[7]),
        line_strong: rgba_hex(hexes[8]),
        text: rgba_hex(hexes[9]),
        text_dim: rgba_hex(hexes[10]),
        text_muted: rgba_hex(hexes[11]),
        accent: rgba_hex(hexes[12]),
        accent_hover: rgba_hex(hexes[13]),
        accent_ink: rgba_hex(hexes[14]),
        accent_soft: rgba_hex(hexes[15]),
        on_accent: rgba_hex(hexes[16]),
    }
}

/// Every palette the settings control offers, in order. Copper first: it is
/// the default, and a saved key that no longer exists lands on it.
pub const RAMPS: [Ramp; 4] = [
    ramp(
        "copper",
        "Copper",
        [
            0x1C1A18FF, 0x201E1CFF, 0x252321FF, 0x2B2926FF, 0x34312EFF, 0x3A3733FF, 0x2F2C29FF,
            0x3B3834FF, 0x4F4B46FF, 0xF6F2EBFF, 0xD2CBC0FF, 0xABA397FF, 0xEE9A5FFF, 0xF5AC78FF,
            0xF2A877FF, 0x43301FFF, 0x24140AFF,
        ],
    ),
    ramp(
        "plum",
        "Plum",
        [
            0x1C1A21FF, 0x211E27FF, 0x26232DFF, 0x2C2934FF, 0x36323FFF, 0x3C3846FF, 0x302D38FF,
            0x3C3846FF, 0x524D5CFF, 0xF5F2F7FF, 0xD1CBD8FF, 0xAAA3B3FF, 0xF2907FFF, 0xF7A698FF,
            0xF5A294FF, 0x452B33FF, 0x2A0F0BFF,
        ],
    ),
    ramp(
        "pine",
        "Pine",
        [
            0x171C1BFF, 0x1B2120FF, 0x202625FF, 0x262D2BFF, 0x2F3735FF, 0x353E3BFF, 0x2B3331FF,
            0x36403DFF, 0x4A5551FF, 0xEFF4F1FF, 0xC8D2CDFF, 0x9FABA6FF, 0xE5C066FF, 0xEFCF80FF,
            0xE9C976FF, 0x3A3620FF, 0x231B05FF,
        ],
    ),
    ramp(
        "blue",
        "Blue",
        [
            0x181A20FF, 0x1D2027FF, 0x22262EFF, 0x282D36FF, 0x2F3540FF, 0x363C48FF, 0x2A2F38FF,
            0x363C47FF, 0x49505CFF, 0xF0F4FBFF, 0xC9D2E0FF, 0x9FA9B8FF, 0x5B9DF5FF, 0x77B0F8FF,
            0x8FBBFAFF, 0x1E2F4AFF, 0x0A1626FF,
        ],
    ),
];

/// The palette a saved key names, or Copper when it names nothing that exists.
pub fn normalize_ramp(key: Option<&str>) -> &'static Ramp {
    RAMPS
        .iter()
        .find(|r| Some(r.key) == key)
        .unwrap_or(&RAMPS[0])
}

/// The live palette. A plain static with interior mutability: every read goes
/// through [`ramp`], and the settings page is the only writer, so no app can
/// see a half-switched palette inside one frame.
static LIVE_RAMP: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// The palette every surface draws in.
pub fn current_ramp() -> &'static Ramp {
    let index = LIVE_RAMP.load(std::sync::atomic::Ordering::Relaxed);
    &RAMPS[index.min(RAMPS.len() - 1)]
}

/// Switches the palette for every surface that reads [`current_ramp`] after
/// this call. Returns the ramp now in effect.
pub fn set_ramp(key: &str) -> &'static Ramp {
    let ramp = normalize_ramp(Some(key));
    let index = RAMPS.iter().position(|r| r.key == ramp.key).unwrap_or(0);
    LIVE_RAMP.store(index, std::sync::atomic::Ordering::Relaxed);
    ramp
}

// ---- surfaces --------------------------------------------------------------
// One warm ramp, every step far enough apart that a panel reads by value alone:
// well < chrome < base < panel < raised < overlay. The design's names for the
// steps are the aliases under the constants.

pub const SURFACE: Rgba = rgba_hex(0x252321FF); // window canvas, main area
pub const CARD: Rgba = rgba_hex(0x2B2926FF); // tile surface, table rows, cards
pub const WINDOW_CHROME: Rgba = rgba_hex(0x201E1CFF); // titlebar, sidebar, ring
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
/// The content half of an edited row: quiet enough for token colors to stay
/// readable on top of it.
pub const DIFF_ADD_BG: Hsla = hsla(0.388, 0.515, 0.669, 0.10);
pub const DIFF_DEL_BG: Hsla = hsla(0.009, 0.892, 0.749, 0.10);
/// The gutter half of an edited row: stronger than the content wash, so the
/// number and marker carry the change and the code stays legible.
pub const DIFF_ADD_GUTTER_BG: Hsla = hsla(0.388, 0.515, 0.669, 0.18);
pub const DIFF_DEL_GUTTER_BG: Hsla = hsla(0.009, 0.892, 0.749, 0.18);
/// The changed-word band inside a paired deletion/addition row.
pub const DIFF_ADD_EMPHASIS: Hsla = hsla(0.388, 0.515, 0.669, 0.30);
pub const DIFF_DEL_EMPHASIS: Hsla = hsla(0.009, 0.892, 0.749, 0.30);
/// The hunk band: the muted text's warm hue, so it stays on the ramp.
pub const DIFF_HUNK_BG: Hsla = hsla(0.08, 0.12, 0.6, 0.10);

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

// ---- live tokens -----------------------------------------------------------
// What app code actually draws with. Each one reads the live palette, so a
// settings switch repaints every surface on its next frame. They are
// functions, not constants: that is the whole point of the move, and it is why
// `theme::TEXT` became `theme::text()`.

/// The window canvas: the main area under the panes.
pub fn surface() -> Rgba {
    current_ramp().base
}
/// The titlebar, the rail and the chrome.
pub fn window_chrome() -> Rgba {
    current_ramp().chrome
}
/// A card, a tile, the detail pane.
pub fn panel() -> Rgba {
    current_ramp().panel
}
/// The selected row, hover fills, active segments.
pub fn raised() -> Rgba {
    current_ramp().raised
}
/// Menus, dialogs and tooltips.
pub fn popup() -> Rgba {
    current_ramp().overlay
}
/// Fields, diffs and code: the recessed step.
pub fn well() -> Rgba {
    current_ramp().well
}
/// Quiet dividers.
pub fn border_light() -> Rgba {
    current_ramp().line_soft
}
/// The default separator.
pub fn hairline() -> Rgba {
    current_ramp().line
}
/// Control borders.
pub fn line_strong() -> Rgba {
    current_ramp().line_strong
}
/// Names and values.
pub fn text() -> Rgba {
    current_ramp().text
}
/// Secondary copy.
pub fn text_dim() -> Rgba {
    current_ramp().text_dim
}
/// Timestamps and hints.
pub fn text_muted() -> Rgba {
    current_ramp().text_muted
}
/// The quietest ink; the same step as [`text_muted`].
pub fn text_faint() -> Rgba {
    current_ramp().text_muted
}
/// The primary fill.
pub fn accent() -> Rgba {
    current_ramp().accent
}
/// The primary fill's hover.
pub fn accent_hover() -> Rgba {
    current_ramp().accent_hover
}
/// The accent as ink: links, focus rings, active pills.
pub fn accent_ink() -> Rgba {
    current_ramp().accent_ink
}
/// The tinted tile behind the brand mark.
pub fn accent_soft() -> Rgba {
    current_ramp().accent_soft
}
/// Text on a filled [`accent`].
pub fn on_accent() -> Rgba {
    current_ramp().on_accent
}
/// A text field's fill.
pub fn input() -> Rgba {
    current_ramp().well
}
/// A neutral pill.
pub fn chip() -> Rgba {
    current_ramp().raised
}
/// The opaque row/pill hover.
pub fn hover_bg() -> Rgba {
    current_ramp().raised
}
/// The selected row.
pub fn selected() -> Rgba {
    current_ramp().raised
}
/// The filler on the side of a split diff row with no line: the live muted
/// ink at a low alpha, laid in a 45° slash pattern. It says "no line here"
/// without reading as a change wash.
pub fn diff_fill_hatch() -> Hsla {
    Hsla {
        a: 0.28,
        ..text_faint().into()
    }
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

    /// A translucent wash over an opaque surface.
    fn over(base: Rgba, wash: Hsla) -> Rgba {
        let wash: Rgba = wash.into();
        let alpha = wash.a;
        Rgba {
            r: base.r * (1. - alpha) + wash.r * alpha,
            g: base.g * (1. - alpha) + wash.g * alpha,
            b: base.b * (1. - alpha) + wash.b * alpha,
            a: 1.,
        }
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
    fn diff_washes_keep_their_ink_readable() {
        for (gutter, content, emphasis) in [
            (DIFF_ADD_GUTTER_BG, DIFF_ADD_BG, DIFF_ADD_EMPHASIS),
            (DIFF_DEL_GUTTER_BG, DIFF_DEL_BG, DIFF_DEL_EMPHASIS),
        ] {
            let gutter_bg = over(WELL, gutter);
            let content_bg = over(WELL, content);
            let emphasis_bg = over(content_bg, emphasis);
            assert!(
                contrast(DIFF_GUTTER, gutter_bg) >= 4.5,
                "line numbers on the gutter wash are {:.2}:1",
                contrast(DIFF_GUTTER, gutter_bg)
            );
            assert!(
                contrast(TEXT_FAINT, content_bg) >= 4.5,
                "comments on the content wash are {:.2}:1",
                contrast(TEXT_FAINT, content_bg)
            );
            assert!(
                contrast(CODE_FG, content_bg) >= 4.5,
                "code on the content wash is {:.2}:1",
                contrast(CODE_FG, content_bg)
            );
            assert!(
                contrast(CODE_FG, emphasis_bg) >= 4.5,
                "emphasized code is {:.2}:1",
                contrast(CODE_FG, emphasis_bg)
            );
        }
        assert!(contrast(TEXT_MUTED, over(WELL, DIFF_HUNK_BG)) >= 4.5);
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

    /// Every palette must clear AA on every surface, and its steps must
    /// still read apart — a palette is a swap of values, not of the rules.
    #[test]
    fn every_palette_clears_aa_and_orders_its_ramp() {
        for ramp in &RAMPS {
            for ink in [ramp.text, ramp.text_dim, ramp.text_muted, ramp.accent_ink] {
                for surface in ramp.surfaces() {
                    assert!(
                        contrast(ink, surface) >= 4.5,
                        "{}: {ink:?} on {surface:?} is {:.2}:1",
                        ramp.key,
                        contrast(ink, surface)
                    );
                }
            }
            assert!(
                contrast(ramp.on_accent, ramp.accent) >= 4.5,
                "{}: text on its primary fill is {:.2}:1",
                ramp.key,
                contrast(ramp.on_accent, ramp.accent)
            );
            let ramp_steps = ramp.surfaces();
            for pair in ramp_steps.windows(2) {
                assert!(
                    luminance(pair[0]) < luminance(pair[1]),
                    "{}: {pair:?} does not lift",
                    ramp.key
                );
            }
        }
    }

    /// The shipped constants are Copper, so the live default and the published
    /// reference cannot drift apart.
    #[test]
    fn the_default_palette_is_the_copper_constants() {
        let copper = normalize_ramp(None);
        assert_eq!(copper.key, "copper");
        assert_eq!(copper.base, SURFACE);
        assert_eq!(copper.chrome, WINDOW_CHROME);
        assert_eq!(copper.panel, CARD);
        assert_eq!(copper.overlay, POPUP);
        assert_eq!(copper.well, INPUT);
        assert_eq!(copper.raised, CHIP);
        assert_eq!(copper.line, HAIRLINE);
        assert_eq!(copper.line_soft, BORDER_LIGHT);
        assert_eq!(copper.line_strong, LINE_STRONG);
        assert_eq!(copper.text, TEXT);
        assert_eq!(copper.text_dim, TEXT_DIM);
        assert_eq!(copper.text_muted, TEXT_MUTED);
        assert_eq!(copper.accent, ACCENT);
        assert_eq!(copper.accent_ink, ACCENT_INK);
        assert_eq!(copper.accent_soft, ACCENT_SOFT);
        assert_eq!(copper.on_accent, ON_ACCENT);
    }

    /// The prototype's own tokens (design/v3/source/warp.css) are the source of
    /// truth for plum and pine; this pins the port to them.
    #[test]
    fn plum_and_pine_match_the_design_prototype() {
        let plum = normalize_ramp(Some("plum"));
        assert_eq!(plum.well, rgba_hex(0x1C1A21FF));
        assert_eq!(plum.base, rgba_hex(0x26232DFF));
        assert_eq!(plum.accent, rgba_hex(0xF2907FFF));
        assert_eq!(plum.accent_ink, rgba_hex(0xF5A294FF));

        let pine = normalize_ramp(Some("pine"));
        assert_eq!(pine.well, rgba_hex(0x171C1BFF));
        assert_eq!(pine.base, rgba_hex(0x202625FF));
        assert_eq!(pine.accent, rgba_hex(0xE5C066FF));
        assert_eq!(pine.on_accent, rgba_hex(0x231B05FF));
    }

    #[test]
    fn a_saved_key_that_no_longer_exists_lands_on_copper() {
        assert_eq!(normalize_ramp(Some("chartreuse")).key, "copper");
        assert_eq!(normalize_ramp(Some("blue")).key, "blue");
    }

    #[test]
    fn switching_the_palette_moves_the_live_tokens() {
        // the process-wide switch is shared, so put it back
        let before = current_ramp().key;
        let blue = set_ramp("blue");
        assert_eq!(current_ramp().key, "blue");
        assert_eq!(text(), blue.text);
        assert_eq!(surface(), blue.base);
        assert_eq!(accent(), blue.accent);
        set_ramp(before);
        assert_eq!(current_ramp().key, before);
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
