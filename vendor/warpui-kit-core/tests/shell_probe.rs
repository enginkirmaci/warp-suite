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
                                            palette::SURFACE,
                                        )
                                        .child("Repositories"),
                                    )
                                    .child(
                                        shell::strip_tab(
                                            "tab-item",
                                            "warp-agent",
                                            false,
                                            palette::PANEL,
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
