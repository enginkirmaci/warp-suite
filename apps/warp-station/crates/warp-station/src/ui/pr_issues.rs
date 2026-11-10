//! The detail pane's Pull requests and Issues tabs. Both open on a toolbar with
//! Open and Closed and the New verb. Pull requests is the design's workbench: a
//! 320 work list beside the selected pull request, or — below 900 px of pane —
//! one at a time with Back. Issues is still one full-width list, with its
//! filter field and Sort menu; a pull request row opens in the pane, and an
//! issue row opens on GitHub.

use crate::store::Store;
use crate::theme;
use crate::ui::A11y;
use crate::ui::icon;
use crate::ui::new_pull::NewPullView;
use crate::ui::pull_request::PullRequestView;
use crate::ui::repos::{segment, segment_row};
use gpui::prelude::*;
use gpui::{Context, Entity, FontWeight, SharedString, Window, div, px};
use gpui_kit::component::input::{InputEvent, InputState};
use warp_station_core::github::{GitHubItem, IssueSort};
use warpui_kit_core::shell;

/// The rows' measure, as the design caps a list inside the detail pane.
const LIST_WIDTH: f32 = 920.;
/// The workbench's list column, and the pane width under which it and the
/// pull request stop sharing the pane.
const WORK_LIST: f32 = 320.;
const STACKED_PANE: f32 = 900.;

pub struct ItemListView {
    store: Entity<Store>,
    pull_requests: bool,
    /// The pull request the workbench shows beside the list.
    pull: Entity<PullRequestView>,
    /// The form that replaces it while a pull request is being written.
    new_pull: Entity<NewPullView>,
    filter: Entity<InputState>,
    query: String,
    /// The filter resets when the view changes.
    folder: Option<String>,
    closed: bool,
    sort_open: bool,
    /// Setting the field needs the window, so this waits for the next render.
    clear_filter: bool,
}

impl ItemListView {
    pub fn new(
        store: Entity<Store>,
        pull: Entity<PullRequestView>,
        new_pull: Entity<NewPullView>,
        pull_requests: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Filter issues"));
        cx.subscribe(&filter, |this, filter, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.query = filter.read(cx).value().trim().to_lowercase();
                cx.notify();
            }
        })
        .detach();
        Self {
            store,
            pull_requests,
            pull,
            new_pull,
            filter,
            query: String::new(),
            folder: None,
            closed: false,
            sort_open: false,
            clear_filter: false,
        }
    }

    /// Escape: the workbench's form or pull request first, then the Sort menu,
    /// then the filter. Returns whether it took the key.
    pub fn dismiss(&mut self, cx: &mut Context<Self>) -> bool {
        // the workbench's pull request is the innermost surface on the tab
        if self.pull_requests {
            if self.new_pull.update(cx, |view, cx| view.dismiss(cx)) {
                return true;
            }
            if self.pull.update(cx, |view, cx| view.dismiss(cx)) {
                return true;
            }
            if self.store.read(cx).new_pull.is_some() {
                // below 900 px the form is the whole pane, so Escape is Back
                if self.new_pull.read(cx).stacked {
                    self.store.update(cx, |store, cx| store.close_new_pull(cx));
                    return true;
                }
            } else if self.pull.read(cx).stacked && self.store.read(cx).open_pull.is_some() {
                self.store
                    .update(cx, |store, cx| store.close_pull_request(cx));
                return true;
            }
        }
        if self.sort_open {
            self.sort_open = false;
            cx.notify();
            return true;
        }
        if !self.query.is_empty() {
            self.clear_filter = true;
            cx.notify();
            return true;
        }
        false
    }

    fn set_closed(&mut self, closed: bool, cx: &mut Context<Self>) {
        self.closed = closed;
        self.sort_open = false;
        // the filter belongs to the view it was typed in, not to the tab
        self.clear_filter = !self.query.is_empty();
        // the selected pull request belongs to the list it came from
        if self.pull_requests {
            self.store.update(cx, |store, cx| {
                store.close_new_pull(cx);
                store.close_pull_request(cx);
            });
        }
        if closed {
            let pull_requests = self.pull_requests;
            self.store.update(cx, |s, _| s.load_closed(pull_requests));
        }
        cx.notify();
    }

    // the filter field needs the window to draw its focus edge
    #[allow(clippy::too_many_arguments)]
    fn render_toolbar(
        &self,
        open_count: usize,
        closed_count: Option<usize>,
        new_url: Option<String>,
        show_controls: bool,
        sort: IssueSort,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let this = cx.entity();
        let closed_view = this.clone();
        let label = if self.pull_requests {
            "New pull request"
        } else {
            "New issue"
        };
        let (open_id, closed_id) = if self.pull_requests {
            ("pr-state-open", "pr-state-closed")
        } else {
            ("issue-state-open", "issue-state-closed")
        };
        let store = self.store.clone();
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
            .child(
                segment_row()
                    .child(segment(
                        open_id,
                        "Open",
                        Some(open_count),
                        !self.closed,
                        move |_, _, cx| this.update(cx, |this, cx| this.set_closed(false, cx)),
                    ))
                    .child(segment(
                        closed_id,
                        "Closed",
                        closed_count,
                        self.closed,
                        move |_, _, cx| {
                            closed_view.update(cx, |this, cx| this.set_closed(true, cx))
                        },
                    )),
            )
            .when(show_controls, |bar| {
                bar.child(
                    div()
                        .w(px(240.))
                        .debug_selector(|| "issue-filter".into())
                        .child(warpui_kit_core::field::text(
                            "issue-filter-field",
                            &self.filter,
                            window,
                            cx,
                        )),
                )
                .child(
                    div()
                        .id("issue-sort")
                        .debug_selector(|| "issue-sort".into())
                        .a11y_button(format!("Sort: {}", sort.label()))
                        .cursor_pointer()
                        .h(px(shell::CONTROL_HEIGHT))
                        .px_2()
                        .rounded(px(6.))
                        .flex()
                        .items_center()
                        .gap_1()
                        .text_size(px(theme::font_control()))
                        .text_color(theme::text_muted())
                        .hover(|s| s.bg(theme::raised()))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.sort_open = !this.sort_open;
                            cx.notify();
                        }))
                        .child("Sort:")
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(theme::text())
                                .child(sort.label()),
                        )
                        .child(icon(
                            "icons/icon-chevron-down.svg",
                            px(theme::icon_tiny()),
                            theme::text_muted(),
                        )),
                )
            })
            .child(div().flex_1())
            .child(
                div()
                    .id("pr-refresh")
                    .a11y_button("Refresh panel")
                    .cursor_pointer()
                    .rounded(px(6.))
                    .size(px(shell::CONTROL_HEIGHT))
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
            .when_some(new_url, |bar, url| {
                // the workbench's detail owns the one filled primary, so the
                // list's New stays a plain button — and a pull request is
                // written in the pane, not in the browser
                let store = self.store.clone();
                let pull_requests = self.pull_requests;
                bar.child(
                    div()
                        .id("list-new")
                        .debug_selector(|| "list-new".into())
                        .a11y_button(label)
                        .cursor_pointer()
                        .h(px(shell::CONTROL_HEIGHT))
                        .px_3()
                        .rounded(px(6.))
                        .border_1()
                        .border_color(theme::line_strong())
                        .bg(theme::panel())
                        .flex()
                        .items_center()
                        .gap_2()
                        .text_size(px(theme::font_control()))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme::text())
                        .hover(|s| s.bg(theme::raised()))
                        .on_click(move |_, _, cx| {
                            if pull_requests {
                                store.update(cx, |store, cx| store.open_new_pull(cx));
                            } else {
                                let _ = warp_station_core::terminal::open_folder(&url);
                            }
                        })
                        .child(icon(
                            "icons/icon-plus.svg",
                            px(theme::icon_small()),
                            theme::text_dim(),
                        ))
                        .child(label),
                )
            })
    }

    fn render_sort_menu(&self, current: IssueSort, cx: &mut Context<Self>) -> gpui::AnyElement {
        let mut menu = div()
            .id("issue-sort-menu")
            .debug_selector(|| "issue-sort-menu".into())
            .absolute()
            .top(px(50.))
            .left(px(shell::DETAIL_PADDING + 390.))
            .w(px(200.))
            .p(px(4.))
            .rounded(px(8.))
            .border_1()
            .border_color(theme::line_strong())
            .bg(theme::overlay())
            .shadow_lg()
            .occlude()
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                this.sort_open = false;
                cx.notify();
            }))
            .flex()
            .flex_col()
            .gap(px(2.));
        for sort in IssueSort::ALL {
            let selector = format!("issue-sort-{}", sort.key());
            menu = menu.child(
                div()
                    .id(SharedString::from(selector.clone()))
                    .debug_selector(move || selector.clone())
                    .a11y_toggle(sort.label(), sort == current)
                    .cursor_pointer()
                    .rounded(px(6.))
                    .h(px(32.))
                    .px_2()
                    .flex()
                    .items_center()
                    .justify_between()
                    .text_size(px(theme::font_control()))
                    .text_color(theme::text())
                    .hover(|d| d.bg(theme::line_strong()))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.store.update(cx, |s, cx| s.set_issue_sort(sort, cx));
                        this.sort_open = false;
                        cx.notify();
                    }))
                    .child(sort.label())
                    .when(sort == current, |d| {
                        d.child(icon(
                            "icons/icon-check.svg",
                            px(theme::icon_small()),
                            theme::accent_ink(),
                        ))
                    }),
            );
        }
        gpui::deferred(menu).into_any_element()
    }
}

impl Render for ItemListView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        let (folder, snapshot, sort) = {
            let store = self.store.read(cx);
            let folder = store.selected_repo.clone();
            let snapshot = folder
                .as_ref()
                .and_then(|f| store.github_by_folder.get(f).cloned());
            (folder, snapshot, store.issue_sort())
        };
        if self.folder != folder {
            self.folder = folder.clone();
            self.closed = false;
            self.sort_open = false;
            self.clear_filter = !self.query.is_empty();
        }
        // a pull request that was just created lands in the Open list
        if self
            .store
            .update(cx, |store, _| std::mem::take(&mut store.pull_created))
        {
            self.closed = false;
        }
        if std::mem::take(&mut self.clear_filter) {
            self.filter
                .update(cx, |input, cx| input.set_value("", window, cx));
            self.query.clear();
        }
        let title = if self.pull_requests {
            "pull requests"
        } else {
            "issues"
        };

        let Some(snapshot) = snapshot else {
            return tab_card(empty_state(format!(
                "No GitHub activity loaded — the {title} tab fills after the gh pass settles"
            )));
        };
        if !snapshot.available {
            return tab_card(empty_state(
                "This repository is not on GitHub (or the gh CLI could not authenticate)".into(),
            ));
        }

        let open_items = if self.pull_requests {
            &snapshot.pulls
        } else {
            &snapshot.issues
        };
        let (closed_items, closed_failed) = {
            let store = self.store.read(cx);
            let failed = folder.as_ref().is_some_and(|f| {
                store
                    .closed_failed
                    .contains(&(f.clone(), self.pull_requests))
            });
            let items = folder.as_ref().and_then(|f| {
                store
                    .github_closed
                    .get(&(f.clone(), self.pull_requests))
                    .cloned()
            });
            (items, failed)
        };
        let mut items: Vec<GitHubItem> = if self.closed {
            closed_items.as_deref().cloned().unwrap_or_default()
        } else {
            open_items.clone()
        };
        let total = items.len();
        // The store dedupes (in-flight, cached, or failed-waiting-for-Retry),
        // so asking here is safe: Refresh clears the guard and the next draw
        // re-asks instead of leaving a stale or endless "Loading…".
        if self.closed {
            let pull_requests = self.pull_requests;
            self.store.update(cx, |s, _| s.load_closed(pull_requests));
        }
        // A failed load is not "still loading": the tab must offer a retry
        // instead of spinning (audit A2).
        let loading = self.closed && closed_items.is_none() && !closed_failed;
        let controls = !self.pull_requests && total > 0;
        if !self.pull_requests {
            sort.sort(&mut items);
            items.retain(|item| item.matches(&self.query));
        }

        let new_url = snapshot.url.as_ref().map(|url| {
            let tail = if self.pull_requests {
                "compare"
            } else {
                "issues/new"
            };
            format!("{url}/{tail}")
        });
        let toolbar = self.render_toolbar(
            open_items.len(),
            closed_items.as_ref().map(|items| items.len()),
            new_url,
            controls,
            sort,
            window,
            cx,
        );
        let menu = (self.sort_open && controls).then(|| self.render_sort_menu(sort, cx));

        let list_id = if self.pull_requests {
            "pr-list"
        } else {
            "issue-list"
        };
        let state = if self.closed { "closed" } else { "open" };
        let repo_name = self
            .store
            .read(cx)
            .selected_entry()
            .map(|repo| repo.name.clone())
            .unwrap_or_default();
        let body = if self.closed && closed_failed {
            let retry_store = self.store.clone();
            let pull_requests = self.pull_requests;
            div()
                .flex_1()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_3()
                .child(
                    div()
                        .text_size(px(theme::font_body()))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::text_dim())
                        .child(format!("Could not load closed {title}.")),
                )
                .child(
                    div()
                        .id("closed-retry")
                        .debug_selector(|| "closed-retry".into())
                        .a11y_button("Retry loading the closed list")
                        .cursor_pointer()
                        .h(px(shell::CONTROL_HEIGHT))
                        .px_3()
                        .rounded(px(6.))
                        .flex()
                        .items_center()
                        .bg(theme::raised())
                        .text_size(px(theme::font_control()))
                        .text_color(theme::text())
                        .hover(|s| s.bg(theme::accent_soft()))
                        .on_click(move |_, _, cx| {
                            retry_store.update(cx, |s, cx| {
                                s.retry_closed(pull_requests);
                                cx.notify();
                            });
                        })
                        .child("Retry"),
                )
                .into_any_element()
        } else if loading {
            empty_state(format!("Loading closed {title}…"))
        } else if total == 0 {
            empty_note(
                format!("No {state} {title}"),
                if repo_name.is_empty() {
                    String::new()
                } else {
                    format!("{repo_name} has nothing waiting here.")
                },
            )
        } else if items.is_empty() {
            let query = self.filter.read(cx).value().trim().to_string();
            no_match(&query, cx)
        } else if self.pull_requests {
            self.render_workbench(&items, cx)
        } else {
            let mut list = div().max_w(px(LIST_WIDTH)).flex().flex_col();
            for item in &items {
                list = list.child(self.render_row(item, cx));
            }
            div()
                .id(list_id)
                .debug_selector(move || list_id.into())
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .px(px(shell::DETAIL_PADDING - 8.))
                .py_2()
                .child(list)
                .into_any_element()
        };

        div()
            .relative()
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .child(toolbar)
            .child(body)
            .children(menu)
            .into_any_element()
    }
}

impl ItemListView {
    /// The design's workbench: the 320 work list, a hairline, and the selected
    /// pull request. Below 900 px of pane the two take turns and Back returns
    /// to the list.
    fn render_workbench(
        &mut self,
        items: &[GitHubItem],
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let stacked = self.store.read(cx).pane_width < STACKED_PANE;
        // E2E hook: WARP_STATION_NEW_PULL=1 opens the form once, for a capture
        if self
            .store
            .update(cx, |store, _| std::mem::take(&mut store.new_pull_requested))
        {
            self.store.update(cx, |store, cx| store.open_new_pull(cx));
        }
        let writing = self.store.read(cx).new_pull.is_some();
        // the first visible row is the selection where both columns show, but
        // never over the form that is already being written
        if !stacked && !writing && self.store.read(cx).open_pull.is_none() {
            let first = items[0].clone();
            self.store
                .update(cx, |store, cx| store.open_pull_request(first, cx));
        }
        self.pull.update(cx, |view, _| view.stacked = stacked);
        self.new_pull.update(cx, |view, _| view.stacked = stacked);
        let selected = self
            .store
            .read(cx)
            .open_pull
            .as_ref()
            .map(|open| open.item.number);

        let mut rows = div().flex().flex_col();
        for item in items {
            rows = rows.child(self.render_pr_row(item, selected == Some(item.number)));
        }
        let label = if self.closed { "Closed" } else { "Open" };
        let list = div()
            .id("pr-list-column")
            .debug_selector(|| "pr-list-column".into())
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
                    .child(format!("{label} {}", items.len()))
                    .child(div().flex_1())
                    .child(source_tag()),
            )
            .child(
                div()
                    .id("pr-list")
                    .debug_selector(|| "pr-list".into())
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
            .child(if writing {
                self.new_pull.clone().into_any_element()
            } else {
                self.pull.clone().into_any_element()
            });
        if stacked {
            if writing || selected.is_some() {
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

    /// A pull request row: 58 high, the reference and title, then head → base,
    /// its state and the source. The selected row takes `raised` and the 3 px
    /// accent edge.
    fn render_pr_row(&self, item: &GitHubItem, selected: bool) -> impl IntoElement + use<> {
        let closed = self.closed;
        let target = item.clone();
        let store = self.store.clone();
        let age = item
            .updated_at
            .as_deref()
            .and_then(warp_station_core::rel_time::format_rfc3339);
        let head = item.head_ref_name.clone().unwrap_or_default();
        let base = item.base_ref_name.clone().unwrap_or_default();
        let state = if closed {
            "closed"
        } else if item.is_draft {
            "draft"
        } else {
            "open"
        };
        let selector = format!("PR-{}", item.number);
        div().border_t_1().border_color(theme::row_line()).child(
            div()
                .id(SharedString::from(selector.clone()))
                .debug_selector(move || selector.clone())
                .a11y_button(format!("#{} {}", item.number, item.title))
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
                .hover(|d| d.bg(theme::panel()))
                .on_click(move |_, _, cx| {
                    let item = target.clone();
                    store.update(cx, |s, cx| s.open_pull_request(item, cx));
                })
                .child(icon(
                    "icons/icon-git-pull-request.svg",
                    px(theme::icon_medium()),
                    if closed {
                        theme::text_muted()
                    } else {
                        theme::PR
                    },
                ))
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
                                .text_color(if closed {
                                    theme::text_dim()
                                } else {
                                    theme::text()
                                })
                                .child(format!("#{} {}", item.number, item.title)),
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
                                .when(!head.is_empty(), |d| {
                                    d.child(
                                        div()
                                            .min_w_0()
                                            .overflow_hidden()
                                            .text_ellipsis()
                                            .font_family(gpui::SharedString::from(
                                                theme::code_font(),
                                            ))
                                            .text_size(px(theme::font_mono_meta()))
                                            .child(format!("{head} → {base}")),
                                    )
                                    .child("·")
                                })
                                .child(state)
                                .child("·")
                                .child(source_tag()),
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
    }

    fn render_row(&self, item: &GitHubItem, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let closed = self.closed;
        let opens_here = item.is_pull_request && !closed;
        let target = item.clone();
        let url = item.url.clone();
        let store = self.store.clone();
        let kind = if item.is_pull_request { "PR" } else { "Issue" };
        let glyph_color = match (closed, item.is_pull_request) {
            (true, _) => theme::text_muted(),
            (false, true) => theme::PR,
            (false, false) => theme::ISSUE,
        };
        let age = item
            .updated_at
            .as_deref()
            .and_then(warp_station_core::rel_time::format_rfc3339);

        let mut second_line = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_1p5()
            .min_w_0()
            .overflow_hidden()
            .whitespace_nowrap()
            .text_size(px(theme::font_meta()))
            .text_color(theme::text_muted());
        if item.is_pull_request {
            let head = item.head_ref_name.clone().unwrap_or_default();
            let base = item.base_ref_name.clone().unwrap_or_default();
            if !head.is_empty() {
                second_line = second_line.child(
                    div()
                        .font_family(gpui::SharedString::from(theme::code_font()))
                        .text_size(px(theme::font_mono_meta()))
                        .child(format!("{head} → {base}")),
                );
            }
        }
        for label in &item.labels {
            second_line = second_line.child(
                div()
                    .rounded(px(4.))
                    .bg(theme::raised())
                    .px_1p5()
                    .text_color(theme::text_dim())
                    .child(label.clone()),
            );
        }
        if !item.is_pull_request {
            let word = if item.comments == 1 {
                "comment"
            } else {
                "comments"
            };
            second_line = second_line.child(format!("{} {word}", item.comments));
        }

        let mut right = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .flex_shrink_0()
            .text_size(px(theme::font_meta()))
            .text_color(theme::text_muted())
            .child(item.author.clone().unwrap_or_else(|| "ghost".into()))
            .children(age);
        if item.is_draft {
            right = right.child(pill("Draft", theme::text_muted()));
        } else if item.is_pull_request && !closed {
            right = right.child(match item.review_decision.as_deref() {
                Some("APPROVED") => pill("Approved", theme::SUCCESS_BRIGHT),
                Some("CHANGES_REQUESTED") => pill("Changes requested", theme::DANGER),
                _ => pill("Review", theme::PR),
            });
        }

        let selector = format!("{kind}-{}", item.number);
        div()
            .id(SharedString::from(selector.clone()))
            .debug_selector(move || selector.clone())
            .a11y_button(format!(
                "{} #{} {} — {}",
                kind,
                item.number,
                item.title,
                item.author.clone().unwrap_or_else(|| "ghost".into())
            ))
            .cursor_pointer()
            .flex()
            .flex_row()
            .items_center()
            .gap_3()
            .rounded(px(6.))
            .px_2()
            .py_2p5()
            .hover(|s| s.bg(theme::raised()))
            .on_click(cx.listener(move |_, _, _, cx| {
                if opens_here {
                    let item = target.clone();
                    store.update(cx, |s, cx| s.open_pull_request(item, cx));
                } else if !url.is_empty() {
                    let _ = warp_station_core::terminal::open_folder(&url);
                }
            }))
            .child(icon(
                if item.is_pull_request {
                    "icons/icon-git-pull-request.svg"
                } else {
                    "icons/icon-issue.svg"
                },
                px(theme::icon_medium()),
                glyph_color,
            ))
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
                            .text_size(px(theme::font_name()))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme::text())
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(format!("#{} {}", item.number, item.title)),
                    )
                    .child(second_line),
            )
            .child(right)
    }
}

fn pill(label: &'static str, color: gpui::Rgba) -> impl IntoElement + use<> {
    div()
        .rounded_full()
        .bg(theme::tint(color))
        .px_2()
        .py_0p5()
        .text_size(px(theme::font_meta()))
        .text_color(color)
        .child(label)
}

/// The quiet source tag the merged lists carry: it says where the row will
/// open, on the raised surface and in the muted ink, never the entity hue.
fn source_tag() -> impl IntoElement + use<> {
    div()
        .flex_shrink_0()
        .rounded(px(4.))
        .bg(theme::raised())
        .px_1p5()
        .py_0p5()
        .text_size(px(theme::font_micro()))
        .text_color(theme::text_muted())
        .child("GitHub")
}

/// A filter that matched nothing says so and offers the way back.
fn no_match(query: &str, cx: &mut Context<ItemListView>) -> gpui::AnyElement {
    div()
        .flex_1()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_1()
        .child(
            div()
                .text_size(px(theme::font_heading()))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::text())
                .child("No issues match"),
        )
        .child(
            div()
                .text_size(px(theme::font_meta()))
                .text_color(theme::text_muted())
                .child(format!("Nothing matches “{query}”.")),
        )
        .child(
            div()
                .id("issue-clear-filter")
                .debug_selector(|| "issue-clear-filter".into())
                .a11y_button("Clear filter")
                .cursor_pointer()
                .mt_1()
                .text_size(px(theme::font_control()))
                .text_color(theme::accent_ink())
                .on_click(cx.listener(|this, _, _, cx| {
                    this.clear_filter = true;
                    cx.notify();
                }))
                .child("Clear filter"),
        )
        .into_any_element()
}

fn tab_card(content: impl IntoElement) -> gpui::AnyElement {
    div()
        .size_full()
        .min_h_0()
        .flex()
        .flex_col()
        .child(content)
        .into_any_element()
}

fn empty_state(text: String) -> gpui::AnyElement {
    empty_note(text, String::new())
}

/// An empty list says what is true and, when it can, which repository it is
/// true of — never just "nothing here".
fn empty_note(title: String, detail: String) -> gpui::AnyElement {
    div()
        .flex_1()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_1()
        .child(
            div()
                .text_size(px(theme::font_body()))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::text_dim())
                .child(title),
        )
        .when(!detail.is_empty(), |column| {
            column.child(
                div()
                    .text_size(px(theme::font_meta()))
                    .text_color(theme::text_muted())
                    .child(detail),
            )
        })
        .into_any_element()
}
