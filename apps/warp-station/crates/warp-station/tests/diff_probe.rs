//! Headless probe for the diff renderer's fonts: the diff `StyledText` must
//! be shaped at the requested detail size in the code font. GPUI shapes text
//! at the *ambient* text style, so `render_diff` pins both on its container;
//! without that the block ignores its size and wrapped segments fall back to
//! the UI font.

use gpui::prelude::*;
use gpui::{
    AbsoluteLength, Context, HighlightStyle, Pixels, Render, StyledText, TestAppContext, Window,
    div, px,
};
use std::cell::RefCell;
use std::rc::Rc;
use warp_station::theme;

const DETAIL_SIZE: f32 = 10.;

struct DiffProbe {
    layout: Rc<RefCell<Option<gpui::TextLayout>>>,
    ambient: Rc<RefCell<Option<gpui::TextStyle>>>,
    text: &'static str,
    width: Pixels,
}

impl DiffProbe {
    fn new(text: &'static str, width: Pixels) -> Self {
        Self {
            layout: Rc::new(RefCell::new(None)),
            ambient: Rc::new(RefCell::new(None)),
            text,
            width,
        }
    }
}

impl Render for DiffProbe {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        // exactly what render_diff builds after the fix: ambient code font +
        // detail size on the container, colour/background-only highlights
        let mut style = window.text_style();
        style.font_family = theme::code_font().into();
        style.font_size = AbsoluteLength::Pixels(px(DETAIL_SIZE));
        let highlight = HighlightStyle {
            color: Some(theme::SUCCESS_BRIGHT.into()),
            background_color: Some(theme::DIFF_ADD_BG),
            ..Default::default()
        };
        let styled = StyledText::new(self.text)
            .with_default_highlights(&style, [(0..self.text.len(), highlight)]);
        *self.layout.borrow_mut() = Some(styled.layout().clone());
        *self.ambient.borrow_mut() = Some(window.text_style());
        div()
            .w(self.width)
            .font_family(gpui::SharedString::from(theme::code_font()))
            .text_size(px(DETAIL_SIZE))
            .child(styled)
    }
}

/// A long single-line added diff row shaped in a narrow box: the block must
/// keep the requested size and the code font across wrapped segments.
#[gpui::test]
fn wrapped_segments_keep_the_code_font(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(theme::apply_kit_theme);

    // gutter + a long added line, mid-run wraps
    let text = "    1  + // \"type\" cannot be platform-specific in launch.json, so each OS gets its own config; F5 reuses the last-selected config per machine.";
    let probe = DiffProbe::new(text, px(420.));
    let layout_handle = probe.layout.clone();
    let ambient_handle = probe.ambient.clone();
    let (_view, cx) = cx.add_window_view(|_, _| probe);
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    // resolve the fonts with the same Font structs the runs carry; the
    // headless harness stubs the font database, so ids only serve as logging
    let ambient = ambient_handle
        .borrow()
        .clone()
        .expect("the probe recorded the ambient style");
    let (code_id, ui_id) = cx.update(|_, cx| {
        let text_system = cx.text_system().clone();
        let mut code_font = ambient.font();
        code_font.family = theme::code_font().into();
        let mut ui_font = ambient.font();
        ui_font.family = theme::ui_font().into();
        (
            text_system.resolve_font(&code_font),
            text_system.resolve_font(&ui_font),
        )
    });

    let layout = layout_handle
        .borrow()
        .clone()
        .expect("the probe's StyledText must have been laid out");
    let lines = layout.line_layouts();
    println!(
        "lines: {}, code id {code_id:?}, ui id {ui_id:?}",
        lines.len()
    );
    assert!(
        lines.iter().any(|line| !line.wrap_boundaries().is_empty()),
        "the probe text must wrap in the narrow box"
    );

    let mut runs = 0usize;
    let mut first_font: Option<gpui::FontId> = None;
    for (ix, line) in lines.iter().enumerate() {
        println!(
            "  line {ix}: size={:?} wraps={} runs={}",
            line.font_size(),
            line.wrap_boundaries().len(),
            line.runs().len(),
        );
        assert_eq!(
            line.font_size(),
            px(DETAIL_SIZE),
            "line {ix} must be shaped at the requested detail size",
        );
        for run in line.runs() {
            runs += 1;
            // a font fallback would split the run into several font ids
            match first_font {
                None => first_font = Some(run.font_id),
                Some(id) => assert_eq!(
                    run.font_id, id,
                    "line {ix} mixed font runs ({:?} vs {id:?})",
                    run.font_id
                ),
            }
        }
    }
    assert!(runs > 0, "the text must shape into at least one run");
}
