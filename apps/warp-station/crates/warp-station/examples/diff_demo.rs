//! Standalone visual fixture for the diff renderer: renders fixed patches in
//! unified and split so the rows, bands, fillers and syntax colors can be
//! screenshotted without touching app state.
//! `cargo run -p warp_station --example diff_demo`

use gpui::prelude::*;
use gpui::{
    Bounds, Context, Render, TitlebarOptions, Window, WindowBounds, WindowOptions, div, px, size,
};
use gpui_platform::application;
use warp_station::theme;
use warpui_kit_diff::{DiffMode, DiffOptions, render_diff_with};

const RUST_PATCH: &str = r#"diff --git a/crates/warp-station-core/src/markdown_live.rs b/crates/warp-station-core/src/markdown_live.rs
index 7c3f9a1..b2e4d55 100644
--- a/crates/warp-station-core/src/markdown_live.rs
+++ b/crates/warp-station-core/src/markdown_live.rs
@@ -533,7 +533,9 @@ fn flatten_pairs(
             next_ok && prev_ok
     }

-    /// Emit the runs of an emphasis forest: segments between nested pairs take
+    /// Emit the runs of an emphasis forest: segments between nested pairs take
+    /// their enclosing pair's emphasis; markers take Marker. Segments only ever
+    /// cover the non-atomic gaps, so atomic spans (code/links) stay intact.
     fn flatten_pairs(
         pairs: &[EmphPair],
         region: Range<usize>,
@@ -544,17 +546,18 @@ fn flatten_pairs(
     ) {
-        // children of this region: pairs whose open marker starts inside it and
-        // that are not contained in another such pair
+        // children of this region: pairs whose markers start inside it and that
+        // are not contained in another such pair
         let mut children: Vec<&EmphPair> = pairs
             .iter()
             .filter(|p| p.open.start >= region.start && p.close.end <= region.end)
             .collect();
         children.sort_by_key(|p| p.open.start);

         let mut cursor = region.start;
-        for pair in children {
-            if pair.open.start > cursor {
-                runs.push((cursor..pair.open.start, base));
+        let mut segment = |start: usize, end: usize, kind: MarkdownLiveKind, runs: &mut Vec<_>| {
+            for gap in gaps {
+                let from = gap.start.max(start);
+                let to = gap.end.min(end);
+                if from < to {
+                    runs.push((from..to, kind));
+                }
+            }
+        };
+        for pair in children {
+            segment(cursor, pair.open.start, base, runs);
             runs.push((pair.open.clone(), MarkdownLiveKind::Marker));
             let nested_kind = MarkdownLiveKind::Emphasis {
                 bold: pair.bold,
@@ -562,11 +563,9 @@ fn flatten_pairs(
                 strike: pair.strike,
             };
-            flatten_pairs(pairs, pair.content.clone(), nested_kind, runs);
+            flatten_pairs(pairs, pair.content.clone(), nested_kind, gaps, runs);
             runs.push((pair.close.clone(), MarkdownLiveKind::Marker));
             cursor = pair.close.end;
         }
-        if cursor < region.end {
-            runs.push((cursor..region.end, base));
-        }
+        segment(cursor, region.end, base, runs);
     }

 #[cfg(test)]
 mod tests {
"#;

const PY_PATCH: &str = r#"diff --git a/scripts/plot.py b/scripts/plot.py
index 1111111..2222222 100644
--- a/scripts/plot.py
+++ b/scripts/plot.py
@@ -10,8 +10,10 @@ def load(path):
     data = json.load(f)
-    if "sparse" in data:
-        return None
+    sparse = data.get("sparse", 0.75)
+    print(f"sparse={sparse}")
     return normalize(data["rows"])
"#;

struct DiffDemo;

impl Render for DiffDemo {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut card = |patch: &'static str, mode: DiffMode| {
            div()
                .w_full()
                .rounded_md()
                .bg(theme::card())
                .border_1()
                .border_color(theme::border_light())
                .py_2()
                .overflow_hidden()
                .child(render_diff_with(
                    patch,
                    None,
                    DiffOptions { mode, wrap: false },
                    theme::font_control(),
                    window,
                    cx,
                ))
        };
        div()
            .size_full()
            .bg(theme::background())
            .p_4()
            .flex()
            .flex_col()
            .gap_3()
            .child(card(RUST_PATCH, DiffMode::Unified))
            .child(card(PY_PATCH, DiffMode::Unified))
            .child(card(PY_PATCH, DiffMode::Split))
    }
}

fn main() {
    application().run(|cx| {
        gpui_kit::init(cx);
        // the shared text size, so the fixture draws what the app draws at the
        // owner's setting (the diff follows it through `font_control()`)
        warpui_kit_core::adopt_text_size(None);
        theme::apply_kit_theme(cx);
        warpui_kit_core::follow_ui_scale(cx, theme::apply_kit_theme);
        let bounds = Bounds::centered(None, size(px(1000.), px(1050.)), cx);
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("diff demo".into()),
                    appears_transparent: true,
                    ..Default::default()
                }),
                ..Default::default()
            },
            cx,
            |_window, cx| cx.new(|_cx| DiffDemo),
        )
        .unwrap();
        cx.activate(true);
    });
}
