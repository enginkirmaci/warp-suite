//! The aligned view's layout rules: the opt-in columns appear only behind
//! their toggles, padding keeps the body's height honest while the caller
//! windows its rows, and the token pass changes no row geometry.

use gpui::prelude::*;
use gpui::{AnyElement, Bounds, Context, Pixels, Render, TestAppContext, Window, div, px};
use warpui_kit_core::{AccentColors, active_tokens, apply_kit_theme, palette};
use warpui_kit_diff::{
    AlignedOptions, AlignedRow, AlignedSide, aligned_pane_head, aligned_pick_box, render_aligned,
    row_height, visible_window,
};

/// One row spec: the left, right and base line indexes into the side
/// buffers, and whether the row is part of a change.
type HostRow = (Option<u32>, Option<u32>, Option<u32>, bool);

struct AlignedHost {
    rows: Vec<HostRow>,
    left: Vec<&'static str>,
    right: Vec<&'static str>,
    base: Vec<&'static str>,
    lead: bool,
    base_column: bool,
    syntax: Option<&'static str>,
    padding: (usize, usize),
}

fn host(rows: Vec<HostRow>) -> AlignedHost {
    AlignedHost {
        rows,
        left: vec!["fn a() {}", "fn b() { let x = 1; }", "fn c() {}"],
        right: vec!["fn a() {}", "fn b() { let y = 2; }", "fn c() {}"],
        base: vec!["fn a() {}", "fn b() {}", "fn c() {}"],
        lead: false,
        base_column: false,
        syntax: None,
        padding: (0, 0),
    }
}

impl Render for AlignedHost {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let rows: Vec<AlignedRow> = self
            .rows
            .iter()
            .map(|(left, right, base, changed)| AlignedRow {
                left: left.map(|index| (index + 1, self.left[index as usize])),
                right: right.map(|index| (index + 1, self.right[index as usize])),
                base: base.map(|index| (index + 1, self.base[index as usize])),
                changed: *changed,
            })
            .collect();
        let mut options = AlignedOptions::new(13.);
        options.padding = self.padding;
        options.syntax = self.syntax;
        options.base = self.base_column;
        let mut lead = move |side: AlignedSide, line: Option<u32>| -> Option<AnyElement> {
            let name = match side {
                AlignedSide::Left => "left",
                AlignedSide::Right => "right",
            };
            Some(
                div()
                    .debug_selector(move || format!("probe-lead-{name}-{}", line.unwrap_or(0)))
                    .into_any_element(),
            )
        };
        if self.lead {
            let lead_ref: &mut dyn FnMut(AlignedSide, Option<u32>) -> Option<AnyElement> =
                &mut lead;
            options.lead = Some(lead_ref);
        }
        let body = render_aligned(&rows, options, window, cx);
        let mut head = vec![("Current", self.left.len())];
        if self.base_column {
            head.push(("Base", self.base.len()));
        }
        head.push(("Incoming", self.right.len()));
        div()
            .size_full()
            .flex()
            .flex_col()
            .w(px(600.))
            .flex_shrink_0()
            .child(aligned_pane_head(&head, self.lead))
            .child(body)
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(aligned_pick_box(true))
                    .child(aligned_pick_box(false)),
            )
    }
}

fn apply_theme(cx: &mut gpui::App) {
    apply_kit_theme(
        AccentColors::default_colors(),
        &active_tokens(),
        palette::surface(),
        cx,
    );
}

fn bounds(vcx: &mut gpui::VisualTestContext, selector: &'static str) -> Bounds<Pixels> {
    vcx.debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} was not drawn"))
}

fn height_is(value: Pixels, rows: usize) {
    let expected = f32::from(row_height(13.)) * rows as f32;
    // layout rounds each row to the whole pixel, so a 20.8px row lands at 21
    assert!(
        (f32::from(value) - expected).abs() <= rows as f32 * 0.5,
        "height {} should hold {rows} rows ({expected})",
        f32::from(value)
    );
}

fn three_rows() -> Vec<HostRow> {
    vec![
        (Some(0), Some(0), Some(0), false),
        (Some(1), Some(1), Some(1), true),
        (Some(2), Some(2), Some(2), false),
    ]
}

#[gpui_kit::gpui::test]
fn a_plain_view_draws_no_optional_columns(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(apply_theme);

    let (_root, vcx) = cx.add_window_view(|_, _| host(three_rows()));
    vcx.run_until_parked();
    vcx.update(|window, cx| drop(window.draw(cx)));

    let body = bounds(vcx, "aligned-panes");
    height_is(body.size.height, 3);
    assert!(vcx.debug_bounds("aligned-lead").is_none());
    assert!(vcx.debug_bounds("aligned-base").is_none());
    assert!(bounds(vcx, "aligned-head").size.height > px(0.));
}

#[gpui_kit::gpui::test]
fn a_lead_cell_draws_only_behind_the_toggle(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(apply_theme);

    let mut page = host(three_rows());
    page.lead = true;
    let (_root, vcx) = cx.add_window_view(|_, _| page);
    vcx.run_until_parked();
    vcx.update(|window, cx| drop(window.draw(cx)));

    // one leading cell per side per row, each carrying its own line
    for line in [1, 2, 3] {
        bounds(vcx, leak(&format!("probe-lead-left-{line}")));
        bounds(vcx, leak(&format!("probe-lead-right-{line}")));
    }
    bounds(vcx, "aligned-lead");
}

#[gpui_kit::gpui::test]
fn padding_keeps_the_bodys_height(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(apply_theme);

    let mut page = host(vec![
        (Some(0), Some(0), Some(0), false),
        (Some(1), Some(1), Some(1), true),
    ]);
    page.padding = (3, 4);
    let (_root, vcx) = cx.add_window_view(|_, _| page);
    vcx.run_until_parked();
    vcx.update(|window, cx| drop(window.draw(cx)));

    // two drawn rows plus three above and four below the window
    height_is(bounds(vcx, "aligned-panes").size.height, 9);
}

#[gpui_kit::gpui::test]
fn the_base_column_appears_behind_the_toggle(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(apply_theme);

    let (_root, vcx) = cx.add_window_view(|_, _| host(three_rows()));
    vcx.run_until_parked();
    vcx.update(|window, cx| drop(window.draw(cx)));
    assert!(vcx.debug_bounds("aligned-base").is_none());

    let mut page = host(three_rows());
    page.base_column = true;
    let (_root, vcx) = cx.add_window_view(|_, _| page);
    vcx.run_until_parked();
    vcx.update(|window, cx| drop(window.draw(cx)));
    bounds(vcx, "aligned-base");
    bounds(vcx, "aligned-left");
    bounds(vcx, "aligned-right");
}

#[gpui_kit::gpui::test]
fn the_token_pass_changes_no_row_geometry(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(apply_theme);

    let (_root, plain) = cx.add_window_view(|_, _| host(three_rows()));
    plain.run_until_parked();
    plain.update(|window, cx| drop(window.draw(cx)));
    let plain = f32::from(bounds(plain, "aligned-panes").size.height);

    let mut page = host(three_rows());
    page.syntax = Some("rust");
    let (_root, colored) = cx.add_window_view(|_, _| page);
    colored.run_until_parked();
    colored.update(|window, cx| drop(window.draw(cx)));
    let colored = f32::from(bounds(colored, "aligned-panes").size.height);

    assert!(
        (plain - colored).abs() < 0.5,
        "token colors must not move a row: {plain} vs {colored}"
    );
}

#[gpui_kit::gpui::test]
fn the_pick_box_draws_both_states(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(apply_theme);

    let (_root, vcx) = cx.add_window_view(|_, _| host(three_rows()));
    vcx.run_until_parked();
    vcx.update(|window, cx| drop(window.draw(cx)));

    bounds(vcx, "aligned-pick-box");
}

#[test]
fn the_window_adds_overscan_and_falls_back_before_layout() {
    assert_eq!(visible_window(0, 0., 0., 20.8), (0, 0));
    // no viewport height yet: the fallback window, clamped to the rows
    assert_eq!(visible_window(100, 0., 0., 20.8), (0, 100));
    // ten rows down, twenty visible: overscan either side
    assert_eq!(visible_window(1000, 208., 416., 20.8), (2, 39));
    // the offset never opens a window past the end
    assert_eq!(visible_window(10, 208., 416., 20.8), (2, 10));
}

/// `debug_bounds` takes a `&'static str`; the lead selectors above are built
/// per line, so they leak one small string each — a probe's process lives
/// exactly as long as its selectors.
fn leak(value: &str) -> &'static str {
    Box::leak(value.to_string().into_boxed_str())
}
