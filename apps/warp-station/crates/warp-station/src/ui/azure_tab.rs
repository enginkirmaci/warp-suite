//! The Pipelines (Azure DevOps) tab: the snapshot's latest run state in the
//! toolbar, then the recent runs, the project's active pull requests and its
//! open work items as cards. The detail pane's own padding and reading grid
//! keep it the same shape as Overview.

use crate::store::Store;
use crate::theme;
use crate::ui::A11y;
use crate::ui::icon;
use gpui::prelude::*;
use gpui::{AnyElement, Context, Entity, FontWeight, SharedString, Window, div, px};
use warp_station_core::azure::{AzureItem, AzureSnapshot, PipelineRun};
use warpui_kit_core::{motion, shell};

pub struct AzureView {
    pub store: Entity<Store>,
}

impl Render for AzureView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        let folder = self.store.read(cx).selected_repo.clone();
        let snapshot = folder
            .as_ref()
            .and_then(|f| self.store.read(cx).azure_by_folder.get(f).cloned());

        let Some(snapshot) = snapshot else {
            return empty_state(
                "No Azure DevOps activity loaded — the pass fills after the scan settles".into(),
            );
        };
        if !snapshot.available {
            return empty_state(
                "This repository has no Azure DevOps remote (or the PAT was rejected)".into(),
            );
        }
        if snapshot.pipelines.is_empty()
            && snapshot.pulls.is_empty()
            && snapshot.work_items.is_empty()
        {
            return empty_state("No activity on Azure DevOps".into());
        }

        let toolbar = self.render_toolbar(&snapshot);
        let body = div()
            .id("azure-list")
            .debug_selector(|| "azure-list".into())
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .child(
                div()
                    .max_w(px(shell::GRID_WIDTH))
                    .p(px(shell::DETAIL_PADDING))
                    .flex()
                    .flex_wrap()
                    .items_start()
                    .gap_4()
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(320.))
                            .flex()
                            .flex_col()
                            .child(runs_card(&snapshot)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(320.))
                            .flex()
                            .flex_col()
                            .gap_4()
                            .child(items_card(
                                "Pull requests",
                                "No active pull requests",
                                &snapshot.pulls,
                                "icons/icon-git-pull-request.svg",
                                theme::PR,
                                "PR",
                            ))
                            .child(items_card(
                                "Work items",
                                "No active work items",
                                &snapshot.work_items,
                                "icons/icon-issue.svg",
                                theme::ISSUE,
                                "WI",
                            )),
                    ),
            );
        div()
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .child(toolbar)
            .child(body)
            .into_any_element()
    }
}

impl AzureView {
    /// The latest run's state, where the other tabs put their toolbar, plus a
    /// refresh for the pass that filled it.
    fn render_toolbar(&self, snapshot: &AzureSnapshot) -> AnyElement {
        let (glyph, color, word, busy) = if snapshot.pipelines.is_empty() {
            (
                "icons/icon-clock.svg",
                theme::TEXT_MUTED,
                "No pipeline runs",
                false,
            )
        } else if snapshot.pipeline_running() {
            (
                "icons/icon-spinner.svg",
                theme::BEHIND,
                "Latest run in progress",
                true,
            )
        } else if snapshot.pipeline_failed() {
            (
                "icons/icon-close.svg",
                theme::DANGER,
                "Latest run failed",
                false,
            )
        } else if snapshot.pipeline_ok() {
            (
                "icons/icon-check.svg",
                theme::SUCCESS_BRIGHT,
                "Latest run succeeded",
                false,
            )
        } else {
            (
                "icons/icon-clock.svg",
                theme::TEXT_MUTED,
                "Latest run queued",
                false,
            )
        };
        let store = self.store.clone();
        let latest = div()
            .flex()
            .items_center()
            .gap_2()
            .text_size(px(theme::font_body()))
            .text_color(theme::TEXT_DIM)
            .child(if busy {
                motion::spinner(
                    "azure-latest-spinner",
                    "icons/icon-spinner.svg",
                    px(theme::icon_small()),
                    color,
                )
                .into_any_element()
            } else {
                icon(glyph, px(theme::icon_small()), color).into_any_element()
            })
            .child(
                div()
                    .text_color(color)
                    .font_weight(FontWeight::MEDIUM)
                    .child(word),
            );

        div()
            .flex_shrink_0()
            .h(px(52.))
            .px(px(shell::DETAIL_PADDING))
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .border_b_1()
            .border_color(theme::HAIRLINE)
            .child(latest)
            .child(div().flex_1())
            .child(
                div()
                    .id("azure-refresh")
                    .debug_selector(|| "azure-refresh".into())
                    .a11y_button("Refresh pipeline panel")
                    .cursor_pointer()
                    .size(px(shell::CONTROL_HEIGHT))
                    .rounded(px(6.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .hover(|s| s.bg(theme::RAISED))
                    .on_click(move |_, _, cx| {
                        store.update(cx, |s, cx| s.refresh_panel(cx));
                    })
                    .child(icon(
                        "icons/icon-refresh.svg",
                        px(theme::icon_small()),
                        theme::TEXT_MUTED,
                    )),
            )
            .into_any_element()
    }
}

/// The recent runs, newest first, with the failed one tinted like a removed
/// line so it reads before the words do.
fn runs_card(snapshot: &AzureSnapshot) -> AnyElement {
    let mut rows = div().flex().flex_col();
    for (ix, run) in snapshot.pipelines.iter().enumerate() {
        rows = rows.child(run_row(ix, run));
    }
    card(
        "Pipelines",
        Some(format!("{} recent", snapshot.pipelines.len())),
    )
    .when(snapshot.pipelines.is_empty(), |card| {
        card.child(
            div()
                .px_4()
                .py_3()
                .text_size(px(theme::font_meta()))
                .text_color(theme::TEXT_MUTED)
                .child("No pipeline runs yet"),
        )
    })
    .child(rows)
    .into_any_element()
}

fn run_row(ix: usize, run: &PipelineRun) -> AnyElement {
    let (glyph, color, word) = if run.is_running() {
        ("spinner", theme::BEHIND, "in progress")
    } else if run.is_failed() {
        ("icons/icon-close.svg", theme::DANGER, "failed")
    } else if run.is_succeeded() {
        ("icons/icon-check.svg", theme::SUCCESS_BRIGHT, "succeeded")
    } else {
        ("icons/icon-clock.svg", theme::TEXT_MUTED, "queued")
    };
    let url = run.url.clone();
    let branch = run.display_branch();
    let mut sub = div()
        .flex()
        .items_center()
        .gap_1p5()
        .text_size(px(theme::font_meta()))
        .text_color(theme::TEXT_MUTED);
    if let Some(branch) = branch {
        sub = sub
            .child(icon(
                "icons/icon-source-branch.svg",
                px(theme::icon_tiny()),
                theme::BRANCH,
            ))
            .child(div().text_color(theme::TEXT_DIM).child(branch))
            .child(div().text_color(theme::TEXT_MUTED).child("·"));
    }
    sub = sub.child(word);

    div()
        .id(SharedString::from(format!("pipeline-run-{ix}")))
        .debug_selector(move || format!("pipeline-run-{ix}"))
        .a11y_button(format!(
            "Pipeline run {}",
            run.definition_name.clone().unwrap_or_default()
        ))
        .when(run.is_failed(), |row| row.bg(theme::tint(theme::DEL)))
        .flex()
        .items_center()
        .gap_2p5()
        .min_h(px(44.))
        .px_4()
        .py_1p5()
        .border_t_1()
        .border_color(theme::BORDER_LIGHT)
        .child(if glyph == "spinner" {
            motion::spinner(
                SharedString::from(format!("pipeline-spin-{ix}")),
                "icons/icon-spinner.svg",
                px(theme::icon_small()),
                theme::BEHIND,
            )
            .into_any_element()
        } else {
            icon(glyph, px(theme::icon_small()), color).into_any_element()
        })
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap_0p5()
                .child(
                    div()
                        .min_w_0()
                        .text_size(px(theme::font_body()))
                        .text_color(theme::TEXT)
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(format!(
                            "{}  {}",
                            run.definition_name
                                .clone()
                                .unwrap_or_else(|| "Pipeline".into()),
                            run.build_number
                        )),
                )
                .child(sub),
        )
        .when_some(url, |row, url| {
            row.child(
                div()
                    .id(SharedString::from(format!("pipeline-open-{ix}")))
                    .a11y_button("Open on Azure DevOps")
                    .cursor_pointer()
                    .size(px(26.))
                    .rounded(px(4.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .hover(|s| s.bg(theme::RAISED))
                    .on_click(move |_, _, _| {
                        let _ = warp_station_core::terminal::open_folder(&url);
                    })
                    .child(icon(
                        "icons/icon-open-in-new.svg",
                        px(theme::icon_tiny()),
                        theme::TEXT_MUTED,
                    )),
            )
        })
        .into_any_element()
}

/// One of the project's lists: its count in the head, then a row per item.
fn items_card(
    title: &'static str,
    empty: &'static str,
    items: &[AzureItem],
    glyph: &'static str,
    color: gpui::Rgba,
    kind: &'static str,
) -> AnyElement {
    let mut rows = div().flex().flex_col();
    for (ix, item) in items.iter().enumerate() {
        rows = rows.child(item_row(ix, item, glyph, color, kind));
    }
    card(title, Some(format!("{}", items.len())))
        .when(items.is_empty(), |card| {
            card.child(
                div()
                    .px_4()
                    .py_3()
                    .text_size(px(theme::font_meta()))
                    .text_color(theme::TEXT_MUTED)
                    .child(empty),
            )
        })
        .child(rows)
        .into_any_element()
}

fn item_row(
    ix: usize,
    item: &AzureItem,
    glyph: &'static str,
    color: gpui::Rgba,
    kind: &'static str,
) -> AnyElement {
    let url = item.url.clone();
    let meta = [item.author.clone(), item.state.clone()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ");
    div()
        .id(SharedString::from(format!("azure-{kind}-{ix}")))
        .debug_selector(move || format!("azure-{kind}-{ix}"))
        .a11y_button(format!("{kind} #{} {}", item.number, item.title))
        .cursor_pointer()
        .flex()
        .items_center()
        .gap_2p5()
        .min_h(px(44.))
        .px_4()
        .py_1p5()
        .border_t_1()
        .border_color(theme::BORDER_LIGHT)
        .hover(|row| row.bg(theme::RAISED))
        .on_click(move |_, _, _| {
            if !url.is_empty() {
                let _ = warp_station_core::terminal::open_folder(&url);
            }
        })
        .child(icon(glyph, px(theme::icon_small()), color))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap_0p5()
                .child(
                    div()
                        .min_w_0()
                        .text_size(px(theme::font_body()))
                        .text_color(theme::TEXT)
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(format!("#{} {}", item.number, item.title)),
                )
                .when(!meta.is_empty(), |column| {
                    column.child(
                        div()
                            .min_w_0()
                            .text_size(px(theme::font_meta()))
                            .text_color(theme::TEXT_MUTED)
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(meta),
                    )
                }),
        )
        .into_any_element()
}

/// A card: the base surface on the detail's panel, its head ruled.
fn card(head: &'static str, count: Option<String>) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .pb_2()
        .rounded(px(8.))
        .border_1()
        .border_color(theme::HAIRLINE)
        .bg(theme::SURFACE)
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_3()
                .min_h(px(44.))
                .px_4()
                .mb_1()
                .border_b_1()
                .border_color(theme::BORDER_LIGHT)
                .child(
                    div()
                        .text_size(px(theme::font_name()))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::TEXT)
                        .child(head),
                )
                .children(count.map(|count| {
                    div()
                        .text_size(px(theme::font_meta()))
                        .text_color(theme::TEXT_MUTED)
                        .child(count)
                })),
        )
}

fn empty_state(text: String) -> gpui::AnyElement {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .p_6()
        .text_size(px(theme::font_body()))
        .text_color(theme::TEXT_FAINT)
        .child(text)
        .into_any_element()
}
