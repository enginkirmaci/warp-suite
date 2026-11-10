//! The Tools pages only Windows has: SnapIt, the engine with no window of its
//! own, with the layouts it reads from `~/.warp/snapit`; and Keep Awake. Both
//! say their state in the page's head and in their row, so the title bar needs
//! no status pill of its own.

use crate::store::Store;
use crate::theme;
use crate::ui::{A11y, icon};
use gpui::prelude::*;
use gpui::{Context, Entity, FontWeight, Rgba, SharedString, Window, div, px};
use warp_station_core::snapit::{self, SnapLayout};
use warpui_kit_core::shell;

const THUMB_W: f32 = 52.;
const THUMB_H: f32 = 30.;
const PREVIEW_W: f32 = 720.;
const PREVIEW_H: f32 = 400.;

pub struct SnapItPage {
    store: Entity<Store>,
    layouts: Vec<SnapLayout>,
    selected: usize,
}

impl SnapItPage {
    pub fn new(store: Entity<Store>) -> Self {
        Self::with_layouts(store, snapit::layouts_in(&snapit::layouts_folder()))
    }

    /// The page over given layouts, so a probe stays off the real folder.
    pub fn with_layouts(store: Entity<Store>, layouts: Vec<SnapLayout>) -> Self {
        Self {
            store,
            layouts,
            selected: 0,
        }
    }

    fn toggle(&mut self, cx: &mut Context<Self>) {
        let executable = self.store.read(cx).settings().snapit.executable;
        let result = if snapit::is_running() {
            snapit::stop()
        } else {
            snapit::start(&executable)
        };
        if let Err(err) = result {
            self.store.update(cx, |s, cx| {
                s.notify_error(err);
                cx.notify();
            });
        }
        cx.notify();
    }
}

impl Render for SnapItPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        let running = snapit::is_running();
        let label = if running {
            "Stop SnapIt"
        } else {
            "Start SnapIt"
        };
        let run = div()
            .id("snapit-run")
            .debug_selector(|| "snapit-run".into())
            .a11y_button(label)
            .cursor_pointer()
            .flex_shrink_0()
            .h(px(shell::CONTROL_HEIGHT))
            .px_3()
            .rounded(px(6.))
            .flex()
            .items_center()
            .gap_2()
            .bg(theme::accent())
            .text_size(px(theme::font_control()))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(theme::on_accent())
            .hover(|s| s.bg(theme::accent_hover()))
            .on_click(cx.listener(|this, _, _, cx| this.toggle(cx)))
            .child(icon(
                if running {
                    "icons/icon-stop.svg"
                } else {
                    "icons/icon-play.svg"
                },
                px(theme::icon_small()),
                theme::on_accent(),
            ))
            .child(label);

        let mut picks = div().flex().flex_row().flex_wrap().gap_2();
        for (ix, layout) in self.layouts.iter().enumerate() {
            let selector = format!("snapit-layout-{ix}");
            let selected = ix == self.selected;
            picks = picks.child(
                div()
                    .id(SharedString::from(selector.clone()))
                    .debug_selector(move || selector.clone())
                    .a11y_toggle(layout.name.clone(), selected)
                    .cursor_pointer()
                    .h(px(48.))
                    .px_2()
                    .rounded(px(6.))
                    .border_1()
                    .border_color(if selected {
                        theme::accent_hover()
                    } else {
                        theme::line_strong()
                    })
                    .when(selected, |d| d.bg(theme::accent_soft()))
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_size(px(theme::font_control()))
                    .text_color(theme::text())
                    .hover(|s| s.bg(theme::raised()))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.selected = ix;
                        cx.notify();
                    }))
                    .child(preview(layout, THUMB_W, THUMB_H, theme::text_muted()))
                    .child(layout.name.clone()),
            );
        }

        let body = match self.layouts.get(self.selected) {
            Some(layout) => {
                let (width, height) = fitted(layout);
                div()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .child(picks)
                    .child(
                        div()
                            .debug_selector(|| "snapit-preview".into())
                            .w(px(width))
                            .child(preview(layout, width, height, theme::accent_ink())),
                    )
                    .child(
                        div()
                            .text_size(px(theme::font_meta()))
                            .text_color(theme::text_muted())
                            .child("The engine snaps a dragged window into the zones these lines make."),
                    )
                    // The About row's Open controls lands here, so the facts
                    // that row promises are on the page it opens.
                    .child(
                        div()
                            .debug_selector(|| "snapit-version".into())
                            .text_size(px(theme::font_meta()))
                            .text_color(theme::text_muted())
                            .child(format!(
                                "SnapIt {} ({}) · {} · {}",
                                env!("CARGO_PKG_VERSION"),
                                env!("WARP_SUITE_BUILD"),
                                env!("WARP_SUITE_CHANNEL"),
                                warpui_kit_core::about::LICENSE,
                            )),
                    )
                    .into_any_element()
            }
            None => div()
                .text_size(px(theme::font_body()))
                .text_color(theme::text_dim())
                .child("No layouts yet. SnapIt writes its defaults the first time it starts.")
                .into_any_element(),
        };

        page(
            "SnapIt",
            running,
            if running { "Running" } else { "Stopped" },
            "Reads the layouts in ~/.warp/snapit",
        )
        .child(div().flex_shrink_0().pb_4().flex().flex_row().child(run))
        .child(
            div()
                .id("snapit-body")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .child(body),
        )
    }
}

/// The preview's size: the layout's shape, as large as the page's box allows.
fn fitted(layout: &SnapLayout) -> (f32, f32) {
    if layout.width <= 0. || layout.height <= 0. {
        return (PREVIEW_W, PREVIEW_H);
    }
    let scale = (PREVIEW_W / layout.width).min(PREVIEW_H / layout.height);
    (layout.width * scale, layout.height * scale)
}

/// The layout's lines inside a frame of the given size.
fn preview(layout: &SnapLayout, width: f32, height: f32, line: Rgba) -> impl IntoElement + use<> {
    let (sx, sy) = if layout.width > 0. && layout.height > 0. {
        (width / layout.width, height / layout.height)
    } else {
        (1., 1.)
    };
    let mut frame = div()
        .relative()
        .flex_shrink_0()
        .w(px(width))
        .h(px(height))
        .rounded(px(4.))
        .border_1()
        .border_color(theme::line_strong())
        .bg(theme::well())
        .overflow_hidden();
    for split in &layout.lines {
        frame = frame.child(
            div()
                .absolute()
                .left(px(split.x * sx))
                .top(px(split.y * sy))
                .w(px((split.width * sx).max(1.)))
                .h(px((split.height * sy).max(1.)))
                .bg(line),
        );
    }
    frame
}

/// Keep Awake: holds the system and the display awake while it is on.
pub struct KeepAwakePage {
    store: Entity<Store>,
}

impl KeepAwakePage {
    pub fn new(store: Entity<Store>) -> Self {
        Self { store }
    }
}

impl Render for KeepAwakePage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        let on = warp_station_core::keepawake::is_enabled();
        let label = if on { "Let it sleep" } else { "Keep awake" };
        page(
            "Keep Awake",
            on,
            if on { "On" } else { "Off" },
            "Holds the system and the display awake",
        )
        .child(
            div().flex().flex_row().child(
                div()
                    .id("keep-awake-toggle")
                    .debug_selector(|| "keep-awake-toggle".into())
                    .a11y_button(label)
                    .cursor_pointer()
                    .h(px(shell::CONTROL_HEIGHT))
                    .px_3()
                    .rounded(px(6.))
                    .flex()
                    .items_center()
                    .bg(theme::accent())
                    .text_size(px(theme::font_control()))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme::on_accent())
                    .hover(|s| s.bg(theme::accent_hover()))
                    .on_click(cx.listener(|this, _, _, cx| {
                        let wanted = !warp_station_core::keepawake::is_enabled();
                        this.store.update(cx, |s, cx| {
                            match warp_station_core::keepawake::set_enabled(wanted) {
                                Ok(()) => s.update_settings(|settings| {
                                    settings.keep_awake.enabled = wanted;
                                }),
                                Err(err) => s.notify_error(err),
                            }
                            cx.notify();
                        });
                        cx.notify();
                    }))
                    .child(label),
            ),
        )
    }
}

/// The two pages' head: title, a state dot with its word, and what it does.
fn page(title: &'static str, on: bool, state: &'static str, hint: &'static str) -> gpui::Div {
    div()
        .size_full()
        .min_h_0()
        .flex()
        .flex_col()
        .gap_4()
        .p(px(shell::DETAIL_PADDING))
        .child(
            div()
                .flex_shrink_0()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .text_size(px(theme::font_screen_title()))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::text())
                        .child(title),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .text_size(px(theme::font_meta()))
                        .text_color(theme::text_muted())
                        .child(div().size(px(8.)).rounded_full().bg(if on {
                            theme::SUCCESS_BRIGHT
                        } else {
                            theme::line_strong()
                        }))
                        .child(state)
                        .child(hint),
                ),
        )
}
