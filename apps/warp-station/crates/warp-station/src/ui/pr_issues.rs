//! The detail pane's Pull requests and Issues tabs: a toolbar with Open and
//! Closed and the New verb, then one row per item — the item glyph, #number and
//! title, a head → base line (PRs) or labels and comments (issues), and an
//! author / age cluster on the right. Issues also get a filter field and a Sort
//! menu. A click on an open pull request turns the pane into that pull request;
//! every other row opens on GitHub.

use crate::store::Store;
use crate::theme;
use crate::ui::A11y;
use crate::ui::icon;
use crate::ui::repos::{segment, segment_row};
use gpui::prelude::*;
use gpui::{Context, Entity, FontWeight, SharedString, Window, div, px};
use gpui_kit::component::Sizable as _;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use warp_station_core::github::{GitHubItem, IssueSort};
use warpui_kit_core::shell;

/// The rows' measure, as the design caps a list inside the detail pane.
const LIST_WIDTH: f32 = 920.;

pub struct ItemListView {
    store: Entity<Store>,
    pull_requests: bool,
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
            filter,
            query: String::new(),
            folder: None,
            closed: false,
            sort_open: false,
            clear_filter: false,
        }
    }

    /// Escape: the Sort menu first, then the filter. Returns whether it took
    /// the key.
    pub fn dismiss(&mut self, cx: &mut Context<Self>) -> bool {
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
        if closed {
            let pull_requests = self.pull_requests;
            self.store.update(cx, |s, _| s.load_closed(pull_requests));
        }
        cx.notify();
    }

    fn render_toolbar(
        &self,
        open_count: usize,
        closed_count: Option<usize>,
        new_url: Option<String>,
        show_controls: bool,
        sort: IssueSort,
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
                        .child(warpui_kit_core::shell::field(
                            "issue-filter-field",
                            Input::new(&self.filter).with_size(theme::field_size()),
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
                bar.child(
                    div()
                        .id("list-new")
                        .a11y_button(label)
                        .cursor_pointer()
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
                        .on_click(move |_, _, _| {
                            let _ = warp_station_core::terminal::open_folder(&url);
                        })
                        .child(icon(
                            "icons/icon-plus.svg",
                            px(theme::icon_small()),
                            theme::on_accent(),
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
        let closed_items = folder.as_ref().and_then(|f| {
            self.store
                .read(cx)
                .github_closed
                .get(&(f.clone(), self.pull_requests))
                .cloned()
        });
        // a closed view stays on its list across a redraw and a refresh
        if self.closed {
            let pull_requests = self.pull_requests;
            self.store.update(cx, |s, _| s.load_closed(pull_requests));
        }
        let mut items: Vec<GitHubItem> = if self.closed {
            closed_items.as_deref().cloned().unwrap_or_default()
        } else {
            open_items.clone()
        };
        let total = items.len();
        let loading = self.closed && closed_items.is_none();
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
        let body = if loading {
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
