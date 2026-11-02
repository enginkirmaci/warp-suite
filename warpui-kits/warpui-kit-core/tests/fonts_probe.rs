//! The bundled faces must load, and the kit mapping must carry the resolved
//! families and the rem base: every kit padding is in rem, so the ladder's body
//! step sitting under the old 12px must not shrink it.

use gpui::{TestAppContext, px};
use gpui_kit::component::Theme;
use warpui_kit_core::{AccentColors, apply_kit_theme, font_tokens, fonts, palette};

fn apply(tokens: warpui_kit_core::FontTokens, cx: &mut gpui::App) {
    apply_kit_theme(
        AccentColors::default_colors(),
        &tokens,
        palette::BACKGROUND,
        cx,
    );
}

#[gpui_kit::gpui::test]
fn the_bundled_faces_load(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    assert_eq!(fonts::bundled_fonts().len(), 9);
    cx.update(|cx| {
        cx.text_system()
            .add_fonts(fonts::bundled_fonts())
            .expect("the bundled TTFs load");
    });
}

#[test]
fn the_getters_read_the_ladder() {
    let tokens = fonts::active_tokens();
    assert_eq!(fonts::font_micro(), tokens.font_micro);
    assert_eq!(fonts::font_mono_meta(), tokens.font_mono_meta);
    assert_eq!(fonts::font_row_meta(), tokens.font_row_meta);
    assert_eq!(fonts::font_control(), tokens.font_control);
    assert_eq!(fonts::font_label(), tokens.font_label);
    assert_eq!(fonts::font_sub(), tokens.font_sub);
    assert_eq!(fonts::font_meta(), tokens.font_meta);
    assert_eq!(fonts::font_body(), tokens.font_body);
    assert_eq!(fonts::font_heading(), tokens.font_heading);
    assert_eq!(fonts::font_brand(), tokens.font_brand);
    assert_eq!(fonts::font_name(), tokens.font_name);
    assert_eq!(fonts::font_note(), tokens.font_note);
    assert_eq!(fonts::font_list_title(), tokens.font_list_title);
    assert_eq!(fonts::font_screen_title(), tokens.font_screen_title);
    assert_eq!(fonts::icon_tiny(), tokens.icon_tiny);
    assert_eq!(fonts::icon_small(), tokens.icon_small);
    assert_eq!(fonts::icon_medium(), tokens.icon_medium);
    assert_eq!(fonts::icon_large(), tokens.icon_large);
}

#[gpui_kit::gpui::test]
fn the_kit_mapping_carries_the_families(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| {
        apply(font_tokens(None, 1.0), cx);
        let t = Theme::global(cx);
        assert_eq!(t.font_family.as_ref(), fonts::ui_font());
        assert_eq!(t.mono_font_family.as_ref(), fonts::code_font());
    });
}

#[gpui_kit::gpui::test]
fn the_rem_base_rides_the_factor_from_sixteen(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| {
        apply(font_tokens(None, 1.0), cx);
        assert_eq!(Theme::global(cx).font_size, px(16.));

        apply(font_tokens(None, 2.0), cx);
        assert_eq!(Theme::global(cx).font_size, px(32.));
    });
}
