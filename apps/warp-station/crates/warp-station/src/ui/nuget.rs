//! The NuGet Package Manager page: watch a folder for new .nupkg build
//! outputs, clear the matching global cache so consumers fetch fresh DLLs.
//! A thin view over the store's NuGet snapshot; the watch itself lives on the
//! net thread's service and survives leaving the page.

use crate::store::Store;
use crate::theme;
use crate::ui::A11y;
use gpui::prelude::*;
use gpui::{App, Context, Entity, FontWeight, Window, div, px};
use gpui_kit::component::Sizable as _;
use gpui_kit::component::input::{Input, InputEvent, InputState};

pub struct NugetDrawer {
    pub store: Entity<Store>,
    watch_input: Entity<InputState>,
}

impl NugetDrawer {
    pub fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let folder = store
            .read(cx)
            .settings()
            .nuget_local
            .watch_folder
            .unwrap_or_default();
        let watch_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Path of the folder to watch…")
                .default_value(folder)
        });
        cx.subscribe(&watch_input, |this, _, event: &InputEvent, cx| {
            // Enter applies: persists the folder and restarts a running watch
            // (per-keystroke applies would churn the watcher)
            if let InputEvent::PressEnter { .. } = event {
                this.apply_watch_folder(cx);
            }
        })
        .detach();
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        Self { store, watch_input }
    }

    /// Persists the typed folder and applies it to the service.
    fn apply_watch_folder(&mut self, cx: &mut Context<Self>) {
        let folder = self.watch_input.read(cx).value().trim().to_string();
        self.store
            .update(cx, |store, _| store.set_nuget_folder(folder));
    }

    fn toggle_watch(&mut self, cx: &mut Context<Self>) {
        let watching = self.store.read(cx).nuget.watching;
        self.apply_watch_folder(cx);
        self.store.update(cx, |store, cx| {
            if watching {
                store.stop_nuget();
            } else {
                store.start_nuget();
            }
            cx.notify();
        });
        cx.notify();
    }

    fn register(&mut self, cx: &mut Context<Self>) {
        self.apply_watch_folder(cx);
        self.store.update(cx, |store, cx| {
            store.register_nuget_source();
            cx.notify();
        });
        cx.notify();
    }
}

impl Render for NugetDrawer {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        let snapshot = self.store.read(cx).nuget.clone();
        let mut activity = div()
            .id("nuget-activity-list")
            .flex()
            .flex_col()
            .overflow_y_scroll();
        for (index, line) in snapshot.activity.iter().enumerate() {
            activity = activity.child(
                div()
                    .id(("nuget-activity", index as u64))
                    .rounded_md()
                    .px_1p5()
                    .py_0p5()
                    .text_size(px(theme::font_meta()))
                    .font_family(gpui::SharedString::from(theme::code_font()))
                    .text_color(theme::TEXT_FAINT)
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(line.clone()),
            );
        }

        div()
            .id("nuget-drawer")
            .size_full()
            .max_w(px(warpui_kit_core::shell::READING_WIDTH + 2. * warpui_kit_core::shell::DETAIL_PADDING))
            .p(px(warpui_kit_core::shell::DETAIL_PADDING))
            .flex()
            .flex_col()
            .gap_3()
            .occlude()
            .child(crate::ui::tool_header(
                "NuGet Package Manager",
                "Refresh the global cache when a local build produces .nupkg files.",
            ))
            // setup card: watch folder, resolved cache paths, register
            .child(
                div()
                    .rounded_lg()
                    .bg(theme::CARD)
                    .border_1()
                    .border_color(theme::BORDER_LIGHT)
                    .p_2()
                    .flex()
                    .flex_col()
                    .gap_1p5()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_size(px(theme::font_meta()))
                                    .text_color(theme::TEXT_MUTED)
                                    .child("Watch directory"),
                            )
                            .child(div().flex_1())
                            .child(action_button(
                                "nuget-register",
                                "Register source",
                                theme::BLUE_BRIGHT,
                                false,
                                cx.listener(|this, _, _, cx| this.register(cx)),
                            )),
                    )
                    .child(Input::new(&self.watch_input).with_size(theme::field_size()))
                    .child(
                        div()
                            .text_size(px(theme::font_control()))
                            .text_color(theme::TEXT_FAINT)
                            .child("Enter applies the folder to a running watch."),
                    )
                    .child(path_row("Local cache", snapshot.copy_folder.as_deref()))
                    .child(path_row(
                        "Global cache",
                        snapshot.global_packages_folder.as_deref(),
                    )),
            )
            // how-it-works card
            .child(
                div()
                    .rounded_lg()
                    .bg(theme::CARD)
                    .border_1()
                    .border_color(theme::BORDER_LIGHT)
                    .p_2()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .child(
                        div()
                            .text_size(px(theme::font_body()))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("How it works"),
                    )
                    .child(explain_line("1. Register your build folder as a local NuGet source."))
                    .child(explain_line("2. Start Watch to monitor for new build outputs (.nupkg)."))
                    .child(explain_line(
                        "3. On every build, the tool clears the global cache, forcing consumer projects to fetch your fresh DLLs.",
                    )),
            )
            // activity card
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .rounded_lg()
                    .bg(theme::CARD)
                    .border_1()
                    .border_color(theme::BORDER_LIGHT)
                    .p_2()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .child(
                                div()
                                    .text_size(px(theme::font_body()))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Activity Log"),
                            )
                            .child(div().flex_1())
                            .child(
                                div()
                                    .rounded_full()
                                    .bg(theme::CHIP)
                                    .px_2()
                                    .py_0p5()
                                    .text_size(px(theme::font_meta()))
                                    .text_color(theme::TEXT_MUTED)
                                    .child(format!("{} refreshed", snapshot.count)),
                            ),
                    )
                    .when(snapshot.activity.is_empty(), |card| {
                        card.child(
                            div()
                                .flex_1()
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_size(px(theme::font_body()))
                                .text_color(theme::TEXT_FAINT)
                                .child("No activity yet — start the watch and build something."),
                        )
                    })
                    .when(!snapshot.activity.is_empty(), |card| card.child(activity)),
            )
            // footer: the watch toggle
            .child(
                div()
                    .flex()
                    .flex_row()
                    .justify_end()
                    .child(if snapshot.watching {
                        action_button(
                            "nuget-watch",
                            "Stop Watch",
                            theme::DANGER,
                            false,
                            cx.listener(|this, _, _, cx| this.toggle_watch(cx)),
                        )
                    } else {
                        action_button(
                            "nuget-watch",
                            "Start Watch",
                            theme::SUCCESS_BRIGHT,
                            false,
                            cx.listener(|this, _, _, cx| this.toggle_watch(cx)),
                        )
                    }),
            )
    }
}

fn path_row(label: &str, value: Option<&str>) -> gpui::AnyElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .child(
            div()
                .w(px(90.))
                .text_size(px(theme::font_meta()))
                .text_color(theme::TEXT_MUTED)
                .child(label.to_string()),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(px(theme::font_meta()))
                .text_color(theme::TEXT_DIM)
                .overflow_hidden()
                .whitespace_nowrap()
                .child(value.unwrap_or("Not resolved yet").to_string()),
        )
        .into_any_element()
}

fn explain_line(text: &str) -> gpui::AnyElement {
    div()
        .text_size(px(theme::font_meta()))
        .text_color(theme::TEXT_DIM)
        .child(text.to_string())
        .into_any_element()
}

fn action_button(
    id: &'static str,
    label: &str,
    color: gpui::Rgba,
    disabled: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> gpui::AnyElement {
    div()
        .id(id)
        .a11y_button(label.to_string())
        .cursor_pointer()
        .rounded_md()
        .px_3()
        .py_1()
        .text_size(px(theme::font_body()))
        .font_weight(FontWeight::SEMIBOLD)
        .bg(theme::tint(color))
        .text_color(color)
        .map(|d| {
            if disabled {
                d.opacity(0.4)
            } else {
                d.hover(|s| s.opacity(0.85))
            }
        })
        .when(!disabled, |d| d.on_click(on_click))
        .child(label.to_string())
        .into_any_element()
}
