//! warpui-kit-core: the foundation crate of the warpui-kit family.
//!
//! One palette, one font stack, one set of shared control helpers for the
//! desktop clients built on [gpui-kit](https://crates.io/crates/gpui-kit):
//!
//! - [`palette`] — the switchable ramps (Copper, Plum, Pine, Blue), one hue
//!   per entity, the accent preset system and the syntax highlight theme
//! - [`fonts`] — the bundled Inter and JetBrains Mono faces, font resolution
//!   and the type scale
//! - [`scale`] — the UI scale every app shares: one text size setting and
//!   control, times the desktop text scaling
//! - [`shell`] — the one window shape: rail, tab strip, list and detail
//! - [`motion`] — durations and the spinner
//! - [`kit`] — maps the palette onto gpui-kit's shadcn-style theme tokens
//! - [`about`] — the About screen every app carries above Settings: the
//!   family table, the marks, and the Build/Legal cards
//! - [`a11y`] — the role/label trait every hand-rolled control passes through
//! - [`mod@icon`] — tinted svg icon builders (asset path or inline source)
//! - [`bootstrap`] — the one app startup: kit init, shared scale and palette,
//!   follow hooks, first theme pass, reduced motion
//! - [`pump`] — the net-event drain loop both store-driven apps ran by hand
//! - [`controls`] — the one button look, menu row, popover card and fold body
//! - [`field`] — the one text box: `.field`, `.area` and the `.kbd` chip
//! - [`media`] — attachment mime guesses and inline images from bytes
//! - [`tooltip`] and [`tooltip_overlay`] — the two tooltip mechanisms
//! - [`text_menu`] — the right-click Copy menu for selectable read-only text
//!
//! The component crates (`warpui-kit-diff`, `warpui-kit-markdown`,
//! `warpui-kit-picker`) build on this crate so every consumer renders in the
//! same family.

pub mod a11y;
pub mod about;
pub mod bootstrap;
pub mod controls;
pub mod field;
pub mod fonts;
pub mod icon;
pub mod kit;
#[cfg(feature = "media")]
pub mod media;
pub mod motion;
pub mod palette;
pub mod pump;
pub mod scale;
pub mod settings;
pub mod shell;
pub mod text_menu;
pub mod tooltip;
pub mod tooltip_overlay;

pub use a11y::A11y;
pub use controls::{BtnKind, button, fold_body, menu_row, popover_card};
pub use fonts::{
    CODE_FONT, FONT_LARGE, FONT_LARGER, FONT_NORMAL, FONT_UI, FontTokens, active_tokens, code_font,
    font_factor, font_tokens, normalize_font_scale, resolve_fonts, set_font_scale,
    set_system_factor, system_factor, ui_font,
};
pub use icon::{icon, icon_data};
pub use kit::apply_kit_theme;
#[cfg(feature = "media")]
pub use media::{decode_png_render, mime_for_path};
pub use palette::{
    ACCENT_PRESETS, AccentColors, AccentPreset, highlight_theme, normalize_accent, rgba_hex,
    rgba_u32, selection, tint,
};
pub use scale::{
    adopt_palette, adopt_text_size, follow_palette, follow_ui_scale, palette_control, palette_key,
    set_palette, set_text_size, text_size_control,
};
