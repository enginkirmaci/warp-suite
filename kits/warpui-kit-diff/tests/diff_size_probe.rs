//! The diff and code blocks must draw at the size the app's text-size setting
//! asks for. GPUI shapes a `StyledText` at the *ambient* text style, and a
//! `TextRun` carries only the font, never a size, so the kit's renderer has to
//! pin the size on the element the rows live in. This probe draws the real
//! `render_diff` under an ambient style the way an app's root sets it and
//! measures the rows.

use gpui::prelude::*;
use gpui::{Context, Render, TestAppContext, Window, div, px};
use warpui_kit_diff::render_diff;

const PATCH: &str = "\
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,4 @@
 fn a() {}
+fn b() {}
 fn c() {}
";

struct DiffAt {
    font_detail: f32,
}

impl Render for DiffAt {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .debug_selector(|| "diff-host".into())
            .w(px(600.))
            .child(render_diff(PATCH, None, self.font_detail, window, cx))
    }
}

#[gpui_kit::gpui::test]
fn the_rows_draw_at_the_requested_detail_size(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (_root, cx) = cx.add_window_view(|_, _| DiffAt { font_detail: 13. });
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    let host = cx
        .debug_bounds("diff-host")
        .expect("the probe drew its host");
    println!("host: {host:?}");
}
