//! The Warp Station theme: the family palette, type and scale from
//! `warpui-kit-core`, re-exported under the app's `theme::` paths, plus the
//! app's layout constants.

// every shared font step and color const stays reachable as `theme::X`
pub use warpui_kit_core::fonts::*;
// the fixed semantic colors stay constants (they never follow the palette),
// and the accent preset system rides along
// the fixed semantic colors stay constants (they never follow the palette),
// and the accent preset system rides along
pub use warpui_kit_core::palette::{
    ACCENT_BORDER, ADD, AHEAD, AMBER_BRIGHT, AccentColors, BEHIND, BRANCH, CHANGES, DANGER, DEL,
    DIFF_ADD_BG, DIFF_DEL_BG, GHOST_BORDER_HOVER, ISSUE, ON_DANGER, PR, SCRIM, SUCCESS_BRIGHT,
    SUCCESS_DEEP, WARN, current_ramp, highlight_theme, tint,
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
// exactly that reason, and the names stay the apps' own vocabulary.

/// The primary fill.
pub fn accent() -> gpui::Rgba {
    current_ramp().accent
}
/// The primary fill's hover.
pub fn accent_hover() -> gpui::Rgba {
    current_ramp().accent_hover
}
/// The accent as ink: links, focus rings, active pills.
pub fn accent_ink() -> gpui::Rgba {
    current_ramp().accent_ink
}
/// The tinted tile behind the brand mark.
pub fn accent_soft() -> gpui::Rgba {
    current_ramp().accent_soft
}
/// Quiet dividers.
pub fn border_light() -> gpui::Rgba {
    current_ramp().line_soft
}
/// The default separator.
pub fn hairline() -> gpui::Rgba {
    current_ramp().line
}
/// Control borders.
pub fn line_strong() -> gpui::Rgba {
    current_ramp().line_strong
}
/// Text on a filled `accent()`.
pub fn on_accent() -> gpui::Rgba {
    current_ramp().on_accent
}
/// A card, a tile, the detail pane.
pub fn panel() -> gpui::Rgba {
    current_ramp().panel
}
/// Menus, dialogs and tooltips.
pub fn popup() -> gpui::Rgba {
    current_ramp().overlay
}
/// The selected row, hover fills, active segments.
pub fn raised() -> gpui::Rgba {
    current_ramp().raised
}
/// The window canvas.
pub fn surface() -> gpui::Rgba {
    current_ramp().base
}
/// Names and values.
pub fn text() -> gpui::Rgba {
    current_ramp().text
}
/// Secondary copy.
pub fn text_dim() -> gpui::Rgba {
    current_ramp().text_dim
}
/// Timestamps and hints.
pub fn text_muted() -> gpui::Rgba {
    current_ramp().text_muted
}
/// Fields, diffs and code: the recessed step.
pub fn well() -> gpui::Rgba {
    current_ramp().well
}
/// The titlebar, the rail and the chrome.
pub fn window_chrome() -> gpui::Rgba {
    current_ramp().chrome
}

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
