//! The Warp Station theme: the family palette, type and scale from
//! `warpui-kit-core`, re-exported under the app's `theme::` paths, plus the
//! app's layout constants.

// every shared font step and color const stays reachable as `theme::X`
pub use warpui_kit_core::fonts::*;
pub use warpui_kit_core::palette::*;

// ---- gpui-kit theme mapping ------------------------------------------------

/// Map the family palette onto gpui-kit's shadcn-style theme tokens. Call after
/// [`gpui_kit::init`], before opening windows (`Theme::update` refreshes every
/// window, so a settings save can re-run it).
pub fn apply_kit_theme(cx: &mut gpui::App) {
    warpui_kit_core::apply_kit_theme(
        AccentColors::default_colors(),
        &active_tokens(),
        BACKGROUND,
        cx,
    );
}

// ---- layout ----------------------------------------------------------------
// Shared chrome sizes. Views and the window position their overlays from
// these, so a change here keeps menus and popups anchored.

/// Custom titlebar height (window buttons + search field live here).
pub const TITLEBAR_H: f32 = warpui_kit_core::shell::TITLE_BAR_HEIGHT;
