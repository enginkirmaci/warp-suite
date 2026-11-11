//! The Pipelines tab: the design's workbench. A 320 run list — GitHub Actions
//! and Azure Pipelines interleaved, newest first, each row tagged with the
//! source it came from — a hairline, then the selected run: its state, branch
//! and commit, start, duration and trigger, one card per stage with its jobs,
//! and the tail of its log. Below 900 px of pane the two take turns and Back
//! returns to the list.
//!
//! The run detail is source-neutral: the row is an `ActivityRun`, and each
//! source answers with the same `RunDetail` — a timeline for Azure, one
//! `gh run view` for Actions. The project's Azure pull requests and work items
//! live under the runs in the same column: they are the only entry to an Azure
//! item, and a pull request opens its own detail in the pane.

use crate::store::Store;
use crate::theme;
use crate::ui::diff_controls;
use crate::ui::pull_request::{plural, source_tag};
use crate::ui::{A11y, icon};
use gpui::prelude::*;
use gpui::{AnyElement, Context, Entity, FontWeight, Rgba, SharedString, Window, div, px};
use warp_station_core::activity::{
    ActivityRun, ActivitySource, RunDetail, RunJob, RunStage, duration_between, sort_newest_first,
};
use warp_station_core::azure::AzureSnapshot;
use warpui_kit_core::{motion, shell};

/// The workbench's list column, and the pane width under which it and the
/// detail stop sharing the pane.
const WORK_LIST: f32 = 320.;
const STACKED_PANE: f32 = 900.;
/// The stage cards and the log keep the work detail's measure.
const BODY_WIDTH: f32 = 920.;
/// The log rows one excerpt draws; the tail is where a failure is.
const LOG_ROWS: usize = 120;

pub struct AzureView {
    pub store: Entity<Store>,
}

impl AzureView {
    pub fn new(store: Entity<Store>, cx: &mut Context<Self>) -> Self {
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        Self { store }
    }

    /// Whether the pane is too narrow for both columns.
    fn stacked(&self, cx: &Context<Self>) -> bool {
        self.store.read(cx).pane_width < STACKED_PANE
    }

    /// Escape: the selected run on a narrow pane. Returns whether it took the
    /// key.
    pub fn dismiss(&mut self, cx: &mut Context<Self>) -> bool {
        if self.stacked(cx) && self.store.read(cx).open_run.is_some() {
            self.store.update(cx, |store, cx| store.close_run(cx));
            return true;
        }
        false
    }
}

impl Render for AzureView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        let folder = self.store.read(cx).selected_repo.clone();
        let azure = folder
            .as_ref()
            .and_then(|f| self.store.read(cx).azure_by_folder.get(f).cloned());
        let github = folder
            .as_ref()
            .and_then(|f| self.store.read(cx).github_by_folder.get(f).cloned());
        let (azure_enabled, github_enabled) = {
            let store = self.store.read(cx);
            let settings = store.settings();
            (
                settings.repos.enable_azure_devops,
                settings.repos.enable_github,
            )
        };
        let azure_ok = azure.as_ref().is_some_and(|snapshot| snapshot.available);
        let github_ok = github.as_ref().is_some_and(|snapshot| snapshot.available);
        if !azure_ok && !github_ok {
            return empty_state(if azure.is_none() && github.is_none() {
                // no source has answered yet: an enabled one is still on its
                // way, and a disabled pair has nothing to wait for
                if azure_enabled || github_enabled {
                    "No activity loaded yet — the pass fills after the scan settles".into()
                } else {
                    "GitHub and Azure DevOps are both turned off — turn one on in Settings to see its runs."
                        .into()
                }
            } else {
                "This repository has no GitHub or Azure DevOps remote.".into()
            });
        }

        // one list, both sources, newest first
        let mut runs: Vec<ActivityRun> = Vec::new();
        if let Some(snapshot) = azure.as_ref().filter(|snapshot| snapshot.available) {
            runs.extend(snapshot.pipelines.iter().map(ActivityRun::of_azure));
        }
        if let Some(snapshot) = github.as_ref().filter(|snapshot| snapshot.available) {
            runs.extend(snapshot.runs.iter().map(ActivityRun::of_github));
        }
        sort_newest_first(&mut runs);

        let toolbar = self.render_toolbar(runs.first());
        let body = self.render_workbench(
            &runs,
            azure.as_deref().filter(|snapshot| snapshot.available),
            github_ok,
            window,
            cx,
        );
        div()
            .debug_selector(|| "azure-tab".into())
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
    /// The latest run's state of either source, where the other tabs put their
    /// toolbar, plus a refresh for the pass that filled it.
    fn render_toolbar(&self, latest: Option<&ActivityRun>) -> AnyElement {
        let (glyph, color, word, busy) = match latest {
            None => (
                "icons/icon-clock.svg",
                theme::text_muted(),
                "No runs yet".to_string(),
                false,
            ),
            Some(run) if run.is_running() => (
                "icons/icon-spinner.svg",
                theme::BEHIND,
                "Latest run in progress".to_string(),
                true,
            ),
            Some(run) if run.is_failed() => (
                "icons/icon-close.svg",
                theme::DANGER,
                "Latest run failed".to_string(),
                false,
            ),
            Some(run) if run.is_succeeded() => (
                "icons/icon-check.svg",
                theme::SUCCESS_BRIGHT,
                "Latest run succeeded".to_string(),
                false,
            ),
            Some(_) => (
                "icons/icon-clock.svg",
                theme::text_muted(),
                "Latest run queued".to_string(),
                false,
            ),
        };
        let store = self.store.clone();
        let source = latest.map(|run| run.source);
        let latest = div()
            .flex()
            .items_center()
            .gap_2()
            .text_size(px(theme::font_body()))
            .text_color(theme::text_dim())
            .child(if busy {
                div()
                    .id("azure-latest-status")
                    .role(gpui::Role::Status)
                    .aria_label("Latest run in progress")
                    .flex()
                    .items_center()
                    .child(motion::spinner(
                        "azure-latest-spinner",
                        "icons/icon-spinner.svg",
                        px(theme::icon_small()),
                        color,
                    ))
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
            .border_color(theme::hairline())
            .child(latest)
            .child(div().flex_1())
            .children(source.map(|source| source_tag(source.label())))
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
                    .hover(|s| s.bg(theme::raised()))
                    .on_click(move |_, _, cx| {
                        store.update(cx, |s, cx| s.refresh_panel(cx));
                    })
                    .child(icon(
                        "icons/icon-refresh.svg",
                        px(theme::icon_small()),
                        theme::text_muted(),
                    )),
            )
            .into_any_element()
    }

    fn render_workbench(
        &mut self,
        runs: &[ActivityRun],
        azure: Option<&AzureSnapshot>,
        github_ok: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pane_width = self.store.read(cx).pane_width;
        let stacked = pane_width < STACKED_PANE;
        // the first run is the selection where both columns show; the pane
        // never opens on an empty detail
        let open = self.store.read(cx).open_run.clone();
        if !stacked && open.is_none() && !runs.is_empty() {
            let first = runs[0].clone();
            self.store.update(cx, |store, cx| store.open_run(first, cx));
        }
        let selected = self
            .store
            .read(cx)
            .open_run
            .as_ref()
            .map(|open| (open.run.source, open.run.id));
        let store = self.store.clone();
        let this = cx.entity();

        let mut rows = div().flex().flex_col();
        for (ix, run) in runs.iter().enumerate() {
            rows = rows.child(self.render_run_row(
                ix,
                run,
                selected == Some((run.source, run.id)),
                &store,
                &this,
            ));
        }
        if runs.is_empty() {
            rows = rows.child(no_runs_note(azure.is_some(), github_ok));
        }
        let mut sources: Vec<&'static str> = Vec::new();
        if azure.is_some_and(|snapshot| !snapshot.pipelines.is_empty()) {
            sources.push(ActivitySource::Azure.label());
        }
        if github_ok {
            sources.push(ActivitySource::GitHub.label());
        }
        let list = div()
            .id("azure-runs-column")
            .debug_selector(|| "azure-runs-column".into())
            .w(px(WORK_LIST))
            .flex_shrink_0()
            .h_full()
            .flex()
            .flex_col()
            .border_r_1()
            .border_color(theme::hairline())
            .bg(theme::surface())
            .child(
                div()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap_2()
                    .h(px(40.))
                    .px_4()
                    .border_b_1()
                    .border_color(theme::hairline())
                    .text_size(px(theme::font_meta()))
                    .text_color(theme::text_dim())
                    .child(if runs.is_empty() {
                        "No runs".to_string()
                    } else {
                        format!("Recent {}", runs.len())
                    })
                    .child(div().flex_1())
                    .children(sources.into_iter().map(source_tag)),
            )
            .child(
                div()
                    .id("azure-list")
                    .debug_selector(|| "azure-list".into())
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(rows),
            );
        let detail = div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .child(self.run_detail(window, cx));

        if stacked {
            if selected.is_some() {
                div()
                    .size_full()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .child(detail)
                    .into_any_element()
            } else {
                div()
                    .size_full()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .child(list)
                    .into_any_element()
            }
        } else {
            div()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_row()
                .child(list)
                .child(detail)
                .into_any_element()
        }
    }

    /// One run row: state glyph, workflow or pipeline name and run number,
    /// branch and state word, then its age and the source it came from.
    fn render_run_row(
        &self,
        ix: usize,
        run: &ActivityRun,
        selected: bool,
        store: &Entity<Store>,
        _this: &Entity<Self>,
    ) -> AnyElement {
        let (glyph, color) = run_glyph(run);
        let branch = run.display_branch();
        let age = run
            .at
            .as_deref()
            .and_then(warp_station_core::rel_time::format_rfc3339);
        let row_run = run.clone();
        let target = store.clone();
        let selector = format!("pipeline-run-{ix}");
        let title = format!("{} {}", run.name, run.label);
        div()
            .border_t_1()
            .border_color(theme::row_line())
            .child(
                div()
                    .id(SharedString::from(selector.clone()))
                    .debug_selector(move || selector.clone())
                    .a11y_button(title.clone())
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .gap_2p5()
                    .min_h(px(58.))
                    .pl(px(13.))
                    .pr_2p5()
                    .py_2()
                    .border_l(px(3.))
                    .border_color(if selected {
                        gpui::Hsla::from(theme::accent())
                    } else {
                        gpui::transparent_black()
                    })
                    .when(selected, |d| d.bg(theme::raised()))
                    .when_run_tint(run.is_failed())
                    .hover(|d| d.bg(theme::panel()))
                    .on_click(move |_, _, cx| {
                        target.update(cx, |s, cx| s.open_run(row_run.clone(), cx));
                    })
                    .child(run_glyph_element(ix, glyph, color))
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
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .text_size(px(theme::font_name()))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme::text())
                                    .child(title),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_1p5()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_size(px(theme::font_meta()))
                                    .text_color(theme::text_muted())
                                    .when_some(branch, |d, branch| {
                                        d.child(icon(
                                            "icons/icon-source-branch.svg",
                                            px(theme::icon_tiny()),
                                            theme::BRANCH,
                                        ))
                                        .child(
                                            div()
                                                .min_w_0()
                                                .overflow_hidden()
                                                .text_ellipsis()
                                                .font_family(gpui::SharedString::from(
                                                    theme::code_font(),
                                                ))
                                                .text_size(px(theme::font_mono_meta()))
                                                .text_color(theme::text_dim())
                                                .child(branch),
                                        )
                                        .child("·")
                                    })
                                    .child(run.state_word())
                                    .child("·")
                                    .child(source_tag(run.source.label())),
                            ),
                    )
                    .children(age.map(|age| {
                        div()
                            .flex_shrink_0()
                            .text_size(px(theme::font_meta()))
                            .text_color(theme::text_muted())
                            .child(age)
                    })),
            )
            .into_any_element()
    }

    /// The selected run's detail, or the empty state that says how to pick one.
    fn run_detail(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let store = self.store.read(cx);
        let Some(open) = store.open_run.clone() else {
            return diff_controls::state(if self.stacked(cx) {
                "Select a run"
            } else {
                "Select a run from the list"
            });
        };
        let detail = open.detail.as_ref();
        // the detail's own call is fresher than the row's snapshot, and for
        // Actions it is the only place the commit and the trigger exist
        let run = effective(&open.run, detail);

        let mut title_row = div().flex().flex_row().flex_wrap().items_center().gap_2();
        if self.stacked(cx) {
            let store = self.store.clone();
            title_row = title_row.child(
                div()
                    .id("run-back")
                    .debug_selector(|| "run-back".into())
                    .a11y_button("Back to the runs")
                    .cursor_pointer()
                    .flex_shrink_0()
                    .size(px(shell::CONTROL_HEIGHT))
                    .rounded(px(6.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .hover(|s| s.bg(theme::raised()))
                    .on_click(move |_, _, cx| {
                        store.update(cx, |store, cx| {
                            store.close_run(cx);
                        });
                    })
                    .child(icon(
                        "icons/icon-arrow-left.svg",
                        px(theme::icon_small()),
                        theme::text_dim(),
                    )),
            );
        }
        title_row = title_row
            .child(
                div()
                    .min_w(px(220.))
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(px(theme::font_screen_title()))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme::text())
                    .child(format!("{} {}", run.name, run.label)),
            )
            .child(state_pill(&run));
        let url = run.url.clone().unwrap_or_default();
        if !url.is_empty() {
            title_row = title_row.child(primary_button("run-open", "Open run", move |_, _, _| {
                let _ = warp_station_core::terminal::open_folder(&url);
            }));
        }

        let branch = run.display_branch();
        let sub = div()
            .mt_2()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .gap(px(14.))
            .text_size(px(theme::font_meta()))
            .text_color(theme::text_dim())
            .when_some(branch, |d, branch| {
                d.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1p5()
                        .child(icon(
                            "icons/icon-source-branch.svg",
                            px(theme::icon_tiny()),
                            theme::BRANCH,
                        ))
                        .child(
                            div()
                                .font_family(theme::code_font())
                                .text_size(px(theme::font_mono_meta()))
                                .text_color(theme::text_dim())
                                .child(branch),
                        ),
                )
            })
            .children(run.short_sha().map(|sha| {
                div()
                    .font_family(theme::code_font())
                    .text_size(px(theme::font_mono_meta()))
                    .text_color(theme::text_muted())
                    .child(sha)
            }))
            .child(source_tag(run.source.label()));

        let head = div()
            .flex_shrink_0()
            .px(px(shell::DETAIL_PADDING))
            .pt_4()
            .pb(px(14.))
            .border_b_1()
            .border_color(theme::hairline())
            .flex()
            .flex_col()
            .child(title_row)
            .child(sub);

        let following = store.following_run(run.source, run.id);
        let strip = run_strip(&run, following);

        let body = div()
            .id("run-body")
            .debug_selector(|| "run-body".into())
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p(px(shell::DETAIL_PADDING))
            .child(
                div()
                    .max_w(px(BODY_WIDTH))
                    .flex()
                    .flex_col()
                    .gap_4()
                    .child(match (detail, open.failed) {
                        (Some(detail), _) => stages_body(detail, run.source),
                        (None, true) => empty_note(
                            "Could not read the run",
                            "The source did not answer with its jobs.",
                        ),
                        (None, false) => diff_controls::state("Reading the run…"),
                    })
                    .children(detail.map(|detail| log_card(&detail.log))),
            );

        div()
            .debug_selector(|| "run-detail".into())
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(theme::panel())
            .child(head)
            .child(strip)
            .child(body)
            .into_any_element()
    }
}

/// The row with the detail's fresher facts laid over it: the commit, the
/// trigger and the times only the detail's own call carries.
fn effective(run: &ActivityRun, detail: Option<&RunDetail>) -> ActivityRun {
    let mut run = run.clone();
    if let Some(detail) = detail {
        run.sha = detail.sha.clone().or(run.sha);
        run.trigger = detail.trigger.clone().or(run.trigger);
        run.start = detail.start.clone().or(run.start);
        run.finish = detail.finish.clone().or(run.finish);
    }
    run
}

/// A repository with no runs, in both sources' words: which one answered is
/// part of the message, so an unconnected source is never read as "nothing
/// ran".
fn no_runs_note(azure: bool, github: bool) -> AnyElement {
    let detail = match (azure, github) {
        (true, true) => "GitHub and Azure DevOps both answered with no runs.",
        (true, false) => "Azure DevOps answered; GitHub is not connected here.",
        (false, true) => "GitHub answered; Azure DevOps is not connected here.",
        (false, false) => "Neither source is connected here.",
    };
    div()
        .debug_selector(|| "runs-empty".into())
        .px_4()
        .py_3()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .text_size(px(theme::font_body()))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::text_dim())
                .child("Nothing has run yet"),
        )
        .child(
            div()
                .text_size(px(theme::font_meta()))
                .text_color(theme::text_muted())
                .child(detail),
        )
        .into_any_element()
}

/// The run's state glyph and its ink.
fn run_glyph(run: &ActivityRun) -> (&'static str, Rgba) {
    if run.is_running() {
        ("icons/icon-spinner.svg", theme::BEHIND)
    } else if run.is_failed() {
        ("icons/icon-close.svg", theme::DANGER)
    } else if run.is_succeeded() {
        ("icons/icon-check.svg", theme::SUCCESS_BRIGHT)
    } else {
        ("icons/icon-clock.svg", theme::text_muted())
    }
}

fn run_glyph_element(ix: usize, glyph: &'static str, color: Rgba) -> AnyElement {
    if glyph == "icons/icon-spinner.svg" {
        return motion::spinner(
            SharedString::from(format!("pipeline-spin-{ix}")),
            "icons/icon-spinner.svg",
            px(theme::icon_medium()),
            color,
        )
        .into_any_element();
    }
    icon(glyph, px(theme::icon_medium()), color).into_any_element()
}

/// The 44 status strip: the state, when it started, how long it took, what
/// started it, and whether the pane is following it — the facts the head has
/// no room for.
fn run_strip(run: &ActivityRun, following: bool) -> AnyElement {
    let (glyph, color) = run_glyph(run);
    let mut facts = div()
        .flex()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap(px(18.))
        .child(
            div()
                .flex()
                .items_center()
                .gap_1p5()
                .text_color(color)
                .child(if run.is_running() {
                    div()
                        .id("run-strip-status")
                        .role(gpui::Role::Status)
                        .aria_label("Run in progress")
                        .flex()
                        .items_center()
                        .child(motion::spinner(
                            "run-strip-spinner",
                            "icons/icon-spinner.svg",
                            px(theme::icon_tiny()),
                            color,
                        ))
                        .into_any_element()
                } else {
                    icon(glyph, px(theme::icon_tiny()), color).into_any_element()
                })
                .child(run.state_word()),
        );
    if let Some(started) = run
        .start
        .as_deref()
        .and_then(warp_station_core::rel_time::format_rfc3339)
    {
        facts = facts.child(format!("Started {started}"));
    }
    if let Some(duration) = duration_between(run.start.as_deref(), run.finish.as_deref()) {
        facts = facts.child(format!("Duration {duration}"));
    }
    if let Some(trigger) = run.trigger_word() {
        facts = facts.child(format!("Trigger {trigger}"));
    }
    if following {
        // the word says why the cards move on their own; the spinner above is
        // the run's, so this one stays a fact
        facts = facts.child(
            div()
                .id("run-following")
                .debug_selector(|| "run-following".into())
                .role(gpui::Role::Status)
                .aria_label("Following this run")
                .text_color(theme::text_muted())
                .child("Following"),
        );
    }
    div()
        .debug_selector(|| "run-status".into())
        .flex_shrink_0()
        .min_h(px(44.))
        .py(px(6.))
        .px(px(shell::DETAIL_PADDING))
        .flex()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap(px(16.))
        .bg(theme::surface())
        .border_b_1()
        .border_color(theme::hairline())
        .text_size(px(theme::font_meta()))
        .text_color(theme::text_dim())
        .child(facts)
        .child(div().flex_1())
        .child(source_tag(run.source.label()))
        .into_any_element()
}

fn state_pill(run: &ActivityRun) -> AnyElement {
    let (glyph, color) = run_glyph(run);
    div()
        .flex_shrink_0()
        .flex()
        .items_center()
        .gap_1()
        .h(px(24.))
        .px_2()
        .rounded_full()
        .bg(theme::tint(color))
        .text_size(px(theme::font_meta()))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(color)
        .child(icon(glyph, px(theme::icon_tiny()), color))
        .child(run.state_word())
        .into_any_element()
}

/// One card per stage, its jobs under it. A failed job takes the removed-line
/// tint, a running one a spinner, a queued or skipped one a clock.
fn stages_body(detail: &RunDetail, source: ActivitySource) -> AnyElement {
    if detail.stages.is_empty() {
        return empty_note(
            "No stages reported",
            match source {
                ActivitySource::Azure => "Azure DevOps answered with an empty timeline.",
                ActivitySource::GitHub => "GitHub answered with no jobs for this run.",
            },
        );
    }
    let mut column = div().flex().flex_col().gap_4();
    for (ix, stage) in detail.stages.iter().enumerate() {
        column = column.child(stage_card(ix, stage));
    }
    column.into_any_element()
}

fn stage_card(ix: usize, stage: &RunStage) -> AnyElement {
    let failed = stage.failed();
    let running = stage.jobs.iter().any(|job| job.is_running());
    let (glyph, color) = if failed > 0 {
        ("icons/icon-close.svg", theme::DANGER)
    } else if running {
        ("icons/icon-spinner.svg", theme::BEHIND)
    } else if !stage.jobs.is_empty() && stage.done() == stage.jobs.len() {
        ("icons/icon-check.svg", theme::SUCCESS_BRIGHT)
    } else {
        ("icons/icon-clock.svg", theme::text_muted())
    };
    let mut card = div()
        .debug_selector(move || format!("run-stage-{ix}"))
        .rounded(px(8.))
        .border_1()
        .border_color(theme::hairline())
        .bg(theme::surface())
        .overflow_hidden()
        .flex()
        .flex_col()
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .min_h(px(44.))
                .px_4()
                .child(if glyph == "icons/icon-spinner.svg" {
                    div()
                        .id(SharedString::from(format!("run-stage-status-{ix}")))
                        .role(gpui::Role::Status)
                        .aria_label(format!("Running stage {}", stage.name))
                        .flex()
                        .items_center()
                        .child(motion::spinner(
                            SharedString::from(format!("run-stage-spin-{ix}")),
                            "icons/icon-spinner.svg",
                            px(theme::icon_small()),
                            color,
                        ))
                        .into_any_element()
                } else {
                    icon(glyph, px(theme::icon_small()), color).into_any_element()
                })
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(px(theme::font_name()))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::text())
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(stage.name.clone()),
                )
                .child(
                    div()
                        .text_size(px(theme::font_meta()))
                        .text_color(theme::text_muted())
                        .child(format!(
                            "{}/{} job{}",
                            stage.done(),
                            stage.jobs.len(),
                            plural(stage.jobs.len())
                        )),
                ),
        );
    for (job_ix, job) in stage.jobs.iter().enumerate() {
        card = card.child(job_row(ix, job_ix, job));
    }
    card.into_any_element()
}

fn job_row(stage_ix: usize, job_ix: usize, job: &RunJob) -> AnyElement {
    let (glyph, color) = if job.is_failed() {
        ("icons/icon-close.svg", theme::DANGER)
    } else if job.is_running() {
        ("icons/icon-spinner.svg", theme::BEHIND)
    } else if job.is_done() {
        ("icons/icon-check.svg", theme::SUCCESS_BRIGHT)
    } else {
        ("icons/icon-clock.svg", theme::text_muted())
    };
    let time = job.duration().unwrap_or_default();
    let selector = format!("run-job-{stage_ix}-{job_ix}");
    let spinner_id = selector.clone();
    div()
        .id(SharedString::from(selector.clone()))
        .debug_selector(move || selector.clone())
        .a11y_button(job.name.clone())
        .min_h(px(40.))
        .px_4()
        .flex()
        .items_center()
        .gap_2p5()
        .border_t_1()
        .border_color(theme::row_line())
        .when(job.is_failed(), |d| d.bg(theme::DIFF_DEL_BG))
        .text_size(px(theme::font_body()))
        .text_color(theme::text())
        .child(if glyph == "icons/icon-spinner.svg" {
            motion::spinner(
                SharedString::from(spinner_id),
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
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(job.name.clone()),
        )
        .when(!time.is_empty(), |d| {
            d.child(
                div()
                    .flex_shrink_0()
                    .font_family(theme::code_font())
                    .text_size(px(theme::font_mono_meta()))
                    .text_color(theme::text_muted())
                    .child(time),
            )
        })
        .into_any_element()
}

/// The run's closing log, tinted where it says something went wrong.
fn log_card(log: &str) -> AnyElement {
    let card = div()
        .debug_selector(|| "run-log".into())
        .rounded(px(8.))
        .border_1()
        .border_color(theme::hairline())
        .bg(theme::surface())
        .overflow_hidden()
        .flex()
        .flex_col()
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .min_h(px(44.))
                .px_4()
                .border_b_1()
                .border_color(theme::row_line())
                .child(
                    div()
                        .flex_1()
                        .text_size(px(theme::font_name()))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::text())
                        .child("Log"),
                )
                .child(
                    div()
                        .text_size(px(theme::font_meta()))
                        .text_color(theme::text_muted())
                        .child("Azure Pipelines"),
                ),
        );
    if log.trim().is_empty() {
        return card
            .child(
                div()
                    .px_4()
                    .py_3()
                    .text_size(px(theme::font_meta()))
                    .text_color(theme::text_muted())
                    .child("No log for this run."),
            )
            .into_any_element();
    }
    let mut lines = div()
        .id("run-log-lines")
        .debug_selector(|| "run-log-lines".into())
        .max_h(px(320.))
        .overflow_y_scroll()
        .p_3()
        .flex()
        .flex_col()
        .bg(theme::well())
        .font_family(theme::code_font())
        .text_size(px(theme::font_mono_meta()));
    if log.trim().is_empty() {
        // a run with nothing to report: an Azure build without log entries,
        // or an Actions run whose steps all passed
        lines = lines.child(
            div()
                .text_color(theme::text_muted())
                .child("Nothing logged for this run"),
        );
    }
    for line in log
        .lines()
        .rev()
        .take(LOG_ROWS)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
    {
        let color = log_line_color(line);
        lines = lines.child(div().whitespace_nowrap().text_color(color).child(
            if line.is_empty() {
                " ".to_string()
            } else {
                line.to_string()
            },
        ));
    }
    card.child(lines).into_any_element()
}

/// A log line's ink: failures read first, a closing success a step quieter.
fn log_line_color(line: &str) -> Rgba {
    let lower = line.to_ascii_lowercase();
    if lower.contains("error")
        || lower.contains("##[error]")
        || lower.contains("failed")
        || lower.contains("fatal")
    {
        theme::DANGER
    } else if lower.contains("succeeded") || lower.contains("completed successfully") {
        theme::SUCCESS_BRIGHT
    } else {
        theme::text_dim()
    }
}

/// A failed run's row keeps the removed-line tint, as the design asks.
trait TintedRow: Sized {
    fn when_run_tint(self, failed: bool) -> Self;
}

impl TintedRow for gpui::Stateful<gpui::Div> {
    fn when_run_tint(self, failed: bool) -> Self {
        if failed {
            self.bg(theme::tint(theme::DEL))
        } else {
            self
        }
    }
}

fn empty_note(title: &'static str, detail: &'static str) -> AnyElement {
    div()
        .flex_1()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_1()
        .p_6()
        .child(
            div()
                .text_size(px(theme::font_heading()))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::text_dim())
                .child(title),
        )
        .child(
            div()
                .text_size(px(theme::font_meta()))
                .text_color(theme::text_muted())
                .child(detail),
        )
        .into_any_element()
}

fn empty_state(text: String) -> gpui::AnyElement {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .p_6()
        .text_size(px(theme::font_body()))
        .text_color(theme::text_faint())
        .child(text)
        .into_any_element()
}

/// The one filled action: a run's "Open run".
fn primary_button(
    id: &'static str,
    label: &'static str,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> AnyElement {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .a11y_button(label)
        .flex_shrink_0()
        .h(px(shell::CONTROL_HEIGHT))
        .px_3()
        .rounded(px(6.))
        .bg(theme::accent())
        .flex()
        .items_center()
        .gap_2()
        .text_size(px(theme::font_control()))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme::on_accent())
        .cursor_pointer()
        .hover(|s| s.bg(theme::accent_hover()))
        .on_click(on_click)
        .child(icon(
            "icons/icon-open-in-new.svg",
            px(theme::icon_small()),
            theme::on_accent(),
        ))
        .child(label)
        .into_any_element()
}
