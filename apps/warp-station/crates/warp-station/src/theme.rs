//! The Warp Station theme: the family palette, type and scale from
//! `warpui-kit-core`, re-exported under the app's `theme::` paths, plus the
//! app's layout constants.

// every shared font step and color const stays reachable as `theme::X`
pub use warpui_kit_core::fonts::*;
// the fixed semantic colors stay constants (they never follow the palette),
// and the accent preset system rides along
pub use warpui_kit_core::palette::{
    ACCENT_BORDER, ADD, AHEAD, AMBER_BRIGHT, AccentColors, BEHIND, BRANCH, CHANGES, DANGER, DEL,
    DIFF_ADD_BG, DIFF_DEL_BG, GHOST_BORDER_HOVER, ISSUE, ON_DANGER, PR, SCRIM, SUCCESS_BRIGHT,
    SUCCESS_DEEP, WARN, highlight_theme, tint,
};
// ---- gpui-kit theme mapping ------------------------------------------------

/// Map the family palette onto gpui-kit's shadcn-style theme tokens. Call after
/// [`gpui_kit::init`], before opening windows (`Theme::update` refreshes every
/// window, so a settings save can re-run it).
pub fn apply_kit_theme(cx: &mut gpui::App) {
    warpui_kit_core::apply_kit_theme(
        AccentColors::default_colors(),
        &active_tokens(),
        surface(),
        cx,
    );
}

// ---- layout ----------------------------------------------------------------
// Shared chrome sizes. Views and the window position their overlays from
// these, so a change here keeps menus and popups anchored.

/// Custom titlebar height (window buttons + search field live here).
pub const TITLEBAR_H: f32 = warpui_kit_core::shell::TITLE_BAR_HEIGHT;

// ---- the live palette ------------------------------------------------------
// Every surface comes through here, so switching the palette in one settings
// page moves all of them. These were constants; they are functions now for
// exactly that reason, and the names stay the apps' own vocabulary — the
// readers themselves are the kit's, re-exported.

pub use warpui_kit_core::palette::{
    accent, accent_hover, accent_ink, accent_soft, border_light, hairline, line_strong, on_accent,
    panel, popup, raised, surface, text, text_dim, text_muted, well, window_chrome,
};

// the app's older spellings, one-liners over the readers above
pub fn background() -> gpui::Rgba {
    surface()
}
pub fn blue_bright() -> gpui::Rgba {
    accent_ink()
}
pub fn card() -> gpui::Rgba {
    panel()
}
pub fn chip() -> gpui::Rgba {
    raised()
}
pub fn ghost_bg() -> gpui::Rgba {
    panel()
}
pub fn ghost_border() -> gpui::Rgba {
    line_strong()
}
pub fn hover_bg() -> gpui::Rgba {
    raised()
}
pub fn key_border() -> gpui::Rgba {
    line_strong()
}
pub fn muted_icon() -> gpui::Rgba {
    text_muted()
}
pub fn overlay() -> gpui::Rgba {
    popup()
}
pub fn row_line() -> gpui::Rgba {
    border_light()
}
pub fn rows_rule() -> gpui::Rgba {
    border_light()
}
pub fn search_bg() -> gpui::Rgba {
    well()
}
pub fn search_border() -> gpui::Rgba {
    hairline()
}
pub fn seg_active() -> gpui::Rgba {
    raised()
}
pub fn seg_active_fg() -> gpui::Rgba {
    text()
}
pub fn seg_bg() -> gpui::Rgba {
    well()
}
pub fn star_muted() -> gpui::Rgba {
    text_muted()
}
pub fn text_faint() -> gpui::Rgba {
    text_muted()
}
pub fn topbar_bg() -> gpui::Rgba {
    window_chrome()
}
