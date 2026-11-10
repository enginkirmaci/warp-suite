//! The kit theme must carry the family palette after apply_kit_theme — a
//! Light→Dark mode flip inside the color-edit closure makes gpui-component
//! reload its stock dark theme over the overrides — and the resolved families.

use gpui::TestAppContext;
use warp_station::theme;

#[gpui_kit::gpui::test]
fn palette_survives_the_mode_flip(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(theme::apply_kit_theme);

    cx.update(|cx| {
        use gpui_kit::component::{Theme, ThemeMode};
        let t = Theme::global(cx);
        assert_eq!(t.mode, ThemeMode::Dark);
        assert_eq!(t.background, gpui::Hsla::from(theme::background()));
        assert_eq!(t.popover, gpui::Hsla::from(theme::popup()));
        assert_eq!(t.primary, gpui::Hsla::from(theme::accent()));
        assert_eq!(t.link, gpui::Hsla::from(theme::blue_bright()));
    });

    // Gray is a full palette, not a surface-only swap: the steel accent moves
    // the kit's primary and link tokens with the ramp.
    cx.update(|cx| {
        warpui_kit_core::palette::set_ramp("gray");
        theme::apply_kit_theme(cx);
    });
    cx.update(|cx| {
        use gpui_kit::component::Theme;
        let gray = warpui_kit_core::palette::normalize_ramp(Some("gray"));
        let t = Theme::global(cx);
        assert_eq!(t.background, gpui::Hsla::from(gray.base));
        assert_eq!(t.primary, gpui::Hsla::from(gray.accent));
        assert_eq!(t.link, gpui::Hsla::from(gray.accent_ink));
    });

    // Leave the process on the default ramp for any later test in this binary.
    cx.update(|cx| {
        warpui_kit_core::palette::set_ramp("copper");
        theme::apply_kit_theme(cx);
    });
}

#[gpui_kit::gpui::test]
fn the_kit_theme_carries_the_family_faces(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(theme::apply_kit_theme);

    cx.update(|cx| {
        use gpui_kit::component::Theme;

        // kit text, the live editor and the diff tokens all read these
        let t = Theme::global(cx);
        assert_eq!(t.font_family.as_ref(), theme::ui_font());
        assert_eq!(t.mono_font_family.as_ref(), theme::code_font());
        assert!(std::sync::Arc::ptr_eq(
            &t.highlight_theme,
            &theme::highlight_theme()
        ));
    });
}
