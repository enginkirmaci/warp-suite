//! The detail pane's Pull requests and Issues tabs. Both open on a toolbar with
//! Open and Closed and the New verb, and both are the design's workbench: a 320
//! work list beside the selected item, or — below 900 px of pane — one at a
//! time with Back. The detail column is the pull request (or the form being
//! written) on one tab and the issue on the other; Issues keeps its filter and
//! Sort over the list.

use crate::store::Store;
use crate::theme;
use crate::ui::A11y;
use crate::ui::icon;
use crate::ui::issue_detail::IssueDetailView;
use crate::ui::new_pull::NewPullView;
use crate::ui::pull_request::PullRequestView;
use crate::ui::repos::{segment, segment_row};
use gpui::prelude::*;
use gpui::{Context, Entity, FontWeight, SharedString, Window, div, px};
use gpui_kit::component::input::{InputEvent, InputState};
use warp_station_core::activity::{ActivityItem, ActivityKind, ActivitySource, IssueSort};
use warpui_kit_core::shell;

/// The workbench's list column, and the pane width under which it and the
/// pull request stop sharing the pane.
const WORK_LIST: f32 = 320.;
const STACKED_PANE: f32 = 900.;

pub struct ItemListView {
    store: Entity<Store>,
    pull_requests: bool,
    /// The pull request the workbench shows beside the list.
    pull: Entity<PullRequestView>,
    /// The Azure pull request's pane: the merged list's Azure rows open it.
    azure_pull: Entity<crate::ui::azure_pull::AzurePullView>,
    /// The form that replaces it while a pull request is being written.
    new_pull: Entity<NewPullView>,
    /// The issue the Issues tab's workbench shows beside the list.
    issue_detail: Entity<IssueDetailView>,
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
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        store: Entity<Store>,
        pull: Entity<PullRequestView>,
        azure_pull: Entity<crate::ui::azure_pull::AzurePullView>,
        new_pull: Entity<NewPullView>,
        issue_detail: Entity<IssueDetailView>,
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
            azure_pull,
            new_pull,
            issue_detail,
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
        } else if self.issue_detail.read(cx).stacked && self.store.read(cx).open_issue.is_some() {
            self.store.update(cx, |store, cx| store.close_issue(cx));
            return true;
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
        // the selected item belongs to the list it came from
        if self.pull_requests {
            self.store.update(cx, |store, cx| {
                store.close_new_pull(cx);
                store.close_pull_request(cx);
            });
        } else {
            self.store.update(cx, |store, cx| store.close_issue(cx));
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
        new_target: Option<NewTarget>,
        show_controls: bool,
        sort: IssueSort,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let this = cx.entity();
        let closed_view = this.clone();
        let label = new_verb(
            self.pull_requests,
            new_target.as_ref().map(|target| target.source),
        );
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
            .when_some(new_target, |bar, target| {
                // the workbench's detail owns the one filled primary, so the
                // list's New stays a plain button — and a pull request is
                // written in the pane, not in the browser
                let store = self.store.clone();
                let pull_requests = self.pull_requests;
                let source = target.source;
                let url = target.url.clone();
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
                            // the in-pane form is GitHub's; Azure opens its
                            // own create page
                            if pull_requests && source == ActivitySource::GitHub {
                                store.update(cx, |store, cx| store.open_new_pull(cx));
                            } else if let Some(url) = url.as_ref() {
                                let _ = warp_station_core::terminal::open_folder(url);
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
        let (folder, github, azure, sort) = {
            let store = self.store.read(cx);
            let folder = store.selected_repo.clone();
            let github = folder
                .as_ref()
                .and_then(|f| store.github_by_folder.get(f).cloned());
            let azure = folder
                .as_ref()
                .and_then(|f| store.azure_by_folder.get(f).cloned());
            (folder, github, azure, store.issue_sort())
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
        let kind = if self.pull_requests {
            ActivityKind::PullRequest
        } else {
            ActivityKind::Issue
        };

        // both platforms answer the same tab; each contributes whatever it
        // resolved, and one that resolved nothing is simply not there
        let folder_text = folder.clone().unwrap_or_default();
        let shown = |source: ActivitySource| self.store.read(cx).source_shown(&folder_text, source);
        let github_on = shown(ActivitySource::GitHub);
        let azure_on = shown(ActivitySource::Azure);
        let connected = github_on || azure_on;

        let mut open_items: Vec<ActivityItem> = Vec::new();
        if github_on && let Some(snapshot) = github.as_ref() {
            let rows = if self.pull_requests {
                &snapshot.pulls
            } else {
                &snapshot.issues
            };
            open_items.extend(rows.iter().map(ActivityItem::of_github));
        }
        if azure_on && let Some(snapshot) = azure.as_ref() {
            let rows = if self.pull_requests {
                &snapshot.pulls
            } else {
                &snapshot.work_items
            };
            open_items.extend(rows.iter().map(|row| ActivityItem::of_azure(row, kind)));
        }
        let open_total = open_items.len();

        // the closed list is per source: one may answer while the other is
        // still out, and one may fail without hiding the other's rows
        let (mut items, closed_loading, failed_sources) = if self.closed {
            let store = self.store.read(cx);
            let mut rows: Vec<ActivityItem> = Vec::new();
            let mut loading = false;
            let mut failed: Vec<ActivitySource> = Vec::new();
            for source in [ActivitySource::GitHub, ActivitySource::Azure] {
                if !shown(source) {
                    continue;
                }
                let key = (folder_text.clone(), self.pull_requests, source);
                match store.closed.get(&key) {
                    Some(answered) => rows.extend(answered.iter().cloned()),
                    None if store.closed_failed.contains(&key) => failed.push(source),
                    None => loading = true,
                }
            }
            (rows, loading, failed)
        } else {
            (open_items.clone(), false, Vec::new())
        };
        let closed_failed = !failed_sources.is_empty();
        // The store dedupes (in-flight, cached, or failed-waiting-for-Retry),
        // so asking here is safe: Refresh clears the guard and the next draw
        // re-asks instead of leaving a stale or endless "Loading…".
        if self.closed && connected {
            let pull_requests = self.pull_requests;
            self.store.update(cx, |s, _| s.load_closed(pull_requests));
        }
        // the count is the list's own, before the filter: an empty tab and a
        // filter that matched nothing are different states
        let total = items.len();
        // the design gives Pull requests no filter, but Issues keeps its
        // Filter issues field and Sort whether or not the list has rows
        let controls = !self.pull_requests;
        if !self.pull_requests {
            sort.sort(&mut items);
            items.retain(|item| item.matches(&self.query));
        }

        // the New verb follows the platform the rows do: a GitHub pull request
        // is written in the in-pane form, so it needs no snapshot URL; GitHub
        // issues and Azure verbs open their platform's create page.
        let new_target = if self.pull_requests && github_on {
            Some(NewTarget {
                source: ActivitySource::GitHub,
                url: None,
            })
        } else if github_on {
            github.as_ref().and_then(|snapshot| {
                snapshot.url.as_ref().map(|url| NewTarget {
                    source: ActivitySource::GitHub,
                    url: Some(format!("{url}/issues/new")),
                })
            })
        } else {
            azure.as_ref().and_then(|snapshot| {
                let url = if self.pull_requests {
                    snapshot.new_pull_request_url()
                } else {
                    snapshot.new_work_item_url()
                };
                url.map(|url| NewTarget {
                    source: ActivitySource::Azure,
                    url: Some(url),
                })
            })
        };
        let toolbar = self.render_toolbar(
            open_total,
            None,
            new_target.clone(),
            controls,
            sort,
            window,
            cx,
        );
        let menu = (self.sort_open && controls).then(|| self.render_sort_menu(sort, cx));

        let state = if self.closed { "closed" } else { "open" };
        let repo_name = self
            .store
            .read(cx)
            .selected_entry()
            .map(|repo| repo.name.clone())
            .unwrap_or_default();
        // E2E hook: WARP_STATION_NEW_PULL=1 opens the form once, before the body
        // picks the empty state — a repository with no rows must still show it
        if self
            .store
            .update(cx, |store, _| std::mem::take(&mut store.new_pull_requested))
        {
            self.store.update(cx, |store, cx| store.open_new_pull(cx));
        }
        let writing = self.pull_requests && self.store.read(cx).new_pull.is_some();
        let body = if writing {
            // the form is the detail column even when no row sits beside it:
            // a repository's first pull request starts here
            self.render_workbench(&items, cx)
        } else if self.closed && closed_failed && total == 0 {
            let message = format!("Could not load closed {title}.");
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
                        .child(message),
                )
                .child(retry_button(
                    "closed-retry",
                    self.pull_requests,
                    self.store.clone(),
                ))
                .into_any_element()
        } else if closed_loading {
            empty_state(format!("Loading closed {title}…"))
        } else if !connected {
            empty_note(
                format!("No {state} {title}"),
                missing_target_note(self.pull_requests, github_on, azure_on),
            )
        } else if total == 0 {
            let detail = if self.closed {
                if repo_name.is_empty() {
                    "Nothing has been closed here yet.".to_string()
                } else {
                    format!("{repo_name} has nothing closed here.")
                }
            } else {
                match new_target.as_ref() {
                    // the toolbar already carries the verb; the empty state
                    // names it so the way forward is not a hunt
                    Some(target) => format!(
                        "Use {} to {} the first one.",
                        new_verb(self.pull_requests, Some(target.source)),
                        if self.pull_requests { "open" } else { "file" },
                    ),
                    None => missing_target_note(self.pull_requests, github_on, azure_on),
                }
            };
            empty_note(format!("No {state} {title}"), detail)
        } else if items.is_empty() {
            let query = self.filter.read(cx).value().trim().to_string();
            no_match(&query, cx)
        } else {
            let workbench = self.render_workbench(&items, cx);
            if closed_failed {
                // one source failed while the other answered: its rows stay,
                // with the way to ask again above them
                let message = format!(
                    "Could not load closed {title} from {}.",
                    failed_sources
                        .iter()
                        .map(|source| source.platform())
                        .collect::<Vec<_>>()
                        .join(" or ")
                );
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .child(note_line(
                        message,
                        retry_button("closed-retry-note", self.pull_requests, self.store.clone()),
                    ))
                    .child(workbench)
                    .into_any_element()
            } else {
                workbench
            }
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
    /// item's detail. Below 900 px of pane the two take turns and Back returns
    /// to the list. The detail column is the pull request (or the form being
    /// written) on the Pull requests tab and the issue on the Issues tab.
    fn render_workbench(
        &mut self,
        items: &[ActivityItem],
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let pane_width = self.store.read(cx).pane_width;
        let stacked = pane_width < STACKED_PANE;
        // the detail column's own width: the rail-stacking rules read it
        let column_width = if stacked {
            pane_width
        } else {
            pane_width - WORK_LIST
        };
        let writing = self.store.read(cx).new_pull.is_some();
        let selected = self
            .store
            .read(cx)
            .open_activity(self.pull_requests)
            .map(|open| open.number);
        let azure_open = self.pull_requests && self.store.read(cx).open_azure_pull.is_some();
        // the first visible row is the selection where both columns show, but
        // never over the form that is already being written
        if !stacked && !writing && selected.is_none() {
            let first = items[0].clone();
            self.store
                .update(cx, |store, cx| store.open_activity_row(first, cx));
        }
        self.pull.update(cx, |view, _| {
            view.stacked = stacked;
            view.column_width = column_width;
        });
        self.new_pull.update(cx, |view, _| view.stacked = stacked);
        self.azure_pull.update(cx, |view, _| {
            view.stacked = stacked;
            view.column_width = column_width;
        });
        self.issue_detail
            .update(cx, |view, _| view.stacked = stacked);

        let mut rows = div().flex().flex_col();
        for item in items {
            rows = rows.child(
                self.render_activity_row(item, selected == Some(item.number))
                    .into_any_element(),
            );
        }
        let label = if self.closed { "Closed" } else { "Open" };
        let list_selector = if self.pull_requests {
            "pr-list"
        } else {
            "issue-list"
        };
        let column_selector = if self.pull_requests {
            "pr-list-column"
        } else {
            "issue-list-column"
        };
        let list = div()
            .id(column_selector)
            .debug_selector(move || column_selector.into())
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
                    // a list one platform answered says so once, in its head;
                    // a merged one leaves each row's tag to speak for itself
                    .children(single_source_of(items).map(source_tag)),
            )
            .child(
                div()
                    .id(list_selector)
                    .debug_selector(move || list_selector.into())
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(rows),
            );

        let detail =
            div()
                .flex_1()
                .min_w_0()
                .h_full()
                .flex()
                .flex_col()
                .child(if !self.pull_requests {
                    self.issue_detail.clone().into_any_element()
                } else if writing {
                    self.new_pull.clone().into_any_element()
                } else if azure_open {
                    self.azure_pull.clone().into_any_element()
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

    /// A row of either list, whichever platform it came from: 58 high, the
    /// platform's reference and the title, then the second line that fits the
    /// kind, the source tag and the age. The selected row takes `raised` and
    /// the 3 px accent edge.
    ///
    /// The press routes by platform and kind: a GitHub pull request or issue
    /// opens its own pane, an Azure pull request opens the Azure pane, and a
    /// work item — which has no pane of its own — opens on Azure.
    fn render_activity_row(&self, item: &ActivityItem, selected: bool) -> impl IntoElement + use<> {
        let closed = self.closed;
        let target = item.clone();
        let store = self.store.clone();
        let age = item
            .updated_at
            .as_deref()
            .and_then(warp_station_core::rel_time::format_rfc3339);
        let selector = format!(
            "{}-{}-{}",
            match item.kind {
                ActivityKind::PullRequest => "PR",
                ActivityKind::Issue => "Issue",
            },
            match item.source {
                ActivitySource::GitHub => "gh",
                ActivitySource::Azure => "az",
            },
            item.number
        );
        let (glyph, ink) = match (item.kind, closed) {
            (ActivityKind::PullRequest, false) => ("icons/icon-git-pull-request.svg", theme::PR),
            (ActivityKind::PullRequest, true) => {
                ("icons/icon-git-pull-request.svg", theme::text_muted())
            }
            (ActivityKind::Issue, false) => ("icons/icon-issue.svg", theme::ISSUE),
            (ActivityKind::Issue, true) => ("icons/icon-issue.svg", theme::text_muted()),
        };

        // the second line says what the kind is about: a branch pair for a
        // pull request, the labels (or type, state and assignee) for an issue
        let mut meta = div()
            .flex()
            .items_center()
            .gap_1p5()
            .min_w_0()
            .overflow_hidden()
            .whitespace_nowrap()
            .text_size(px(theme::font_meta()))
            .text_color(theme::text_muted());
        match item.kind {
            ActivityKind::PullRequest => {
                let head = item.head_ref.clone().unwrap_or_default();
                let base = item.base_ref.clone().unwrap_or_default();
                if !head.is_empty() {
                    meta = meta
                        .child(
                            div()
                                .min_w_0()
                                .overflow_hidden()
                                .text_ellipsis()
                                .font_family(SharedString::from(theme::code_font()))
                                .text_size(px(theme::font_mono_meta()))
                                .child(format!("{head} → {base}")),
                        )
                        .child("·");
                } else if let Some(branch) = item.base_ref.clone().filter(|b| !b.is_empty()) {
                    meta = meta
                        .child(
                            div()
                                .font_family(SharedString::from(theme::code_font()))
                                .text_size(px(theme::font_mono_meta()))
                                .child(branch),
                        )
                        .child("·");
                }
                meta = meta
                    .child(item.state_word())
                    .child("·")
                    .child(source_tag(item.source));
            }
            ActivityKind::Issue => {
                let mut bits: Vec<String> = item.labels.clone();
                if bits.is_empty() {
                    if let Some(work_item_type) = item.work_item_type.clone() {
                        bits.push(work_item_type);
                    }
                    bits.push(item.state_word());
                    if let Some(assignee) = item.assigned_to.clone() {
                        bits.push(assignee);
                    }
                }
                if !bits.is_empty() {
                    meta = meta
                        .child(
                            div()
                                .min_w_0()
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(bits.join(" · ")),
                        )
                        .child("·");
                }
                meta = meta
                    .child(format!(
                        "{} comment{}",
                        item.comments,
                        if item.comments == 1 { "" } else { "s" }
                    ))
                    .child("·")
                    .child(source_tag(item.source));
            }
        }

        div().border_t_1().border_color(theme::row_line()).child(
            div()
                .id(SharedString::from(selector.clone()))
                .debug_selector(move || selector.clone())
                .a11y_button(format!("{} {}", item.reference(), item.title))
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
                    store.update(cx, |store, cx| match (item.source, item.kind) {
                        (ActivitySource::GitHub, ActivityKind::PullRequest) => {
                            store.open_pull_request(item.to_github_item(), cx)
                        }
                        (ActivitySource::GitHub, ActivityKind::Issue) => {
                            store.open_issue(item.to_github_item(), cx)
                        }
                        (ActivitySource::Azure, ActivityKind::PullRequest) => {
                            store.open_azure_pull(item.to_azure_item(), cx)
                        }
                        // a work item has no pane of its own yet
                        (ActivitySource::Azure, ActivityKind::Issue) => {
                            if !item.url.is_empty() {
                                let _ = warp_station_core::terminal::open_folder(&item.url);
                            }
                        }
                    });
                })
                .child(icon(glyph, px(theme::icon_medium()), ink))
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
                                .child(format!("{} {}", item.reference(), item.title)),
                        )
                        .child(meta),
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
}

/// The closed list's way to ask again.
fn retry_button(
    id: &'static str,
    pull_requests: bool,
    store: Entity<Store>,
) -> impl IntoElement + use<> {
    div()
        .id(id)
        .debug_selector({
            let id = id.to_string();
            move || id.clone()
        })
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
            store.update(cx, |s, cx| {
                s.retry_closed(pull_requests);
                cx.notify();
            });
        })
        .child("Retry")
}

/// A quiet line above rows that answered, saying one platform did not.
fn note_line<A: IntoElement>(message: String, action: A) -> impl IntoElement + use<A> {
    div()
        .flex_shrink_0()
        .flex()
        .items_center()
        .gap_2()
        .px_4()
        .h(px(36.))
        .border_b_1()
        .border_color(theme::hairline())
        .bg(theme::panel())
        .text_size(px(theme::font_meta()))
        .text_color(theme::text_muted())
        .child(message)
        .child(div().flex_1())
        .child(action)
}

/// The platform a work list's rows all came from, when they share one.
fn single_source_of(items: &[ActivityItem]) -> Option<ActivitySource> {
    let first = items.first()?.source;
    items
        .iter()
        .all(|item| item.source == first)
        .then_some(first)
}

/// The quiet source tag the merged lists carry: it says where the row will
/// open, on the raised surface and in the muted ink, never the entity hue.
fn source_tag(source: ActivitySource) -> impl IntoElement + use<> {
    div()
        .flex_shrink_0()
        .rounded(px(4.))
        .bg(theme::raised())
        .px_1p5()
        .py_0p5()
        .text_size(px(theme::font_micro()))
        .text_color(theme::text_muted())
        .child(source.label())
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

/// The New verb's label for this list and platform.
fn new_verb(pull_requests: bool, source: Option<ActivitySource>) -> &'static str {
    match (pull_requests, source) {
        (true, _) => "New pull request",
        (false, Some(ActivitySource::Azure)) => "New work item",
        (false, _) => "New issue",
    }
}

/// The New verb's destination: a GitHub pull request is written in the pane,
/// so it has no URL; the other verbs open the platform's create page.
#[derive(Clone, Debug)]
struct NewTarget {
    source: ActivitySource,
    url: Option<String>,
}

/// What an empty list asks for when it has no known create target: name the
/// integration whose remote or credentials are missing.
fn missing_target_note(pull_requests: bool, github_on: bool, azure_on: bool) -> String {
    let azure = azure_on && !github_on;
    let integration = if azure { "Azure DevOps" } else { "GitHub" };
    let action = match (pull_requests, azure) {
        (true, _) => "open a pull request",
        (false, true) => "file a work item",
        (false, false) => "file an issue",
    };
    format!("Connect {integration} to {action}.")
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
