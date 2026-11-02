//! Two things keep the palette switch honest across the kit.
//!
//! The kit's own chrome must read the live palette: `popover_card`, the
//! text-size trough, the shell's rows and the mapped shadcn tokens are drawn
//! by core, so a file still naming Copper's constants would paint a copper
//! menu inside a blue app. gpui's headless context reports bounds, not painted
//! colours, so that is checked at the source (the files are `include_str!`d,
//! which keeps it a compile-time dependency and not a path guess).
//!
//! The functional half — a control still laying out after a switch — is here
//! too, because a palette change must not disturb the frame.

use gpui::prelude::*;
use gpui::{Context, IntoElement, Render, TestAppContext, Window};
use warpui_kit_core::{palette, text_size_control};

/// The ramp tokens, which every surface must read live.
const RAMP_TOKENS: [&str; 17] = [
    "SURFACE",
    "BACKGROUND",
    "CARD",
    "PANEL",
    "WINDOW_CHROME",
    "TOPBAR_BG",
    "POPUP",
    "OVERLAY",
    "INPUT",
    "WELL",
    "CHIP",
    "RAISED",
    "HAIRLINE",
    "BORDER_LIGHT",
    "LINE_STRONG",
    "TEXT",
    "TEXT_DIM",
];

/// The kit's drawing code, as it ships.
const KIT_SOURCES: [(&str, &str); 5] = [
    ("core/controls.rs", include_str!("../src/controls.rs")),
    ("core/shell.rs", include_str!("../src/shell.rs")),
    ("core/scale.rs", include_str!("../src/scale.rs")),
    ("core/kit.rs", include_str!("../src/kit.rs")),
    (
        "picker/lib.rs",
        include_str!("../../warpui-kit-picker/src/lib.rs"),
    ),
];

/// A doc comment or a test may legitimately name a constant (the Copper
/// reference test does); a drawing call may not.
fn drawing_lines(source: &str) -> impl Iterator<Item = &str> {
    source.lines().filter(|line| {
        let t = line.trim_start();
        !t.starts_with("//")
            && !t.starts_with("///")
            && !t.starts_with("*")
            && !t.starts_with("#[cfg(test)]")
    })
}

#[test]
fn kit_drawing_code_reads_the_live_palette() {
    let mut offenders = Vec::new();
    for (name, source) in KIT_SOURCES {
        for line in drawing_lines(source) {
            for token in RAMP_TOKENS {
                let needle = format!("palette::{token}");
                // `palette::TEXT_MUTED` must not match a search for `TEXT`
                if line.contains(&needle)
                    && !line.contains(&format!("{needle}_"))
                    && !line.contains(&format!("palette::{}()", token.to_lowercase()))
                {
                    offenders.push(format!("{name}: {}", line.trim()));
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "these draw a fixed palette step instead of the live one:\n{}",
        offenders.join("\n")
    );
}

struct Chrome;

impl Render for Chrome {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        warpui_kit_core::popover_card().child(text_size_control())
    }
}

#[gpui_kit::gpui::test]
fn a_switch_moves_the_tokens_and_the_control_still_lays_out(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    palette::set_ramp("blue");
    let blue = palette::normalize_ramp(Some("blue"));
    assert_eq!(palette::popup(), blue.overlay, "the popover step follows");
    assert_eq!(palette::well(), blue.well, "the field/trough step follows");
    assert_ne!(palette::popup(), palette::POPUP, "and it is not Copper");

    let (_root, cx) = cx.add_window_view(|_, _| Chrome);
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    assert!(
        cx.debug_bounds("text-size-normal").is_some(),
        "the trough still lays out after the switch"
    );

    palette::set_ramp("copper");
    assert_eq!(palette::popup(), palette::POPUP, "back to Copper");
}
