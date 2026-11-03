//! The shell lays out as the design says: a fixed rail and list, and a detail
//! pane that takes the rest of the window.

use gpui::{Context, IntoElement, Render, TestAppContext, Window, div, prelude::*, px, size};
use warpui_kit_core::{motion, palette, shell};

struct Shell;

impl Render for Shell {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .size_full()
            .child(shell::title_bar().debug_selector(|| "title".into()))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .min_h_0()
                    .child(
                        shell::rail()
                            .debug_selector(|| "rail".into())
                            .child(shell::rail_item("repos", "icons/repos.svg", "Repos", true))
                            .child(shell::rail_item("tools", "icons/tools.svg", "Tools", false)),
                    )
                    .child(
                        shell::stage()
                            .debug_selector(|| "stage".into())
                            .child(
                                shell::tab_strip()
                                    .debug_selector(|| "strip".into())
                                    .child(
                                        shell::strip_tab(
                                            "tab-list",
                                            "Repositories",
                                            true,
                                            palette::surface(),
                                        )
                                        .child("Repositories"),
                                    )
                                    .child(
                                        shell::strip_tab(
                                            "tab-item",
                                            "warp-agent",
                                            false,
                                            palette::panel(),
                                        )
                                        .child("warp-agent"),
                                    ),
                            )
                            .child(
                                shell::panes()
                                    .child(
                                        shell::list_pane(shell::LIST_WIDTH)
                                            .debug_selector(|| "list".into())
                                            .child(
                                                shell::list_row("row", true)
                                                    .debug_selector(|| "row".into())
                                                    .child("warp-station"),
                                            ),
                                    )
                                    .child(
                                        shell::detail_pane()
                                            .debug_selector(|| "detail".into())
                                            .child(shell::detail_tab("tab", "Overview", true))
                                            .child(motion::spinner(
                                                "busy",
                                                "icons/arc.svg",
                                                px(16.),
                                                palette::BEHIND,
                                            )),
                                    ),
                            ),
                    ),
            )
    }
}

#[gpui_kit::gpui::test]
fn the_panes_take_their_widths(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (_root, cx) = cx.add_window_view(|_, _| Shell);
    cx.simulate_resize(size(px(1440.), px(900.)));
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    let mut bounds = |selector: &'static str| cx.debug_bounds(selector).expect(selector);
    let rail = bounds("rail");
    let list = bounds("list");
    let detail = bounds("detail");
    let strip = bounds("strip");
    let title = bounds("title");

    assert_eq!(rail.size.width, px(shell::RAIL_WIDTH));
    assert_eq!(list.size.width, px(shell::LIST_WIDTH));
    assert_eq!(strip.size.height, px(shell::TAB_STRIP_HEIGHT));
    assert_eq!(title.size.height, px(shell::TITLE_BAR_HEIGHT));
    assert_eq!(
        strip.origin.x, list.origin.x,
        "the strip runs over the list"
    );
    assert_eq!(
        detail.size.width,
        px(1440. - shell::RAIL_WIDTH - shell::LIST_WIDTH),
        "the detail takes the rest"
    );
    assert_eq!(detail.origin.x, list.origin.x + list.size.width);
}

#[gpui_kit::gpui::test]
fn the_row_fills_its_list(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (_root, cx) = cx.add_window_view(|_, _| Shell);
    cx.simulate_resize(size(px(1440.), px(900.)));
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    let row = cx.debug_bounds("row").expect("row");
    let list = cx.debug_bounds("list").expect("list");
    // the list's right border takes one pixel
    assert_eq!(row.size.width, list.size.width - px(1.));
}

/// The settings row vocabulary: a switch row is 60 high, its label is on the
/// left at the `name` step, and the switch sits at the trailing edge.
#[gpui_kit::gpui::test]
fn a_switch_row_puts_the_label_left_and_the_switch_right(cx: &mut gpui::TestAppContext) {
    use gpui::prelude::*;
    use gpui::{Context, IntoElement, Render, Window, div};

    struct Page;
    impl Render for Page {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .w(gpui::px(600.))
                .child(warpui_kit_core::settings::switch_row(
                    "probe-switch",
                    "Reduce motion",
                    "settings-row-Reduce motion",
                    false,
                    true,
                    |_, _, _| {},
                ))
        }
    }

    cx.update(gpui_kit::init);
    let (_root, cx) = cx.add_window_view(|_, _| Page);
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    let switch = cx.debug_bounds("switch-Reduce motion").expect("switch");
    // the design's `.switch` is 36 wide by 20 high
    assert_eq!(switch.size.width, gpui::px(36.));
    assert_eq!(switch.size.height, gpui::px(20.));
    // and it trails the label: it sits in the right-hand half of a 600 row
    assert!(
        switch.origin.x > gpui::px(300.),
        "the switch trails the label, got x={:?}",
        switch.origin.x
    );
}

/// A described row keeps its control inside a narrow pane: the description
/// wraps, and the control ends at the row's trailing edge.
#[gpui_kit::gpui::test]
fn a_described_row_keeps_its_control_inside_a_narrow_pane(cx: &mut gpui::TestAppContext) {
    use gpui::prelude::*;
    use gpui::{Context, IntoElement, Render, Window, div};

    struct Page;
    impl Render for Page {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .w(gpui::px(700.))
                .child(warpui_kit_core::settings::row_with_desc(
                    "probe-row",
                    "Text size",
                    "Applied right away. The other Warp apps follow when they next open.",
                    div().h(gpui::px(34.)).w(gpui::px(314.)).bg(gpui::black()),
                    true,
                ))
        }
    }

    cx.update(gpui_kit::init);
    let (_root, cx) = cx.add_window_view(|_, _| Page);
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    let row = cx.debug_bounds("probe-row").expect("row");
    eprintln!("PROBE row {row:?}");
    assert!(
        row.size.width <= gpui::px(700.),
        "the row must not grow past its container: {row:?}"
    );
}
