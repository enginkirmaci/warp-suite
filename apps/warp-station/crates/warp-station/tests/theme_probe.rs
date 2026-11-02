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
        assert_eq!(t.background, gpui::Hsla::from(theme::BACKGROUND));
        assert_eq!(t.popover, gpui::Hsla::from(theme::POPUP));
        assert_eq!(t.primary, gpui::Hsla::from(theme::ACCENT));
        assert_eq!(t.link, gpui::Hsla::from(theme::BLUE_BRIGHT));
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
