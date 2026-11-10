//! A pull request in the Pull requests tab: the workbench's detail column.
//! The head carries the state, the one review verb and the one filled primary;
//! a status strip under it holds the facts the head has no room for; then the
//! pull request's own three tabs — Overview (policies, description, comments,
//! reviewers), Files (the changed-file rail and the diff) and Commits (the day
//! grouped commits and one commit's diff).
//!
//! Everything it draws comes from `gh pr view` plus two lazy calls; every verb
//! is one `gh pr` subcommand, so the view never guesses what the platform will
//! allow — the policy card says what it knows and the merge says the rest.

use crate::store::{OpenPull, Store};
use crate::theme;
use crate::ui::diff_controls;
use crate::ui::{A11y, icon};
use gpui::prelude::*;
use gpui::{AnyElement, App, Entity, FontWeight, Rgba, SharedString, Window, div, px};
use gpui_kit::base::text::TextView;
use gpui_kit::component::input::TextareaState;
use warp_station_core::github::{
    CheckState, GitHubCheck, GitHubCommit, GitHubFilePatch, GitHubPull, PullAction,
};
use warp_station_core::settings::DiffViewMode;
use warpui_kit_core::{field, motion, shell};
use warpui_kit_diff::{DiffScrollHandles, render_diff_with_scroll};

/// The status strip's height, as the design's 44.
const STRIP_HEIGHT: f32 = 44.;
/// The Files tab's rail and the Commits tab's list, from the prototype.
const FILE_RAIL: f32 = 240.;
/// The band the rail becomes when its column is narrow, and the column width
/// under which the design stacks it above the diff.
const RAIL_BAND: f32 = 180.;
const STACKED_COLUMN: f32 = 900.;
const COMMIT_LIST: f32 = 220.;
const COMMIT_RAIL: f32 = 200.;
/// The reading column the description and the comments keep inside the tab.
const READING: f32 = 760.;

/// The pull request's own tabs, under its status strip.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PullTab {
    #[default]
    Overview,
    Files,
    Commits,
}

impl PullTab {
    pub const ALL: [PullTab; 3] = [PullTab::Overview, PullTab::Files, PullTab::Commits];

    pub fn label(self) -> &'static str {
        match self {
            PullTab::Overview => "Overview",
            PullTab::Files => "Files",
            PullTab::Commits => "Commits",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            PullTab::Overview => "overview",
            PullTab::Files => "files",
            PullTab::Commits => "commits",
        }
    }
}

pub struct PullRequestView {
    store: Entity<Store>,
    tab: PullTab,
    /// The file the Files tab shows.
    file_ix: usize,
    /// The commit the Commits tab shows, and the file inside it.
    commit_ix: usize,
    commit_file_ix: usize,
    /// The policy card's expanded check list.
    checks_open: bool,
    diff_view: DiffViewMode,
    diff_wrap: bool,
    diff_scroll: DiffScrollHandles,
    commit_scroll: DiffScrollHandles,
    /// The head's two menus.
    primary_menu: bool,
    review_menu: bool,
    /// The "Add a comment" composer.
    comment: Entity<TextareaState>,
    /// The pull request the view's state belongs to: a new selection starts
    /// back on Overview with its own composer.
    showing: Option<u64>,
    /// The view is the whole detail pane: the workbench's list is not beside it.
    pub stacked: bool,
    /// The detail column's own width, set by the workbench. The Files and
    /// Commits rails stack above the diff under 900 px of it.
    pub column_width: f32,
}

impl PullRequestView {
    pub fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        let comment = cx.new(|cx| TextareaState::new(window, cx).placeholder("Write a comment…"));
        // the composer's button follows what is typed in it, and the field is
        // not the view's own state
        cx.observe(&comment, |_, _, cx| cx.notify()).detach();
        let diff_view = store.read(cx).diff_view();
        let diff_wrap = store.read(cx).diff_wrap();
        Self {
            store,
            tab: PullTab::Overview,
            file_ix: 0,
            commit_ix: 0,
            commit_file_ix: 0,
            checks_open: false,
            diff_view,
            diff_wrap,
            diff_scroll: DiffScrollHandles::default(),
            commit_scroll: DiffScrollHandles::default(),
            primary_menu: false,
            review_menu: false,
            comment,
            showing: None,
            stacked: false,
            column_width: f32::MAX,
        }
    }

    /// Escape: the head's menus first. Back (below 900 px) is the page's.
    pub fn dismiss(&mut self, cx: &mut Context<Self>) -> bool {
        if self.primary_menu || self.review_menu {
            self.primary_menu = false;
            self.review_menu = false;
            cx.notify();
            return true;
        }
        false
    }

    fn select_tab(&mut self, tab: PullTab, cx: &mut Context<Self>) {
        self.tab = tab;
        self.primary_menu = false;
        self.review_menu = false;
        cx.notify();
    }

    /// A new pull request starts on Overview, with its own composer and no
    /// menu left over from the one before it.
    fn reset_for(&mut self, number: u64) {
        if self.showing == Some(number) {
            return;
        }
        self.showing = Some(number);
        self.tab = PullTab::Overview;
        self.file_ix = 0;
        self.commit_ix = 0;
        self.commit_file_ix = 0;
        self.checks_open = false;
        self.primary_menu = false;
        self.review_menu = false;
    }
}

impl Render for PullRequestView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        let this = cx.entity();
        let Some(open) = self.store.read(cx).open_pull.clone() else {
            return div().into_any_element();
        };
        let number = open.item.number;
        self.reset_for(number);
        // a comment that landed empties the composer it was written in
        if self
            .store
            .update(cx, |store, _| std::mem::take(&mut store.pull_comment_sent))
        {
            self.comment
                .update(cx, |input, cx| input.set_value("", window, cx));
        }

        // the two lazy calls, asked for as the tab that needs them draws
        let commit_sha = if self.tab == PullTab::Commits {
            selected_commit(&open, self.commit_ix).map(|commit| commit.oid.clone())
        } else {
            None
        };
        if self.tab == PullTab::Files {
            self.store.update(cx, |store, _| store.load_pull_diff());
        }
        if let Some(sha) = commit_sha.as_deref() {
            self.store
                .update(cx, |store, _| store.load_pull_commit_files(sha));
        }

        let store = self.store.read(cx);
        let diff = store.pull_diff.get(&number).cloned().flatten();
        let commit_files = commit_sha
            .as_ref()
            .and_then(|sha| store.pull_commit_files.get(sha).cloned())
            .flatten();
        let acting = store.pull_acting;
        let show_tooltips = store.settings().general.show_tooltips;
        let repo_name = store
            .selected_entry()
            .map(|repo| repo.name.clone())
            .unwrap_or_default();

        let pull = open.pull.as_ref();
        let merging = open.merging;
        let merge_armed = open.merge_armed;
        let body = match self.tab {
            PullTab::Overview => self.overview(pull, &open, window, cx, &this),
            PullTab::Files => {
                self.files_tab(pull, diff.as_deref(), window, cx, &this, show_tooltips)
            }
            PullTab::Commits => self.commits_tab(
                pull,
                commit_files.as_deref(),
                &open,
                window,
                cx,
                &this,
                show_tooltips,
            ),
        };

        let pane = div()
            .debug_selector(|| "pull-request".into())
            .relative()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(theme::panel())
            .child(self.head(&open, pull, repo_name, acting, merging, merge_armed, &this))
            .child(status_strip(pull, &open))
            .child(self.tab_strip(pull, &this))
            .child(body);
        pane.children(
            self.primary_menu
                .then(|| self.primary_menu_card(pull, &this)),
        )
        .children(self.review_menu.then(|| self.review_menu_card(pull, &this)))
        .into_any_element()
    }
}

impl PullRequestView {
    // ---- head -------------------------------------------------------------

    #[allow(clippy::too_many_arguments)]
    fn head(
        &self,
        open: &OpenPull,
        pull: Option<&GitHubPull>,
        repo_name: String,
        acting: bool,
        merging: bool,
        merge_armed: bool,
        this: &Entity<Self>,
    ) -> AnyElement {
        let item = &open.item;
        let title = pull.map_or(item.title.as_str(), |p| p.title.as_str());
        let url = pull
            .map(|p| p.url.clone())
            .filter(|url| !url.is_empty())
            .unwrap_or_else(|| item.url.clone());
        let closed = pull.is_some_and(|p| !p.is_open());

        let mut title_row = div().flex().flex_row().flex_wrap().items_center().gap_2();
        if self.stacked {
            let store = self.store.clone();
            title_row = title_row.child(
                div()
                    .id("pull-back")
                    .debug_selector(|| "pull-back".into())
                    .a11y_button("Back to pull requests")
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
                            store.close_pull_request(cx);
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
                    // a floor, so the actions wrap under a long title instead
                    // of squeezing it to a stub
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
            .child(state_pill(pull))
            .child(head_actions(
                &self.store,
                pull,
                acting,
                merging,
                merge_armed,
                closed,
                url,
                this,
            ));

        div()
            .flex_shrink_0()
            .px(px(shell::DETAIL_PADDING))
            .pt_4()
            .pb(px(14.))
            .flex()
            .flex_col()
            .child(title_row)
            .child(subtitle(open, pull, &repo_name))
            .into_any_element()
    }

    // ---- tabs -------------------------------------------------------------

    fn tab_strip(&self, pull: Option<&GitHubPull>, this: &Entity<Self>) -> AnyElement {
        let files = pull.map_or(0, |p| p.files.len());
        let commits = pull.map_or(0, |p| p.commits.len());
        let mut strip = div()
            .debug_selector(|| "pull-tabs".into())
            .flex_shrink_0()
            .px(px(shell::DETAIL_PADDING))
            .flex()
            .flex_row()
            .border_b_1()
            .border_color(theme::hairline());
        for tab in PullTab::ALL {
            let count = match tab {
                PullTab::Files => files,
                PullTab::Commits => commits,
                PullTab::Overview => 0,
            };
            let selector = format!("pull-tab-{}", tab.key());
            let this = this.clone();
            strip = strip.child(
                shell::detail_tab(selector.clone(), tab.label(), self.tab == tab)
                    .debug_selector(move || selector.clone())
                    .on_click(move |_, _, cx| {
                        this.update(cx, |view, cx| view.select_tab(tab, cx));
                    })
                    .children((count > 0).then(|| {
                        div()
                            .text_size(px(theme::font_meta()))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme::text_muted())
                            .child(format!("{count}"))
                    })),
            );
        }
        strip.into_any_element()
    }

    // ---- Overview ---------------------------------------------------------

    fn overview(
        &self,
        pull: Option<&GitHubPull>,
        open: &OpenPull,
        window: &mut Window,
        cx: &mut Context<Self>,
        this: &Entity<Self>,
    ) -> AnyElement {
        let body = div()
            .id("pull-overview")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p(px(shell::DETAIL_PADDING));
        let Some(pull) = pull else {
            return body
                .child(diff_controls::state(if open.failed {
                    "GitHub did not answer."
                } else {
                    "Loading the pull request…"
                }))
                .into_any_element();
        };
        let main = div()
            .flex_grow(2.)
            .flex_shrink(1.)
            .flex_basis(px(0.))
            .min_w(px(320.))
            .max_w(px(READING))
            .flex()
            .flex_col()
            .gap_4()
            .child(self.policy_card(pull, this))
            .child(description_card(pull))
            .child(self.comments_section(pull, window, cx));
        let side = div()
            .flex_grow(1.)
            .flex_shrink(1.)
            .flex_basis(px(0.))
            .min_w(px(240.))
            .flex()
            .flex_col()
            .gap_4()
            .child(reviewer_card(pull));
        body.child(
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .items_start()
                .gap_4()
                .child(main)
                .child(side),
        )
        .into_any_element()
    }

    fn policy_card(&self, pull: &GitHubPull, this: &Entity<Self>) -> AnyElement {
        let failed = pull.count_checks(CheckState::Failed);
        let running = pull.count_checks(CheckState::Running);
        let passed = pull.count_checks(CheckState::Passed);
        let skipped = pull.count_checks(CheckState::Skipped);
        let review_required = pull.review_decision.as_deref() == Some("REVIEW_REQUIRED");
        let blocked = failed > 0
            || pull.changes_requested() > 0
            || pull.has_conflicts()
            || review_required
            || (pull.checks.is_empty() && !pull.ready_to_merge() && !pull.is_draft);

        let mut rows = div().flex().flex_col();

        // the checks
        if pull.checks.is_empty() {
            rows = rows.child(policy_row(
                "icons/icon-clock.svg",
                theme::text_muted(),
                "No checks reported yet".to_string(),
                "Checks will appear here when the pipeline runs.".into(),
            ));
        } else if failed > 0 {
            rows = rows.child(policy_row(
                "icons/icon-close.svg",
                theme::DANGER,
                format!("{failed} required check{} failed", plural(failed)),
                "A required check must pass before completion.".into(),
            ));
        } else if running > 0 {
            rows = rows.child(policy_row(
                "icons/icon-clock.svg",
                theme::BEHIND,
                format!("{running} required check{} running", plural(running)),
                "A required check is still running.".into(),
            ));
        } else {
            let detail = match (passed, skipped) {
                (0, 0) => String::new(),
                (n, 0) => format!("{n} check{} passed", plural(n)),
                (n, s) => format!("{n} passed · {s} skipped"),
            };
            rows = rows.child(policy_row(
                "icons/icon-check.svg",
                theme::SUCCESS_BRIGHT,
                "All required checks passed".to_string(),
                detail,
            ));
        }
        if !pull.checks.is_empty() {
            let checks_open = self.checks_open;
            let label = if checks_open {
                "Hide checks".to_string()
            } else {
                format!(
                    "View {} check{}",
                    pull.checks.len(),
                    plural(pull.checks.len())
                )
            };
            let this = this.clone();
            rows = rows.child(
                div()
                    .id("pull-checks-toggle")
                    .debug_selector(|| "pull-checks-toggle".into())
                    .a11y_button(label.clone())
                    .cursor_pointer()
                    .px(px(16.))
                    .pb(px(10.))
                    .text_size(px(theme::font_meta()))
                    .text_color(theme::accent_ink())
                    .hover(|s| s.text_color(theme::text()))
                    .on_click(move |_, _, cx| {
                        this.update(cx, |this, cx| {
                            this.checks_open = !this.checks_open;
                            cx.notify();
                        });
                    })
                    .child(label),
            );
            if checks_open {
                let mut list = div()
                    .debug_selector(|| "pull-checks".into())
                    .flex()
                    .flex_col()
                    .border_t_1()
                    .border_color(theme::row_line());
                for (ix, check) in pull.checks.iter().enumerate() {
                    list = list.child(check_row(ix, check));
                }
                rows = rows.child(list);
            }
        }

        // the review
        let changes = pull.changes_requested();
        let awaiting = pull.requested_reviewers.len();
        if changes > 0 {
            rows = rows.child(policy_row(
                "icons/icon-close.svg",
                theme::DANGER,
                format!("{changes} reviewer{} asked for changes", plural(changes)),
                "A change request blocks completion until it is dismissed.".into(),
            ));
        } else if review_required {
            rows = rows.child(policy_row(
                "icons/icon-clock.svg",
                theme::BEHIND,
                "Review required before merging".to_string(),
                "The repository wants an approving review first.".into(),
            ));
        } else if awaiting > 0 {
            rows = rows.child(policy_row(
                "icons/icon-clock.svg",
                theme::BEHIND,
                format!("{awaiting} reviewer{} must approve", plural(awaiting)),
                "The required reviewer policy blocks completion.".into(),
            ));
        } else if pull.approvals() > 0 {
            rows = rows.child(policy_row(
                "icons/icon-check.svg",
                theme::SUCCESS_BRIGHT,
                "All required reviewers approved".to_string(),
                String::new(),
            ));
        } else {
            rows = rows.child(policy_row(
                "icons/icon-clock.svg",
                theme::text_muted(),
                "No required reviewers".to_string(),
                "Optional reviewers can review this pull request.".into(),
            ));
        }

        // the merge state
        let checked = pull
            .updated_at
            .as_deref()
            .and_then(warp_station_core::rel_time::format_rfc3339)
            .map(|when| format!("Last checked {when}"))
            .unwrap_or_default();
        rows = rows.child(if pull.has_conflicts() {
            policy_row(
                "icons/icon-close.svg",
                theme::DANGER,
                "Merge conflicts must be resolved".to_string(),
                checked,
            )
        } else {
            policy_row(
                "icons/icon-check.svg",
                theme::SUCCESS_BRIGHT,
                "No merge conflicts".to_string(),
                checked,
            )
        });

        // auto-complete, when the platform says it is armed
        if pull.auto_merge {
            rows = rows.child(policy_row(
                "icons/icon-clock.svg",
                theme::BEHIND,
                "Auto-complete set".to_string(),
                "The pull request completes when the blocking policies clear.".into(),
            ));
        }

        let word = if pull.is_draft {
            "Draft"
        } else if blocked {
            "Blocked"
        } else {
            "Ready"
        };
        card("pull-policies", "Policies", word, rows.into_any_element())
    }

    fn comments_section(
        &self,
        pull: &GitHubPull,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let count = pull.comments.len() + usize::from(!pull.body.trim().is_empty());
        let mut thread = div().flex().flex_col().gap_4();
        if !pull.body.trim().is_empty() {
            let author = pull.author.clone().unwrap_or_else(|| "ghost".into());
            let when = pull
                .created_at
                .as_deref()
                .and_then(warp_station_core::rel_time::format_rfc3339)
                .map_or_else(
                    || "opened this pull request".to_string(),
                    |when| format!("opened {when}"),
                );
            thread = thread.child(comment_card(
                0,
                &author,
                when,
                &pull.body,
                warpui_kit_core::text_menu::focus(cx),
            ));
        }
        for (ix, note) in pull.comments.iter().enumerate() {
            let when = note
                .created_at
                .as_deref()
                .and_then(warp_station_core::rel_time::format_rfc3339)
                .map_or_else(
                    || "commented".to_string(),
                    |when| format!("commented {when}"),
                );
            thread = thread.child(comment_card(
                ix + 1,
                &note.author,
                when,
                &note.body,
                warpui_kit_core::text_menu::focus(cx),
            ));
        }
        if count == 0 {
            thread = thread.child(
                div()
                    .text_size(px(theme::font_meta()))
                    .text_color(theme::text_muted())
                    .child("No comments yet."),
            );
        }

        let store = self.store.clone();
        let body = self.comment.read(cx).value().trim().to_string();
        let sendable = !body.is_empty();
        let composer = div()
            .flex()
            .flex_col()
            .rounded(px(8.))
            .border_1()
            .border_color(theme::hairline())
            .bg(theme::card())
            .overflow_hidden()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_4()
                    .h(px(40.))
                    .border_b_1()
                    .border_color(theme::row_line())
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(theme::font_heading()))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme::text())
                            .child("Add a comment"),
                    )
                    .child(
                        div()
                            .text_size(px(theme::font_meta()))
                            .text_color(theme::text_muted())
                            .child("Plain text"),
                    ),
            )
            .child(
                div()
                    .p_4()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(field::area("pull-comment-field", &self.comment, window, cx))
                    .child(div().flex().justify_end().child(primary_button(
                        "pull-comment",
                        "Comment",
                        sendable,
                        move |_, _, cx| {
                            let body = body.clone();
                            store.update(cx, |store, cx| {
                                store.act_on_pull(PullAction::Comment(body), cx);
                            });
                        },
                    ))),
            );

        div()
            .id("pull-comments")
            .debug_selector(|| "pull-comments".into())
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(theme::font_heading()))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme::text())
                            .child("Comments"),
                    )
                    .child(
                        div()
                            .text_size(px(theme::font_meta()))
                            .text_color(theme::text_muted())
                            .child(format!("{count} comment{}", plural(count))),
                    ),
            )
            .child(thread)
            .child(composer)
            .into_any_element()
    }

    // ---- Files ------------------------------------------------------------

    fn files_tab(
        &self,
        pull: Option<&GitHubPull>,
        diff: Option<&[GitHubFilePatch]>,
        window: &mut Window,
        cx: &mut Context<Self>,
        this: &Entity<Self>,
        show_tooltips: bool,
    ) -> AnyElement {
        let files: Vec<&GitHubFilePatch> = match (diff, pull) {
            (Some(diff), _) => diff.iter().collect(),
            (None, Some(pull)) => pull.files.iter().collect(),
            (None, None) => Vec::new(),
        };
        if files.is_empty() {
            return div()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .child(diff_controls::state(if diff.is_none() {
                    "Loading the patch…"
                } else {
                    "No files changed"
                }))
                .into_any_element();
        }
        let ix = self.file_ix.min(files.len().saturating_sub(1));
        let file = files[ix];

        let mut rail = div().flex().flex_col().child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .h(px(34.))
                .px(px(16.))
                .text_size(px(theme::font_meta()))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::text_dim())
                .child("Files")
                .child(
                    div()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme::text_muted())
                        .child(format!("{}", files.len())),
                ),
        );
        let mut rows = div()
            .id("pull-files")
            .debug_selector(|| "pull-files".into())
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .py_1();
        for (index, entry) in files.iter().enumerate() {
            let selected = index == ix;
            let this = this.clone();
            let selector = format!("pull-file-{index}");
            let (dir, name) = split_path(&entry.path);
            rows = rows.child(
                div()
                    .id(SharedString::from(selector.clone()))
                    .debug_selector(move || selector.clone())
                    .a11y_button(entry.path.clone())
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .gap_2()
                    .h(px(34.))
                    .pl(px(13.))
                    .pr_2()
                    .border_l(px(3.))
                    .border_color(if selected {
                        gpui::Hsla::from(theme::accent())
                    } else {
                        gpui::transparent_black()
                    })
                    .when(selected, |d| d.bg(theme::raised()))
                    .hover(|d| d.bg(theme::panel()))
                    .on_click(move |_, _, cx| {
                        this.update(cx, |this, cx| {
                            this.file_ix = index;
                            cx.notify();
                        });
                    })
                    .child(
                        div()
                            .flex_shrink_0()
                            .w(px(14.))
                            .font_family(theme::code_font())
                            .text_size(px(theme::font_mono_meta()))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(diff_controls::status_color(&entry.status))
                            .child(entry.status.clone()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_size(px(theme::font_meta()))
                            .text_color(theme::text())
                            .child(
                                div()
                                    .flex_shrink(1.0)
                                    .min_w(px(0.))
                                    .text_color(theme::text_muted())
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .child(dir),
                            )
                            .child(div().flex_shrink_0().child(name)),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .flex()
                            .gap_1p5()
                            .font_family(theme::code_font())
                            .text_size(px(theme::font_meta()))
                            .child(
                                div()
                                    .text_color(theme::SUCCESS_BRIGHT)
                                    .child(format!("+{}", entry.additions)),
                            )
                            .child(
                                div()
                                    .text_color(theme::DANGER)
                                    .child(format!("−{}", entry.deletions)),
                            ),
                    ),
            );
        }
        rail = rail.child(rows);
        // the design stacks the rail above the diff when its column is narrow,
        // because 240 px beside a 400 px diff leaves neither readable
        let narrow = self.column_width < STACKED_COLUMN;
        let rail = if narrow {
            div()
                .debug_selector(|| "pull-files-band".into())
                .flex_shrink_0()
                .w_full()
                .h(px(RAIL_BAND))
                .flex()
                .flex_col()
                .border_b_1()
                .border_color(theme::hairline())
                .bg(theme::surface())
                .child(rail)
        } else {
            div()
                .flex_shrink_0()
                .w(px(FILE_RAIL))
                .h_full()
                .flex()
                .flex_col()
                .border_r_1()
                .border_color(theme::hairline())
                .bg(theme::surface())
                .child(rail)
        };

        let patch = file.patch.as_str();
        let has_patch = patch.contains("\n@@");
        let split = self.diff_view == DiffViewMode::Split;
        let body = if has_patch {
            div()
                .id("pull-diff-scroll")
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .child(diff_controls::attach_filling(
                    gpui::ElementId::Name("pull-diff-menu".into()),
                    render_diff_with_scroll(
                        patch,
                        Some(&file.path),
                        warpui_kit_diff::DiffOptions {
                            mode: diff_controls::kit_mode(self.diff_view),
                            wrap: self.diff_wrap,
                        },
                        &self.diff_scroll,
                        theme::font_control(),
                        window,
                        cx,
                    ),
                    warpui_kit_core::text_menu::focus(cx),
                ))
                .into_any_element()
        } else {
            diff_controls::state(if patch.is_empty() {
                "Loading the patch…"
            } else {
                "No textual changes"
            })
        };

        let this_wrap = this.clone();
        let this_view = this.clone();
        let store = self.store.clone();
        let controls = diff_controls::wrap_toggle(
            "pull",
            self.diff_wrap,
            store.clone(),
            move |next, cx| {
                this_wrap.update(cx, |this, cx| {
                    this.diff_wrap = next;
                    cx.notify();
                });
            },
            show_tooltips,
        );
        let view_toggle = diff_controls::view_toggle(
            "pull",
            split,
            store,
            move |next, cx| {
                this_view.update(cx, |this, cx| {
                    this.diff_view = next;
                    cx.notify();
                });
            },
            show_tooltips,
        );

        let diff_column = div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .bg(theme::well())
            .child(
                div()
                    .flex_shrink_0()
                    .h(px(44.))
                    .px_4()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .bg(theme::panel())
                    .border_b_1()
                    .border_color(theme::hairline())
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .font_family(theme::code_font())
                            .text_size(px(theme::font_mono_meta()))
                            .text_color(theme::text())
                            .child(file.path.clone()),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(controls)
                            .child(view_toggle),
                    ),
            )
            .child(body)
            .children(
                (has_patch && !self.diff_wrap)
                    .then(|| diff_controls::scrollbars("pull", &self.diff_scroll, split)),
            );
        let layout = if narrow {
            div().flex_1().min_h_0().min_w_0().flex().flex_col()
        } else {
            div().flex_1().min_h_0().min_w_0().flex().flex_row()
        };
        layout.child(rail).child(diff_column).into_any_element()
    }

    // ---- Commits ----------------------------------------------------------

    #[allow(clippy::too_many_arguments)]
    fn commits_tab(
        &self,
        pull: Option<&GitHubPull>,
        files: Option<&[GitHubFilePatch]>,
        open: &OpenPull,
        window: &mut Window,
        cx: &mut Context<Self>,
        this: &Entity<Self>,
        show_tooltips: bool,
    ) -> AnyElement {
        let Some(pull) = pull else {
            return diff_controls::state(if open.failed {
                "GitHub did not answer."
            } else {
                "Loading the pull request…"
            });
        };
        if pull.commits.is_empty() {
            return diff_controls::state("No commits yet");
        }
        let ix = self.commit_ix.min(pull.commits.len() - 1);
        let commit = &pull.commits[ix];

        let mut list = div()
            .id("pull-commits")
            .debug_selector(|| "pull-commits".into())
            .flex_shrink_0()
            .w(px(COMMIT_LIST))
            .h_full()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .border_r_1()
            .border_color(theme::hairline())
            .bg(theme::surface())
            .py_1();
        let mut day = String::new();
        for (index, entry) in pull.commits.iter().enumerate() {
            let label = entry
                .committed_at
                .as_deref()
                .and_then(warp_station_core::rel_time::day_label)
                .unwrap_or_else(|| "Earlier".to_string());
            if label != day {
                list = list.child(
                    div()
                        .px(px(13.))
                        .h(px(30.))
                        .flex()
                        .items_center()
                        .text_size(px(theme::font_meta()))
                        .text_color(theme::text_muted())
                        .child(label.clone()),
                );
                day = label;
            }
            let selected = index == ix;
            let this = this.clone();
            let selector = format!("pull-commit-{index}");
            let age = entry
                .committed_at
                .as_deref()
                .and_then(warp_station_core::rel_time::format_rfc3339)
                .unwrap_or_default();
            let who = entry.author.clone().unwrap_or_else(|| "Unknown".into());
            let short = short_sha(&entry.oid);
            list = list.child(
                div()
                    .id(SharedString::from(selector.clone()))
                    .debug_selector(move || selector.clone())
                    .a11y_button(entry.headline.clone())
                    .cursor_pointer()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .px(px(13.))
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
                        this.update(cx, |this, cx| {
                            this.commit_ix = index;
                            this.commit_file_ix = 0;
                            cx.notify();
                        });
                    })
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(px(theme::font_meta()))
                            .text_color(theme::text())
                            .child(entry.headline.clone()),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .text_size(px(theme::font_micro()))
                            .text_color(theme::text_muted())
                            .child(avatar(&who, 16.))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .child(format!("{who} · {age}")),
                            )
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .font_family(theme::code_font())
                                    .text_color(theme::text_muted())
                                    .child(short),
                            ),
                    ),
            );
        }

        let file_list: Vec<&GitHubFilePatch> = files
            .map(|files| files.iter().collect())
            .unwrap_or_default();
        let fi = self.commit_file_ix.min(file_list.len().saturating_sub(1));
        let selected_file = file_list.get(fi).copied();
        let adds: u64 = file_list.iter().map(|f| f.additions).sum();
        let dels: u64 = file_list.iter().map(|f| f.deletions).sum();
        let when = commit
            .committed_at
            .as_deref()
            .and_then(warp_station_core::rel_time::format_rfc3339)
            .unwrap_or_default();
        let who = commit.author.clone().unwrap_or_else(|| "Unknown".into());

        let mut detail = div().flex_1().min_w_0().min_h_0().flex().flex_col().child(
            div()
                .flex_shrink_0()
                .px(px(shell::DETAIL_PADDING))
                .py_3()
                .flex()
                .flex_row()
                .items_center()
                .gap_3()
                .border_b_1()
                .border_color(theme::hairline())
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .text_size(px(theme::font_heading()))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(theme::text())
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .child(commit.headline.clone()),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_wrap()
                                .items_center()
                                .gap_2()
                                .text_size(px(theme::font_meta()))
                                .text_color(theme::text_muted())
                                .child(who)
                                .when(!when.is_empty(), |d| d.child("·").child(when))
                                .child("·")
                                .child(
                                    div()
                                        .font_family(theme::code_font())
                                        .text_size(px(theme::font_mono_meta()))
                                        .child(short_sha(&commit.oid)),
                                )
                                .child("·")
                                .child(format!(
                                    "{} file{}",
                                    file_list.len(),
                                    plural(file_list.len())
                                ))
                                .when(!file_list.is_empty(), |d| {
                                    d.child("·")
                                        .child(
                                            div()
                                                .font_family(theme::code_font())
                                                .text_color(theme::SUCCESS_BRIGHT)
                                                .child(format!("+{adds}")),
                                        )
                                        .child(
                                            div()
                                                .font_family(theme::code_font())
                                                .text_color(theme::DANGER)
                                                .child(format!("−{dels}")),
                                        )
                                }),
                        ),
                )
                .child(plain_button(
                    "pull-copy-sha",
                    "Copy SHA",
                    "icons/icon-copy.svg",
                    true,
                    {
                        let sha = commit.oid.clone();
                        move |_, _, cx| {
                            cx.write_to_clipboard(gpui::ClipboardItem::new_string(sha.clone()));
                        }
                    },
                )),
        );

        if file_list.is_empty() {
            detail = detail.child(diff_controls::state(if files.is_none() {
                "Loading the commit…"
            } else {
                "No files in this commit"
            }));
            return div()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_row()
                .child(list)
                .child(detail)
                .into_any_element();
        }

        let mut rail = div().flex().flex_col().child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .h(px(34.))
                .px(px(13.))
                .text_size(px(theme::font_meta()))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::text_dim())
                .child("Files")
                .child(
                    div()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme::text_muted())
                        .child(format!("{}", file_list.len())),
                ),
        );
        let mut rows = div()
            .id("pull-commit-files")
            .debug_selector(|| "pull-commit-files".into())
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .py_1();
        for (index, entry) in file_list.iter().enumerate() {
            let selected = index == fi;
            let this = this.clone();
            let selector = format!("pull-commit-file-{index}");
            let (dir, name) = split_path(&entry.path);
            rows = rows.child(
                div()
                    .id(SharedString::from(selector.clone()))
                    .debug_selector(move || selector.clone())
                    .a11y_button(entry.path.clone())
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .gap_2()
                    .h(px(30.))
                    .pl(px(10.))
                    .pr_2()
                    .border_l(px(3.))
                    .border_color(if selected {
                        gpui::Hsla::from(theme::accent())
                    } else {
                        gpui::transparent_black()
                    })
                    .when(selected, |d| d.bg(theme::raised()))
                    .hover(|d| d.bg(theme::panel()))
                    .on_click(move |_, _, cx| {
                        this.update(cx, |this, cx| {
                            this.commit_file_ix = index;
                            cx.notify();
                        });
                    })
                    .child(
                        div()
                            .flex_shrink_0()
                            .font_family(theme::code_font())
                            .text_size(px(theme::font_mono_meta()))
                            .text_color(diff_controls::status_color(&entry.status))
                            .child(entry.status.clone()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(px(theme::font_meta()))
                            .text_color(theme::text())
                            .child(if dir.is_empty() {
                                name
                            } else {
                                format!("{dir}{name}")
                            }),
                    ),
            );
        }
        rail = rail.child(rows);

        let split = self.diff_view == DiffViewMode::Split;
        let patch = selected_file.map(|f| f.patch.as_str()).unwrap_or_default();
        let has_patch = patch.contains("\n@@");
        let body = if has_patch {
            div()
                .id("pull-commit-diff-scroll")
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .child(diff_controls::attach_filling(
                    gpui::ElementId::Name("pull-commit-diff-menu".into()),
                    render_diff_with_scroll(
                        patch,
                        selected_file.map(|f| f.path.as_str()),
                        warpui_kit_diff::DiffOptions {
                            mode: diff_controls::kit_mode(self.diff_view),
                            wrap: self.diff_wrap,
                        },
                        &self.commit_scroll,
                        theme::font_control(),
                        window,
                        cx,
                    ),
                    warpui_kit_core::text_menu::focus(cx),
                ))
                .into_any_element()
        } else {
            diff_controls::state("No textual changes")
        };
        let path = selected_file.map(|f| f.path.clone()).unwrap_or_default();
        let this_wrap = this.clone();
        let this_view = this.clone();
        let store = self.store.clone();
        let controls = diff_controls::wrap_toggle(
            "pull-commit",
            self.diff_wrap,
            store.clone(),
            move |next, cx| {
                this_wrap.update(cx, |this, cx| {
                    this.diff_wrap = next;
                    cx.notify();
                });
            },
            show_tooltips,
        );
        let view_toggle = diff_controls::view_toggle(
            "pull-commit",
            split,
            store,
            move |next, cx| {
                this_view.update(cx, |this, cx| {
                    this.diff_view = next;
                    cx.notify();
                });
            },
            show_tooltips,
        );
        let diff_column = div()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(theme::well())
            .child(
                div()
                    .flex_shrink_0()
                    .h(px(44.))
                    .px_4()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .bg(theme::panel())
                    .border_b_1()
                    .border_color(theme::hairline())
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .font_family(theme::code_font())
                            .text_size(px(theme::font_mono_meta()))
                            .text_color(theme::text())
                            .child(path),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(controls)
                            .child(view_toggle),
                    ),
            )
            .child(body)
            .children(
                (has_patch && !self.diff_wrap)
                    .then(|| diff_controls::scrollbars("pull-commit", &self.commit_scroll, split)),
            );

        let narrow = self.column_width < STACKED_COLUMN;
        let rail = if narrow {
            div()
                .debug_selector(|| "pull-commit-files-band".into())
                .flex_shrink_0()
                .w_full()
                .h(px(RAIL_BAND))
                .flex()
                .flex_col()
                .border_b_1()
                .border_color(theme::hairline())
                .bg(theme::surface())
                .child(rail)
        } else {
            div()
                .flex_shrink_0()
                .w(px(COMMIT_RAIL))
                .h_full()
                .flex()
                .flex_col()
                .border_r_1()
                .border_color(theme::hairline())
                .bg(theme::surface())
                .child(rail)
        };
        let inner = if narrow {
            div().flex_1().min_h_0().flex().flex_col()
        } else {
            div().flex_1().min_h_0().flex().flex_row()
        };
        detail = detail.child(inner.child(rail).child(diff_column));
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_row()
            .child(list)
            .child(detail)
            .into_any_element()
    }

    // ---- menus ------------------------------------------------------------

    fn primary_menu_card(&self, pull: Option<&GitHubPull>, this: &Entity<Self>) -> AnyElement {
        let mut menu = menu_shell("pull-primary-menu");
        if let Some(pull) = pull {
            let store = self.store.clone();
            let this_click = this.clone();
            menu = menu.child(menu_item(
                "pull-menu-merge",
                "icons/icon-git-pull-request.svg",
                "Merge pull request",
                pull.ready_to_merge(),
                move |_, _, cx| {
                    store.update(cx, |s, cx| s.press_merge(cx));
                    this_click.update(cx, |this, cx| {
                        this.primary_menu = false;
                        cx.notify();
                    });
                },
            ));
            let store = self.store.clone();
            let this_click = this.clone();
            if pull.is_draft {
                menu = menu.child(menu_item(
                    "pull-menu-ready",
                    "icons/icon-check.svg",
                    "Ready for review",
                    true,
                    move |_, _, cx| {
                        store.update(cx, |s, cx| {
                            s.act_on_pull(PullAction::Ready, cx);
                        });
                        this_click.update(cx, |this, cx| {
                            this.primary_menu = false;
                            cx.notify();
                        });
                    },
                ));
            } else {
                menu = menu.child(menu_item(
                    "pull-menu-draft",
                    "icons/icon-text.svg",
                    "Mark as draft",
                    true,
                    move |_, _, cx| {
                        store.update(cx, |s, cx| {
                            s.act_on_pull(PullAction::Draft, cx);
                        });
                        this_click.update(cx, |this, cx| {
                            this.primary_menu = false;
                            cx.notify();
                        });
                    },
                ));
            }
            let store = self.store.clone();
            let this_click = this.clone();
            menu = menu.child(menu_divider()).child(menu_item(
                "pull-menu-abandon",
                "icons/icon-trash.svg",
                "Abandon",
                true,
                move |_, _, cx| {
                    store.update(cx, |s, cx| s.ask_abandon(cx));
                    this_click.update(cx, |this, cx| {
                        this.primary_menu = false;
                        cx.notify();
                    });
                },
            ));
        }
        gpui::deferred(menu).into_any_element()
    }

    fn review_menu_card(&self, pull: Option<&GitHubPull>, this: &Entity<Self>) -> AnyElement {
        let mut menu = menu_shell("pull-review-menu");
        if pull.is_some() {
            let store = self.store.clone();
            let this_click = this.clone();
            menu = menu.child(menu_item(
                "pull-menu-approve",
                "icons/icon-check.svg",
                "Approve",
                true,
                move |_, _, cx| {
                    store.update(cx, |s, cx| {
                        s.act_on_pull(PullAction::Approve, cx);
                    });
                    this_click.update(cx, |this, cx| {
                        this.review_menu = false;
                        cx.notify();
                    });
                },
            ));
            let store = self.store.clone();
            let this_click = this.clone();
            menu = menu.child(menu_item(
                "pull-menu-changes",
                "icons/icon-close.svg",
                "Request changes",
                true,
                move |_, _, cx| {
                    store.update(cx, |s, cx| {
                        s.act_on_pull(PullAction::RequestChanges("Requested changes.".into()), cx);
                    });
                    this_click.update(cx, |this, cx| {
                        this.review_menu = false;
                        cx.notify();
                    });
                },
            ));
        }
        gpui::deferred(menu).into_any_element()
    }
}

// ---- free pieces -----------------------------------------------------------

/// The status strip: why the pull request can't merge, what people said, and
/// who is on it, on the design's 44 px `base` band.
fn status_strip(pull: Option<&GitHubPull>, open: &OpenPull) -> AnyElement {
    let strip = div()
        .debug_selector(|| "pull-status".into())
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
    let Some(pull) = pull else {
        return strip
            .child(if open.failed {
                "GitHub did not answer."
            } else {
                "Loading the pull request…"
            })
            .into_any_element();
    };

    let mut facts = div()
        .flex()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap(px(18.));
    if pull.changes_requested() > 0 {
        facts = facts.child(labelled(
            "icons/icon-close.svg",
            theme::DANGER,
            "Changes requested",
        ));
    } else if pull.has_conflicts() {
        facts = facts.child(labelled(
            "icons/icon-close.svg",
            theme::DANGER,
            "Merge conflict",
        ));
    }
    facts = facts.child(match pull.approvals() {
        0 => "No approvals".to_string(),
        1 => "1 approval".to_string(),
        n => format!("{n} approvals"),
    });
    let comments = pull.comments.len();
    facts = facts.child(format!("{comments} comment{}", plural(comments)));
    facts = facts.children(
        pull.updated_at
            .as_deref()
            .and_then(warp_station_core::rel_time::format_rfc3339)
            .map(|when| format!("Updated {when}")),
    );

    let mut right = div().flex().flex_row().items_center().gap_3();
    if pull.auto_merge {
        right = right.child(labelled(
            "icons/icon-clock.svg",
            theme::BEHIND,
            "Auto-complete set",
        ));
    }
    let people: Vec<&str> = pull
        .reviews
        .iter()
        .map(|review| review.author.as_str())
        .chain(pull.requested_reviewers.iter().map(String::as_str))
        .collect();
    if !people.is_empty() {
        let mut stack = div()
            .debug_selector(|| "pull-reviewers".into())
            .flex()
            .flex_row()
            .items_center();
        for (ix, who) in people.iter().take(3).enumerate() {
            let mut chip = div().flex_shrink_0();
            if ix > 0 {
                chip = chip.ml(px(-6.));
            }
            stack = stack.child(chip.child(ringed_avatar(who, 22.)));
        }
        if people.len() > 3 {
            stack = stack.child(
                div()
                    .ml(px(-6.))
                    .flex_shrink_0()
                    .h(px(22.))
                    .px(px(6.))
                    .rounded_full()
                    .bg(theme::raised())
                    .flex()
                    .items_center()
                    .text_size(px(theme::font_micro()))
                    .text_color(theme::text_dim())
                    .child(format!("+{}", people.len() - 3)),
            );
        }
        right = right.child(stack);
    }

    strip
        .child(facts)
        .child(div().flex_1())
        .child(right)
        .into_any_element()
}

fn subtitle(open: &OpenPull, pull: Option<&GitHubPull>, repo_name: &str) -> AnyElement {
    let item = &open.item;
    let author = pull
        .and_then(|p| p.author.clone())
        .or_else(|| item.author.clone())
        .unwrap_or_else(|| "ghost".into());
    let head_ref = pull
        .map(|p| p.head_ref_name.clone())
        .or_else(|| item.head_ref_name.clone())
        .unwrap_or_default();
    let base_ref = pull
        .map(|p| p.base_ref_name.clone())
        .or_else(|| item.base_ref_name.clone())
        .unwrap_or_default();
    let mut row = div()
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
                .child(author.clone()),
        )
        .child(format!("{repo_name} #{}", item.number));
    if !head_ref.is_empty() {
        row = row.child(
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
                        .child(format!("{head_ref} → {base_ref}")),
                ),
        );
    }
    row = row
        .children(pull.and_then(check_summary))
        .children(pull.map(|p| {
            let (adds, dels) = p.line_counts();
            div()
                .flex()
                .gap_1p5()
                .font_family(theme::code_font())
                .text_size(px(theme::font_mono_meta()))
                .child(div().text_color(theme::ADD).child(format!("+{adds}")))
                .child(div().text_color(theme::DEL).child(format!("−{dels}")))
        }))
        .child(source_tag("GitHub"));
    row.into_any_element()
}

/// Approve with its review menu, the one filled primary with its menu, and the
/// quiet open-in-browser icon.
#[allow(clippy::too_many_arguments)]
fn head_actions(
    store: &Entity<Store>,
    pull: Option<&GitHubPull>,
    acting: bool,
    merging: bool,
    merge_armed: bool,
    closed: bool,
    url: String,
    this: &Entity<PullRequestView>,
) -> AnyElement {
    let mut row = div()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .flex_shrink_0();
    let busy = acting || merging;

    if let Some(pull) = pull {
        if !closed && !pull.is_draft {
            let this_click = this.clone();
            let store_click = store.clone();
            let approve = plain_button(
                "pull-approve",
                "Approve",
                "icons/icon-check.svg",
                !busy,
                move |_, _, cx| {
                    store_click.update(cx, |store, cx| {
                        store.act_on_pull(PullAction::Approve, cx);
                    });
                    this_click.update(cx, |this, cx| {
                        this.review_menu = false;
                        cx.notify();
                    });
                },
            );
            let this_menu = this.clone();
            row = row.child(split_button(
                "pull-approve",
                approve,
                false,
                move |_, _, cx| {
                    this_menu.update(cx, |this, cx| {
                        this.review_menu = !this.review_menu;
                        this.primary_menu = false;
                        cx.notify();
                    });
                },
            ));
        }

        let primary: AnyElement = if closed {
            if pull.is_closed() {
                let this_click = this.clone();
                let store_click = store.clone();
                plain_button(
                    "pull-reopen",
                    "Reopen",
                    "icons/icon-undo.svg",
                    !busy,
                    move |_, _, cx| {
                        store_click.update(cx, |store, cx| {
                            store.act_on_pull(PullAction::Reopen, cx);
                        });
                        this_click.update(cx, |_, cx| cx.notify());
                    },
                )
            } else {
                div().into_any_element()
            }
        } else if pull.is_draft {
            let this_click = this.clone();
            let store_click = store.clone();
            primary_button_in(
                "pull-ready",
                "Ready for review",
                !busy,
                Shape::Left,
                move |_, _, cx| {
                    store_click.update(cx, |store, cx| {
                        store.act_on_pull(PullAction::Ready, cx);
                    });
                    this_click.update(cx, |_, cx| cx.notify());
                },
            )
        } else {
            let store_click = store.clone();
            let armed = merge_armed;
            let ready = pull.ready_to_merge() && !busy;
            let label = if merging {
                "Merging…"
            } else if armed {
                "Confirm merge"
            } else {
                "Merge pull request"
            };
            div()
                .id("pull-merge")
                .debug_selector(|| "pull-merge".into())
                .a11y_button(label)
                .flex_shrink_0()
                .h(px(shell::CONTROL_HEIGHT))
                .px_3()
                .rounded_l(px(6.))
                .flex()
                .items_center()
                .gap_2()
                .text_size(px(theme::font_control()))
                .font_weight(FontWeight::SEMIBOLD)
                .when(ready, |d| {
                    d.cursor_pointer()
                        .bg(if armed {
                            theme::DANGER
                        } else {
                            theme::accent()
                        })
                        .text_color(if armed {
                            theme::ON_DANGER
                        } else {
                            theme::on_accent()
                        })
                        .hover(|s| s.opacity(0.92))
                        .on_click(move |_, _, cx| store_click.update(cx, |s, cx| s.press_merge(cx)))
                })
                .when(!ready, |d| {
                    d.bg(theme::raised()).text_color(theme::text_muted())
                })
                .when(merging, |d| {
                    d.child(motion::spinner(
                        "pull-merge-spinner",
                        "icons/icon-spinner.svg",
                        px(theme::icon_small()),
                        theme::text_muted(),
                    ))
                })
                .child(label)
                .into_any_element()
        };

        if closed {
            row = row.child(primary);
        } else {
            let this_menu = this.clone();
            row = row.child(split_button(
                "pull-primary",
                primary,
                true,
                move |_, _, cx| {
                    this_menu.update(cx, |this, cx| {
                        this.primary_menu = !this.primary_menu;
                        this.review_menu = false;
                        cx.notify();
                    });
                },
            ));
        }
    }

    row.child(
        div()
            .id("pull-github")
            .debug_selector(|| "pull-github".into())
            .a11y_button("Open on GitHub")
            .cursor_pointer()
            .flex_shrink_0()
            .size(px(shell::CONTROL_HEIGHT))
            .rounded(px(6.))
            .flex()
            .items_center()
            .justify_center()
            .hover(|s| s.bg(theme::raised()))
            .when(!url.is_empty(), |d| {
                d.on_click(move |_, _, _| {
                    let _ = warp_station_core::terminal::open_folder(&url);
                })
            })
            .child(icon(
                "icons/icon-open-in-new.svg",
                px(theme::icon_small()),
                theme::text_dim(),
            )),
    )
    .into_any_element()
}

/// A split button: the action, then the chevron that opens its menu. The two
/// halves share one fill and one edge, so they read as the one control the
/// design draws.
pub(crate) fn split_button(
    prefix: &'static str,
    action: AnyElement,
    filled: bool,
    on_menu: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let chevron = format!("{prefix}-menu");
    let ink = if filled {
        theme::on_accent()
    } else {
        theme::text_dim()
    };
    div()
        .flex_shrink_0()
        .flex()
        .flex_row()
        .child(
            div()
                .flex_shrink_0()
                .flex()
                .child(action)
                .rounded_l(px(6.))
                .overflow_hidden(),
        )
        .child(
            div()
                .id(chevron.clone())
                .debug_selector(move || chevron.clone())
                .a11y_button("More actions")
                .cursor_pointer()
                .flex_shrink_0()
                .w(px(28.))
                .h(px(shell::CONTROL_HEIGHT))
                .flex()
                .items_center()
                .justify_center()
                .rounded_r(px(6.))
                .when(filled, |d| {
                    d.bg(theme::accent())
                        .border_1()
                        .border_color(theme::accent_hover())
                        .hover(|s| s.bg(theme::accent_hover()))
                })
                .when(!filled, |d| {
                    d.bg(theme::panel())
                        .border_1()
                        .border_color(theme::line_strong())
                        .hover(|s| s.bg(theme::raised()))
                })
                .on_click(on_menu)
                .child(icon(
                    "icons/icon-chevron-down.svg",
                    px(theme::icon_tiny()),
                    ink,
                )),
        )
        .into_any_element()
}

fn description_card(pull: &GitHubPull) -> AnyElement {
    let body = if pull.body.trim().is_empty() {
        div()
            .p_4()
            .text_size(px(theme::font_meta()))
            .text_color(theme::text_muted())
            .child("No description.")
            .into_any_element()
    } else {
        div()
            .p_4()
            .text_size(px(theme::font_body()))
            .text_color(theme::text())
            .child(
                TextView::markdown(
                    gpui::ElementId::Name("pull-description".into()),
                    SharedString::from(pull.body.clone()),
                )
                .selectable(true),
            )
            .into_any_element()
    };
    div()
        .debug_selector(|| "pull-description".into())
        .rounded(px(8.))
        .border_1()
        .border_color(theme::hairline())
        .bg(theme::card())
        .overflow_hidden()
        .flex()
        .flex_col()
        .child(
            div()
                .h(px(40.))
                .px_4()
                .flex()
                .items_center()
                .border_b_1()
                .border_color(theme::row_line())
                .child(
                    div()
                        .text_size(px(theme::font_heading()))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::text())
                        .child("Description"),
                ),
        )
        .child(body)
        .into_any_element()
}

/// The reviewers' card: who is still owed a review, and what the reviews said.
fn reviewer_card(pull: &GitHubPull) -> AnyElement {
    let mut body = div().px_4().py_2().flex().flex_col();
    if pull.requested_reviewers.is_empty() && pull.reviews.is_empty() {
        body = body.child(
            div()
                .py_2()
                .text_size(px(theme::font_meta()))
                .text_color(theme::text_muted())
                .child("No reviewers yet."),
        );
    }
    if !pull.requested_reviewers.is_empty() {
        body = body.child(reviewer_group(
            "Required",
            pull.requested_reviewers.iter().map(|who| {
                (
                    who.as_str(),
                    "Awaiting review",
                    theme::text_muted(),
                    "icons/icon-clock.svg",
                )
            }),
        ));
    }
    if !pull.reviews.is_empty() {
        body = body.child(reviewer_group(
            "Reviews",
            pull.reviews.iter().map(|review| {
                let (label, color, path) = match review.state.as_str() {
                    "APPROVED" => ("Approved", theme::SUCCESS_BRIGHT, "icons/icon-check.svg"),
                    "CHANGES_REQUESTED" => {
                        ("Changes requested", theme::DANGER, "icons/icon-close.svg")
                    }
                    "DISMISSED" => ("Dismissed", theme::text_muted(), "icons/icon-clock.svg"),
                    _ => ("Commented", theme::text_muted(), "icons/icon-text.svg"),
                };
                (review.author.as_str(), label, color, path)
            }),
        ));
    }
    div()
        .debug_selector(|| "pull-reviewers-card".into())
        .rounded(px(8.))
        .border_1()
        .border_color(theme::hairline())
        .bg(theme::card())
        .overflow_hidden()
        .flex()
        .flex_col()
        .child(
            div()
                .h(px(40.))
                .px_4()
                .flex()
                .items_center()
                .gap_2()
                .border_b_1()
                .border_color(theme::row_line())
                .child(
                    div()
                        .flex_1()
                        .text_size(px(theme::font_heading()))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::text())
                        .child("Reviewers"),
                )
                .child(
                    div()
                        .text_size(px(theme::font_meta()))
                        .text_color(theme::text_muted())
                        .child(format!("{} required", pull.requested_reviewers.len())),
                ),
        )
        .child(body)
        .into_any_element()
}

fn reviewer_group<'a>(
    label: &'a str,
    rows: impl Iterator<Item = (&'a str, &'a str, Rgba, &'a str)>,
) -> AnyElement {
    let mut group = div()
        .py_1()
        .flex()
        .flex_col()
        .gap_0p5()
        .border_t_1()
        .border_color(theme::row_line())
        .child(
            div()
                .py(px(2.))
                .text_size(px(theme::font_meta()))
                .text_color(theme::text_muted())
                .child(label.to_string()),
        );
    for (who, state, color, path) in rows {
        group = group.child(
            div()
                .h(px(44.))
                .flex()
                .items_center()
                .gap_2p5()
                .child(avatar(who, 24.))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .text_size(px(theme::font_body()))
                                .text_color(theme::text())
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .child(who.to_string()),
                        )
                        .child(
                            div()
                                .text_size(px(theme::font_meta()))
                                .text_color(theme::text_muted())
                                .child(state.to_string()),
                        ),
                )
                .child(icon(path, px(theme::icon_small()), color)),
        );
    }
    group.into_any_element()
}

fn check_summary(pull: &GitHubPull) -> Option<AnyElement> {
    if pull.checks.is_empty() {
        return None;
    }
    let failed = pull.count_checks(CheckState::Failed);
    let running = pull.count_checks(CheckState::Running);
    let (path, color, word) = if failed > 0 {
        (
            "icons/icon-close.svg",
            theme::DANGER,
            format!("{failed} check{} failing", plural(failed)),
        )
    } else if running > 0 {
        (
            "icons/icon-clock.svg",
            theme::BEHIND,
            format!("{running} check{} running", plural(running)),
        )
    } else {
        (
            "icons/icon-check.svg",
            theme::SUCCESS_BRIGHT,
            "All checks passing".to_string(),
        )
    };
    Some(labelled(path, color, word))
}

fn state_pill(pull: Option<&GitHubPull>) -> AnyElement {
    let (label, color) = match pull {
        Some(p) if p.is_merged() => ("Merged", theme::PR),
        Some(p) if p.is_closed() => ("Closed", theme::text_muted()),
        Some(p) if p.is_draft => ("Draft", theme::text_muted()),
        _ => ("Open", theme::PR),
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
        .child(icon(
            if label == "Closed" {
                "icons/icon-close.svg"
            } else {
                "icons/icon-git-pull-request.svg"
            },
            px(theme::icon_tiny()),
            color,
        ))
        .child(label)
        .into_any_element()
}

pub(crate) fn source_tag(label: &'static str) -> AnyElement {
    div()
        .flex_shrink_0()
        .rounded(px(4.))
        .bg(theme::raised())
        .px_1p5()
        .py_0p5()
        .text_size(px(theme::font_micro()))
        .text_color(theme::text_muted())
        .child(label)
        .into_any_element()
}

fn card(
    selector: &'static str,
    title: &'static str,
    trailing: &'static str,
    body: AnyElement,
) -> AnyElement {
    div()
        .debug_selector(move || selector.into())
        .rounded(px(8.))
        .border_1()
        .border_color(theme::hairline())
        .bg(theme::card())
        .overflow_hidden()
        .flex()
        .flex_col()
        .child(
            div()
                .h(px(40.))
                .px_4()
                .flex()
                .items_center()
                .gap_2()
                .border_b_1()
                .border_color(theme::row_line())
                .child(
                    div()
                        .flex_1()
                        .text_size(px(theme::font_heading()))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::text())
                        .child(title),
                )
                .child(
                    div()
                        .text_size(px(theme::font_meta()))
                        .text_color(theme::text_muted())
                        .child(trailing),
                ),
        )
        .child(body)
        .into_any_element()
}

fn policy_row(path: &'static str, color: Rgba, title: String, detail: String) -> AnyElement {
    div()
        .px_4()
        .py_3()
        .flex()
        .items_start()
        .gap_3()
        .border_t_1()
        .border_color(theme::row_line())
        .child(icon(path, px(theme::icon_small()), color))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(
                    div()
                        .text_size(px(theme::font_body()))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme::text())
                        .child(title),
                )
                .when(!detail.is_empty(), |d| {
                    d.child(
                        div()
                            .mt_0p5()
                            .text_size(px(theme::font_meta()))
                            .text_color(theme::text_muted())
                            .child(detail),
                    )
                }),
        )
        .into_any_element()
}

fn check_row(ix: usize, check: &GitHubCheck) -> AnyElement {
    let glyph = match check.state {
        CheckState::Passed => icon(
            "icons/icon-check.svg",
            px(theme::icon_small()),
            theme::SUCCESS_BRIGHT,
        )
        .into_any_element(),
        CheckState::Failed => icon(
            "icons/icon-close.svg",
            px(theme::icon_small()),
            theme::DANGER,
        )
        .into_any_element(),
        CheckState::Running => motion::spinner(
            SharedString::from(format!("pull-check-spinner-{ix}")),
            "icons/icon-spinner.svg",
            px(theme::icon_small()),
            theme::BEHIND,
        )
        .into_any_element(),
        CheckState::Skipped => icon(
            "icons/icon-clock.svg",
            px(theme::icon_small()),
            theme::text_muted(),
        )
        .into_any_element(),
    };
    let url = check.url.clone();
    let selector = format!("pull-check-{ix}");
    div()
        .id(SharedString::from(selector.clone()))
        .debug_selector(move || selector.clone())
        .a11y_button(check.name.clone())
        .min_h(px(38.))
        .px_4()
        .flex()
        .items_center()
        .gap_2p5()
        .text_size(px(theme::font_body()))
        .text_color(theme::text())
        .when(check.state == CheckState::Failed, |d| {
            d.bg(theme::DIFF_DEL_BG)
        })
        .when_some(url, |d, url| {
            d.cursor_pointer()
                .hover(|s| s.bg(theme::raised()))
                .on_click(move |_, _, _| {
                    let _ = warp_station_core::terminal::open_folder(&url);
                })
        })
        .child(glyph)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(check.name.clone()),
        )
        .into_any_element()
}

pub(crate) fn comment_card(
    ix: usize,
    author: &str,
    verb: String,
    text: &str,
    focus: Option<gpui::FocusHandle>,
) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .gap_3()
        .child(avatar(author, 32.))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .rounded(px(8.))
                .bg(theme::card())
                .border_1()
                .border_color(theme::hairline())
                .overflow_hidden()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .px_4()
                        .h(px(40.))
                        .border_b_1()
                        .border_color(theme::row_line())
                        .child(
                            div()
                                .text_size(px(theme::font_body()))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(theme::text())
                                .child(author.to_string()),
                        )
                        .child(
                            div()
                                .text_size(px(theme::font_meta()))
                                .text_color(theme::text_muted())
                                .child(verb),
                        ),
                )
                .child(warpui_kit_core::text_menu::attach(
                    gpui::ElementId::Name(format!("pull-comment-menu-{ix}").into()),
                    div().p_4().child(
                        TextView::markdown(
                            gpui::ElementId::Name(format!("pull-comment-{ix}").into()),
                            SharedString::from(text.to_string()),
                        )
                        .selectable(true),
                    ),
                    focus,
                )),
        )
        .into_any_element()
}

fn labelled(path: &'static str, color: Rgba, word: impl Into<SharedString>) -> AnyElement {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap_1p5()
        .text_color(color)
        .child(icon(path, px(theme::icon_tiny()), color))
        .child(word.into())
        .into_any_element()
}

/// How a button's corners sit: on its own, or as the left half of a split
/// whose chevron carries the right half.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Shape {
    Whole,
    Left,
}

/// A neutral action at the control height: `panel` fill, `line-strong` edge.
pub(crate) fn plain_button(
    id: &'static str,
    label: &'static str,
    icon_path: &'static str,
    enabled: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    plain_button_in(id, label, icon_path, enabled, Shape::Whole, on_click)
}

pub(crate) fn plain_button_in(
    id: &'static str,
    label: &'static str,
    icon_path: &'static str,
    enabled: bool,
    shape: Shape,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let mut button = div()
        .id(id)
        .debug_selector(move || id.into())
        .a11y_button(label)
        .flex_shrink_0()
        .h(px(shell::CONTROL_HEIGHT))
        .px_3()
        .border_1()
        .border_color(theme::line_strong())
        .bg(theme::panel())
        .flex()
        .items_center()
        .gap_2()
        .text_size(px(theme::font_control()))
        .font_weight(FontWeight::MEDIUM)
        .text_color(theme::text());
    button = match shape {
        Shape::Whole => button.rounded(px(6.)),
        // square on the right: the chevron beside it is the other half
        Shape::Left => button.rounded_l(px(6.)),
    };
    button
        .when(enabled, |d| {
            d.cursor_pointer()
                .hover(|s| s.bg(theme::raised()))
                .on_click(on_click)
        })
        .when(!enabled, |d| d.opacity(0.45))
        .child(icon(icon_path, px(theme::icon_small()), theme::text_dim()))
        .child(label)
        .into_any_element()
}

/// The one filled action.
pub(crate) fn primary_button(
    id: &'static str,
    label: &'static str,
    enabled: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    primary_button_in(id, label, enabled, Shape::Whole, on_click)
}

pub(crate) fn primary_button_in(
    id: &'static str,
    label: &'static str,
    enabled: bool,
    shape: Shape,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let mut button = div()
        .id(id)
        .debug_selector(move || id.into())
        .a11y_button(label)
        .flex_shrink_0()
        .h(px(shell::CONTROL_HEIGHT))
        .px_3()
        .bg(theme::accent())
        .flex()
        .items_center()
        .gap_2()
        .text_size(px(theme::font_control()))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme::on_accent());
    button = match shape {
        Shape::Whole => button.rounded(px(6.)),
        Shape::Left => button.rounded_l(px(6.)),
    };
    button
        .when(enabled, |d| {
            d.cursor_pointer()
                .hover(|s| s.bg(theme::accent_hover()))
                .on_click(on_click)
        })
        .when(!enabled, |d| d.opacity(0.45))
        .child(label)
        .into_any_element()
}

pub(crate) fn menu_shell(selector: &'static str) -> gpui::Stateful<gpui::Div> {
    div()
        .id(selector)
        .debug_selector(move || selector.into())
        .absolute()
        .top(px(56.))
        .right(px(shell::DETAIL_PADDING))
        .w(px(240.))
        .p(px(4.))
        .rounded(px(8.))
        .border_1()
        .border_color(theme::line_strong())
        .bg(theme::overlay())
        .shadow_lg()
        .occlude()
        .flex()
        .flex_col()
        .gap(px(2.))
}

pub(crate) fn menu_item(
    id: &'static str,
    icon_path: &'static str,
    label: &'static str,
    enabled: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .a11y_button(label)
        .h(px(32.))
        .px_2()
        .rounded(px(6.))
        .flex()
        .items_center()
        .gap_2()
        .text_size(px(theme::font_control()))
        .text_color(theme::text())
        .when(enabled, |d| {
            d.cursor_pointer()
                .hover(|s| s.bg(theme::line_strong()))
                .on_click(on_click)
        })
        .when(!enabled, |d| d.opacity(0.45))
        .child(icon(icon_path, px(theme::icon_small()), theme::text_dim()))
        .child(label)
        .into_any_element()
}

pub(crate) fn menu_divider() -> AnyElement {
    div()
        .my_1()
        .h(px(1.))
        .bg(theme::hairline())
        .into_any_element()
}

pub(crate) fn plural(count: usize) -> &'static str {
    if count == 1 { "" } else { "s" }
}

pub(crate) fn short_sha(oid: &str) -> String {
    oid.chars().take(7).collect()
}

fn split_path(path: &str) -> (String, String) {
    match path.rsplit_once('/') {
        Some((dir, name)) => (format!("{dir}/"), name.to_string()),
        None => (String::new(), path.to_string()),
    }
}

fn selected_commit(open: &OpenPull, index: usize) -> Option<&GitHubCommit> {
    let pull = open.pull.as_ref()?;
    let index = index.min(pull.commits.len().saturating_sub(1));
    pull.commits.get(index)
}

/// Initials on a circle: gh hands out no picture the window could draw offline.
pub(crate) fn avatar(name: &str, size: f32) -> AnyElement {
    div()
        .flex_shrink_0()
        .size(px(size))
        .rounded_full()
        .bg(theme::raised())
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(theme::font_micro()))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme::text_dim())
        .child(initials(name))
        .into_any_element()
}

/// The status strip's overlapping stack: each face carries a ring in the
/// strip's own surface so two avatars never blur into one.
fn ringed_avatar(name: &str, size: f32) -> AnyElement {
    div()
        .flex_shrink_0()
        .rounded_full()
        .border_2()
        .border_color(theme::surface())
        .child(avatar(name, size))
        .into_any_element()
}

pub(crate) fn initials(name: &str) -> String {
    name.split(['-', '_', ' '])
        .filter_map(|part| part.chars().next())
        .take(2)
        .flat_map(char::to_uppercase)
        .collect()
}
