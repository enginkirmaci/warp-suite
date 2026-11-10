//! One issue in the Issues tab: the workbench's detail column. The head holds
//! the title, the state and the one filled primary; a status strip under it
//! carries the labels, the assignee and the counts; then the description card
//! and the comment thread. Everything comes from one `gh issue view`, asked for
//! when the row is selected.

use crate::store::Store;
use crate::theme;
use crate::ui::pull_request::{avatar, comment_card, plural, source_tag};
use crate::ui::{A11y, icon};
use gpui::prelude::*;
use gpui::{AnyElement, Context, Entity, FontWeight, Window, div, px};
use warp_station_core::github::GitHubIssue;
use warpui_kit_core::shell;

/// The reading measure of the description and the thread.
const READING: f32 = 760.;
/// The status strip's height, as the design's 44.
const STRIP_HEIGHT: f32 = 44.;

pub struct IssueDetailView {
    store: Entity<Store>,
    /// The view is the whole detail pane: the workbench's list is not beside it.
    pub stacked: bool,
}

impl IssueDetailView {
    pub fn new(store: Entity<Store>, cx: &mut Context<Self>) -> Self {
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        Self {
            store,
            stacked: false,
        }
    }

    /// Escape: the view itself has no menu, so this is the page's business.
    pub fn dismiss(&mut self, _cx: &mut Context<Self>) -> bool {
        false
    }
}

impl Render for IssueDetailView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        let store = self.store.read(cx);
        let Some(open) = store.open_issue.clone() else {
            return div().into_any_element();
        };
        let repo_name = store
            .selected_entry()
            .map(|repo| repo.name.clone())
            .unwrap_or_default();
        let issue = open.issue.as_ref();
        let item = &open.item;

        let title = issue.map_or(item.title.as_str(), |issue| issue.title.as_str());
        let url = issue
            .map(|issue| issue.url.clone())
            .filter(|url| !url.is_empty())
            .unwrap_or_else(|| item.url.clone());
        let state = issue.map_or("Open", |issue| issue.state_word());
        let closed = issue.is_some_and(|issue| !issue.is_open());

        let mut title_row = div().flex().flex_row().flex_wrap().items_center().gap_2();
        if self.stacked {
            let store = self.store.clone();
            title_row = title_row.child(
                div()
                    .id("issue-detail-back")
                    .debug_selector(|| "issue-detail-back".into())
                    .a11y_button("Back to issues")
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
                            store.close_issue(cx);
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
                    .text_size(px(theme::font_screen_title()))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme::text())
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(title.to_string()),
            )
            .child(state_pill(state, closed))
            .child(div().flex_1())
            .child(primary_button("issue-open", "Open on GitHub", {
                let url = url.clone();
                move |_, _, _| {
                    if !url.is_empty() {
                        let _ = warp_station_core::terminal::open_folder(&url);
                    }
                }
            }));

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
            .child(subtitle(issue, item, &repo_name));

        let body = div()
            .id("issue-thread")
            .debug_selector(|| "issue-thread".into())
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p(px(shell::DETAIL_PADDING))
            .child(match issue {
                Some(issue) => thread(issue, cx),
                None => div()
                    .text_size(px(theme::font_body()))
                    .text_color(theme::text_muted())
                    .child(if open.failed {
                        "GitHub did not answer."
                    } else {
                        "Loading the issue…"
                    })
                    .into_any_element(),
            });

        div()
            .debug_selector(|| "issue-detail".into())
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(theme::panel())
            .child(head)
            .child(status_strip(issue))
            .child(body)
            .into_any_element()
    }
}

/// Author, `repo #number`, and when it was opened.
fn subtitle(
    issue: Option<&GitHubIssue>,
    item: &warp_station_core::github::GitHubItem,
    repo_name: &str,
) -> AnyElement {
    let author = issue
        .and_then(|issue| issue.author.clone())
        .or_else(|| item.author.clone())
        .unwrap_or_else(|| "ghost".into());
    let opened = issue
        .and_then(|issue| issue.created_at.as_deref())
        .or(item.created_at.as_deref())
        .and_then(warp_station_core::rel_time::format_rfc3339)
        .map(|when| format!("Opened {when}"));
    div()
        .mt_2()
        .flex()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap(px(14.))
        .text_size(px(theme::font_meta()))
        .text_color(theme::text_dim())
        .child(
            div()
                .flex()
                .items_center()
                .gap_1p5()
                .child(avatar(&author, 22.))
                .child(author),
        )
        .child(format!("{repo_name} #{}", item.number))
        .children(opened)
        .into_any_element()
}

/// The `base` band under the head: the state, the labels, the assignee, the
/// counts and where the row will open.
fn status_strip(issue: Option<&GitHubIssue>) -> AnyElement {
    let strip = div()
        .debug_selector(|| "issue-status".into())
        .flex_shrink_0()
        .min_h(px(STRIP_HEIGHT))
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
        .text_color(theme::text_dim());
    let Some(issue) = issue else {
        return strip.child("").into_any_element();
    };

    let closed = !issue.is_open();
    let (path, color) = if closed {
        ("icons/icon-issue.svg", theme::text_muted())
    } else {
        ("icons/icon-issue.svg", theme::ISSUE)
    };
    let mut facts = div()
        .flex()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap(px(18.));
    facts = facts.child(
        div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap_1p5()
            .text_color(color)
            .child(icon(path, px(theme::icon_tiny()), color))
            .child(issue.state_word()),
    );
    for label in &issue.labels {
        facts = facts.child(
            div()
                .flex_shrink_0()
                .rounded(px(4.))
                .bg(theme::raised())
                .px_1p5()
                .py_0p5()
                .text_color(theme::text_dim())
                .child(label.clone()),
        );
    }
    if let Some(assignee) = issue.assignees.first() {
        facts = facts.child(
            div()
                .flex()
                .items_center()
                .gap_1p5()
                .child(avatar(assignee, 20.))
                .child(assignee.clone()),
        );
    }
    let comments = issue.comments.len();
    facts = facts
        .child(format!("{comments} comment{}", plural(comments)))
        .children(
            issue
                .updated_at
                .as_deref()
                .and_then(warp_station_core::rel_time::format_rfc3339)
                .map(|when| format!("Updated {when}")),
        );

    strip
        .child(facts)
        .child(div().flex_1())
        .child(source_tag("GitHub"))
        .into_any_element()
}

/// The description card and the comments under it.
fn thread(issue: &GitHubIssue, cx: &Context<IssueDetailView>) -> AnyElement {
    let author = issue.author.clone().unwrap_or_else(|| "ghost".into());
    let opened = issue
        .created_at
        .as_deref()
        .and_then(warp_station_core::rel_time::format_rfc3339)
        .map_or_else(
            || "opened this issue".to_string(),
            |when| format!("opened {when}"),
        );
    let mut column = div()
        .max_w(px(READING))
        .flex()
        .flex_col()
        .gap_4()
        .child(comment_card(
            0,
            &author,
            opened,
            &issue.body,
            warpui_kit_core::text_menu::focus(cx),
        ));
    if issue.comments.is_empty() {
        column = column.child(
            div()
                .debug_selector(|| "issue-no-comments".into())
                .text_size(px(theme::font_meta()))
                .text_color(theme::text_muted())
                .child("No comments yet."),
        );
    }
    for (ix, comment) in issue.comments.iter().enumerate() {
        let when = comment
            .created_at
            .as_deref()
            .and_then(warp_station_core::rel_time::format_rfc3339)
            .map_or_else(
                || "commented".to_string(),
                |when| format!("commented {when}"),
            );
        column = column.child(comment_card(
            ix + 1,
            &comment.author,
            when,
            &comment.body,
            warpui_kit_core::text_menu::focus(cx),
        ));
    }
    div()
        .debug_selector(|| "issue-description".into())
        .flex()
        .flex_col()
        .child(column)
        .into_any_element()
}

fn state_pill(state: &str, closed: bool) -> AnyElement {
    let color = if closed {
        theme::text_muted()
    } else {
        theme::ISSUE
    };
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
        .child(icon("icons/icon-issue.svg", px(theme::icon_tiny()), color))
        .child(state.to_string())
        .into_any_element()
}

/// The one filled action a detail carries.
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
        .cursor_pointer()
        .text_size(px(theme::font_control()))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme::on_accent())
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
