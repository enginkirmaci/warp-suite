//! Maps the palette onto gpui-kit's shadcn-style theme tokens so kit
//! components (Input/Textarea/TextView/popovers) render in the family colors.
//! The accent family and font tokens are parameters so a settings-page preset
//! swap can re-run the mapping (`Theme::update` refreshes every window). Call
//! after [`gpui_kit::init`], before opening windows.

use crate::fonts::{self, FontTokens};
use crate::palette::{self, AccentColors};
use gpui::App;

/// The window-chrome color the kit `sidebar` token paints (titlebar/sidebar).
/// The clients disagree on the step — DevTools blends it into the page,
/// warp-agent keeps it on the chrome step — so the caller names theirs.
pub fn apply_kit_theme(
    accent: AccentColors,
    fonts_tokens: &FontTokens,
    sidebar: gpui::Rgba,
    cx: &mut App,
) {
    use gpui_kit::component::{Theme, ThemeMode};

    fonts::resolve_fonts(cx);
    // A mode flip inside the color-edit closure makes the kit reload the
    // registered theme's colors AFTER the closure (Theme::update docs),
    // wiping every override — flip first, then edit.
    Theme::update(cx, |t| t.mode = ThemeMode::Dark);
    Theme::update(cx, |t| {
        t.background = palette::surface().into();
        t.foreground = palette::text().into();
        t.primary = accent.accent.into();
        t.primary_foreground = accent.on.into();
        t.secondary = palette::raised().into();
        t.secondary_foreground = palette::text().into();
        t.muted = palette::well().into();
        t.muted_foreground = palette::text_muted().into();
        t.accent = palette::HOVER;
        t.accent_foreground = palette::text().into();
        t.danger = palette::DANGER.into();
        t.danger_foreground = palette::ON_DANGER.into();
        t.border = palette::hairline().into();
        // the kit paints an input's border with `input` and its fill with
        // `input` mixed over the surface, so a fill-coloured token made the
        // border vanish on popups; the hairline gives fields a real edge
        t.input = palette::hairline().into();
        t.ring = accent.text.into();
        t.selection = palette::selection(&accent).into();
        t.caret = palette::text().into();
        t.link = accent.text.into();
        t.popover = palette::popup().into();
        t.popover_foreground = palette::text().into();
        t.sidebar = sidebar.into();
        t.sidebar_foreground = palette::text().into();
        t.sidebar_border = palette::hairline().into();

        // controls on 6px corners, panels on 8 — one radius per class
        t.radius = gpui::px(6.);
        t.radius_lg = gpui::px(8.);

        t.font_family = gpui::SharedString::from(fonts::ui_font());
        t.mono_font_family = gpui::SharedString::from(fonts::code_font());
        // the rem base: kit components size text/padding in rem — ride the
        // same factor as our own tokens
        let factor = fonts_tokens.font_body / fonts::BASE_TOKENS.font_body;
        t.font_size = gpui::px(16. * factor);
        t.mono_font_size = gpui::px(13. * factor);

        t.highlight_theme = palette::highlight_theme();

        // tables blend into the page: no canvas of their own, no zebra, no
        // visible grid (the color fields reconcile into the tokens)
        t.table = palette::surface().into();
        t.table_head = palette::surface().into();
        t.table_head_foreground = palette::text_muted().into();
        t.table_even = palette::surface().into();
        t.table_hover = palette::HOVER;
        t.table_active = palette::tint(accent.accent);
        t.table_active_border = accent.accent.into();
        t.table_row_border = palette::border_light().into();
        t.table_foot = palette::surface().into();

        // scrollbars ride the same ramp: no stock light-grey thumb
        t.tokens.scrollbar = (gpui::Hsla {
            a: 0.0,
            ..gpui::Hsla::from(palette::text_muted())
        })
        .into();
        t.tokens.scrollbar_thumb = gpui::Hsla::from(palette::line_strong()).into();
        t.tokens.scrollbar_thumb_hover = gpui::Hsla::from(palette::text_muted()).into();
    });
}
