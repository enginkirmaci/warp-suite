//! An Azure DevOps pull request in the Pipelines tab's detail column: the same
//! pane shape as the GitHub pull request, with a second data source behind it.
//! The head carries the state, the review verb and the one filled primary; a
//! status strip holds the approvals and the age; then Overview (policies,
//! description, threads), Files (the changed-file rail and the file's diff) and
//! Commits.
//!
//! Azure hands out no patch with its change list, so a file's diff is a second
//! call per file: the pane says so rather than pretending it has one.

use crate::store::Store;
use crate::theme;
use crate::ui::diff_controls;
use crate::ui::pull_request::{
    Shape, avatar, comment_card, menu_divider, menu_item, menu_shell, plain_button,
    plain_button_in, plural, primary_button_in, source_tag, split_button,
};
use crate::ui::{A11y, icon};
use gpui::prelude::*;
use gpui::{AnyElement, Context, Entity, FontWeight, Rgba, SharedString, Window, div, px};
use gpui_kit::base::text::TextView;
use warp_station_core::azure::{AzureFile, AzurePolicy, AzurePull, AzurePullAction, AzureThread};
use warp_station_core::settings::DiffViewMode;
use warpui_kit_core::{motion, shell};
use warpui_kit_diff::{DiffScrollHandles, render_diff_with_scroll};

const STRIP_HEIGHT: f32 = 44.;
const FILE_RAIL: f32 = 240.;
const RAIL_BAND: f32 = 180.;
const STACKED_COLUMN: f32 = 900.;
const READING: f32 = 760.;

/// The Azure pull request's tabs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AzureTab {
    #[default]
    Overview,
    Files,
    Commits,
}

impl AzureTab {
    pub const ALL: [AzureTab; 3] = [AzureTab::Overview, AzureTab::Files, AzureTab::Commits];

    pub fn label(self) -> &'static str {
        match self {
            AzureTab::Overview => "Overview",
            AzureTab::Files => "Files",
            AzureTab::Commits => "Commits",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            AzureTab::Overview => "overview",
            AzureTab::Files => "files",
            AzureTab::Commits => "commits",
        }
    }
}

pub struct AzurePullView {
    store: Entity<Store>,
    tab: AzureTab,
    file_ix: usize,
    diff_view: DiffViewMode,
    diff_wrap: bool,
    diff_scroll: DiffScrollHandles,
    review_menu: bool,
    primary_menu: bool,
    /// The pull request the view's own state belongs to.
    showing: Option<u64>,
    /// The view is the whole detail pane: the workbench's list is not beside it.
    pub stacked: bool,
    /// The detail column's width: the Files rail reads it.
    pub column_width: f32,
}

impl AzurePullView {
    pub fn new(store: Entity<Store>, cx: &mut Context<Self>) -> Self {
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        let diff_view = store.read(cx).diff_view();
        let diff_wrap = store.read(cx).diff_wrap();
        Self {
            store,
            tab: AzureTab::Overview,
            file_ix: 0,
            diff_view,
            diff_wrap,
            diff_scroll: DiffScrollHandles::default(),
            review_menu: false,
            primary_menu: false,
            showing: None,
            stacked: false,
            column_width: f32::MAX,
        }
    }

    /// Escape: the head's menus first. The workbench closes the pull request.
    pub fn dismiss(&mut self, cx: &mut Context<Self>) -> bool {
        if self.review_menu || self.primary_menu {
            self.review_menu = false;
            self.primary_menu = false;
            cx.notify();
            return true;
        }
        false
    }

    fn reset_for(&mut self, number: u64) {
        if self.showing == Some(number) {
            return;
        }
        self.showing = Some(number);
        self.tab = AzureTab::Overview;
        self.file_ix = 0;
        self.review_menu = false;
        self.primary_menu = false;
    }
}

impl Render for AzurePullView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        let this = cx.entity();
        let Some(open) = self.store.read(cx).open_azure_pull.clone() else {
            return div().into_any_element();
        };
        let number = open.item.number;
        self.reset_for(number);
        // the tabs' data, asked for as the tab that needs them draws
        if self.tab == AzureTab::Files {
            self.store
                .update(cx, |store, cx| store.load_azure_pull_files(cx));
        }
        if self.tab == AzureTab::Commits {
            self.store
                .update(cx, |store, cx| store.load_azure_pull_commits(cx));
        }
        let file_path = (self.tab == AzureTab::Files)
            .then(|| {
                let files = open.files.clone().unwrap_or_default();
                files
                    .get(self.file_ix.min(files.len().saturating_sub(1)))
                    .map(|file| file.path.clone())
            })
            .flatten();
        if let Some(path) = file_path.as_deref() {
            self.store
                .update(cx, |store, cx| store.load_azure_pull_diff(path, cx));
        }

        let store = self.store.read(cx);
        let show_tooltips = store.settings().general.show_tooltips;
        let repo_name = store
            .selected_entry()
            .map(|repo| repo.name.clone())
            .unwrap_or_default();
        let detail = open.detail.as_ref();
        let pull = detail.map(|detail| &detail.pull);
        let policies: &[AzurePolicy] = detail.map(|d| d.policies.as_slice()).unwrap_or(&[]);
        let threads: &[AzureThread] = detail.map(|d| d.threads.as_slice()).unwrap_or(&[]);
        let acting = open.acting;

        let body = match self.tab {
            AzureTab::Overview => self.overview(pull, policies, threads, &open, cx),
            AzureTab::Files => self.files_tab(&open, window, cx, &this, show_tooltips),
            AzureTab::Commits => self.commits_tab(&open, cx),
        };

        let pane = div()
            .debug_selector(|| "azure-pull".into())
            .relative()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(theme::panel())
            .child(self.head(&open, pull, policies, acting, &repo_name, &this))
            .child(status_strip(pull, threads, &open))
            .child(self.tab_strip(pull, &open, &this))
            .child(body);
        pane.children(
            self.primary_menu
                .then(|| self.primary_menu_card(pull, policies, acting, &this)),
        )
        .children(
            self.review_menu
                .then(|| self.review_menu_card(pull, acting, &this)),
        )
        .into_any_element()
    }
}

impl AzurePullView {
    // ---- head -------------------------------------------------------------

    fn head(
        &self,
        open: &crate::store::OpenAzurePull,
        pull: Option<&AzurePull>,
        policies: &[AzurePolicy],
        acting: bool,
        repo_name: &str,
        this: &Entity<Self>,
    ) -> AnyElement {
        let item = &open.item;
        let title = pull.map_or(item.title.as_str(), |pull| pull.title.as_str());
        let url = pull
            .and_then(|pull| pull.url.clone())
            .filter(|url| !url.is_empty())
            .unwrap_or_else(|| item.url.clone());
        let active = pull.is_some_and(AzurePull::is_active);
        let draft = pull.is_some_and(|pull| pull.is_draft);

        let mut title_row = div().flex().flex_row().flex_wrap().items_center().gap_2();
        if self.stacked {
            let store = self.store.clone();
            title_row = title_row.child(
                div()
                    .id("azure-pull-back")
                    .debug_selector(|| "azure-pull-back".into())
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
                            store.close_azure_pull(cx);
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
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(px(theme::font_screen_title()))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme::text())
                    .child(title.to_string()),
            )
            .child(state_pill(pull));

        if let Some(pull) = pull {
            let mut actions = div().flex().flex_row().items_center().gap_2();
            // the review verb: a vote, so Approve is a split with Decline in
            // its menu
            if active && !draft {
                let this_click = this.clone();
                let store_click = self.store.clone();
                let reviewer = pull.my_reviewer().map(|r| r.id.clone());
                let approve = plain_button_in(
                    "azure-approve",
                    "Approve",
                    "icons/icon-check.svg",
                    !acting && reviewer.is_some(),
                    Shape::Left,
                    move |_, _, cx| {
                        if let Some(reviewer) = reviewer.clone() {
                            store_click.update(cx, |store, cx| {
                                store.act_on_azure_pull(
                                    AzurePullAction::Vote { reviewer, vote: 10 },
                                    cx,
                                );
                            });
                        }
                        this_click.update(cx, |view, cx| {
                            view.review_menu = false;
                            cx.notify();
                        });
                    },
                );
                let this_menu = this.clone();
                actions = actions.child(split_button(
                    "azure-approve",
                    approve,
                    false,
                    move |_, _, cx| {
                        this_menu.update(cx, |view, cx| {
                            view.review_menu = !view.review_menu;
                            view.primary_menu = false;
                            cx.notify();
                        });
                    },
                ));
            }

            let primary: AnyElement = if pull.is_abandoned() {
                let store_click = self.store.clone();
                plain_button(
                    "azure-reopen",
                    "Reopen",
                    "icons/icon-undo.svg",
                    !acting,
                    move |_, _, cx| {
                        store_click.update(cx, |store, cx| {
                            store.act_on_azure_pull(AzurePullAction::Reopen, cx);
                        });
                    },
                )
            } else if pull.is_completed() {
                div().into_any_element()
            } else if pull.auto_complete {
                let store_click = self.store.clone();
                plain_button(
                    "azure-autocomplete",
                    "Auto-complete set",
                    "icons/icon-clock.svg",
                    !acting,
                    move |_, _, cx| {
                        store_click.update(cx, |store, cx| {
                            store.act_on_azure_pull(AzurePullAction::CancelAutoComplete, cx);
                        });
                    },
                )
            } else if pull.ready_to_complete(policies) {
                let store_click = self.store.clone();
                let armed = open.armed_complete;
                let label = if acting {
                    "Completing…"
                } else if armed {
                    "Confirm complete"
                } else {
                    "Complete"
                };
                div()
                    .id("azure-complete")
                    .debug_selector(|| "azure-complete".into())
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
                    .when(!acting, |d| {
                        d.cursor_pointer().on_click(move |_, _, cx| {
                            store_click.update(cx, |store, cx| {
                                store.act_on_azure_pull(AzurePullAction::Complete, cx);
                            });
                        })
                    })
                    .when(acting, |d| {
                        d.child(motion::spinner(
                            "azure-complete-spinner",
                            "icons/icon-spinner.svg",
                            px(theme::icon_small()),
                            theme::on_accent(),
                        ))
                    })
                    .child(label)
                    .into_any_element()
            } else {
                let store_click = self.store.clone();
                let reviewer = pull.me.clone();
                primary_button_in(
                    "azure-autocomplete",
                    "Set auto-complete",
                    !acting && reviewer.is_some(),
                    Shape::Left,
                    move |_, _, cx| {
                        if let Some(reviewer) = reviewer.clone() {
                            store_click.update(cx, |store, cx| {
                                store.act_on_azure_pull(
                                    AzurePullAction::SetAutoComplete { reviewer },
                                    cx,
                                );
                            });
                        }
                    },
                )
            };
            if pull.is_abandoned() || pull.is_completed() {
                actions = actions.child(primary);
            } else {
                let this_menu = this.clone();
                actions = actions.child(split_button(
                    "azure-primary",
                    primary,
                    true,
                    move |_, _, cx| {
                        this_menu.update(cx, |view, cx| {
                            view.primary_menu = !view.primary_menu;
                            view.review_menu = false;
                            cx.notify();
                        });
                    },
                ));
            }
            title_row = title_row.child(actions);
        }

        title_row = title_row.child(
            div()
                .id("azure-open")
                .debug_selector(|| "azure-open".into())
                .a11y_button("Open on Azure DevOps")
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
        );

        div()
            .flex_shrink_0()
            .px(px(shell::DETAIL_PADDING))
            .pt_4()
            .pb(px(14.))
            .flex()
            .flex_col()
            .child(title_row)
            .child(subtitle(open, pull, repo_name))
            .into_any_element()
    }

    // ---- tabs -------------------------------------------------------------

    fn tab_strip(
        &self,
        pull: Option<&AzurePull>,
        open: &crate::store::OpenAzurePull,
        this: &Entity<Self>,
    ) -> AnyElement {
        let files = open.files.as_ref().map_or(0, |files| files.len());
        let commits = open.commits.as_ref().map_or(0, |commits| commits.len());
        let mut strip = div()
            .debug_selector(|| "azure-pull-tabs".into())
            .flex_shrink_0()
            .px(px(shell::DETAIL_PADDING))
            .flex()
            .flex_row()
            .border_b_1()
            .border_color(theme::hairline());
        for tab in AzureTab::ALL {
            let count = match tab {
                AzureTab::Files => files,
                AzureTab::Commits => commits,
                AzureTab::Overview => 0,
            };
            let selector = format!("azure-pull-tab-{}", tab.key());
            let this = this.clone();
            strip = strip.child(
                shell::detail_tab(selector.clone(), tab.label(), self.tab == tab)
                    .debug_selector(move || selector.clone())
                    .on_click(move |_, _, cx| {
                        this.update(cx, |view, cx| {
                            view.tab = tab;
                            view.primary_menu = false;
                            view.review_menu = false;
                            cx.notify();
                        });
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
        let _ = pull;
        strip.into_any_element()
    }

    // ---- Overview ---------------------------------------------------------

    fn overview(
        &self,
        pull: Option<&AzurePull>,
        policies: &[AzurePolicy],
        threads: &[AzureThread],
        open: &crate::store::OpenAzurePull,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let body = div()
            .id("azure-pull-overview")
            .debug_selector(|| "azure-pull-overview".into())
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p(px(shell::DETAIL_PADDING));
        let Some(pull) = pull else {
            return body
                .child(diff_controls::state(if open.failed {
                    "Azure DevOps did not answer."
                } else {
                    "Reading the pull request…"
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
            .child(policies_card(pull, policies))
            .child(description_card(pull))
            .child(threads_card(threads, cx));
        let side = div()
            .flex_grow(1.)
            .flex_shrink(1.)
            .flex_basis(px(0.))
            .min_w(px(240.))
            .flex()
            .flex_col()
            .gap_4()
            .child(reviewers_card(pull));
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

    // ---- Files ------------------------------------------------------------

    fn files_tab(
        &self,
        open: &crate::store::OpenAzurePull,
        window: &mut Window,
        cx: &mut Context<Self>,
        this: &Entity<Self>,
        show_tooltips: bool,
    ) -> AnyElement {
        let files = open.files.clone().unwrap_or_default();
        if files.is_empty() {
            return diff_controls::state(if open.files.is_none() {
                "Reading the changed files…"
            } else {
                "No files changed"
            });
        }
        let ix = self.file_ix.min(files.len() - 1);
        let file = &files[ix];

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
            .id("azure-pull-files")
            .debug_selector(|| "azure-pull-files".into())
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .py_1();
        for (index, entry) in files.iter().enumerate() {
            rows = rows.child(file_row(index, entry, index == ix, this));
        }
        rail = rail.child(rows);
        let narrow = self.column_width < STACKED_COLUMN;
        let rail = if narrow {
            div()
                .debug_selector(|| "azure-pull-files-band".into())
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

        let loaded = open.diff.as_ref().filter(|(path, _)| path == &file.path);
        let patch = loaded
            .and_then(|(_, patch)| patch.clone())
            .unwrap_or_default();
        let has_patch = patch.contains("\n@@");
        let split = self.diff_view == DiffViewMode::Split;
        let body = if has_patch {
            div()
                .id("azure-pull-diff-scroll")
                .debug_selector(|| "azure-pull-diff-scroll".into())
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .child(diff_controls::attach_filling(
                    gpui::ElementId::Name("azure-pull-diff-menu".into()),
                    render_diff_with_scroll(
                        &patch,
                        Some(&file.path),
                        warpui_kit_diff::DiffOptions {
                            mode: diff_controls::kit_mode(self.diff_view),
                            wrap: self.diff_wrap,
                        },
                        &self.diff_scroll,
                        theme::font_mono_meta(),
                        window,
                        cx,
                    ),
                    warpui_kit_core::text_menu::focus(cx),
                ))
                .into_any_element()
        } else {
            diff_controls::state(if loaded.is_none() {
                "Reading the file…"
            } else {
                "No textual changes"
            })
        };

        let this_wrap = this.clone();
        let this_view = this.clone();
        let store = self.store.clone();
        let controls = diff_controls::wrap_toggle(
            "azure-pull",
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
            "azure-pull",
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
        let layout = if narrow {
            div().flex_1().min_h_0().min_w_0().flex().flex_col()
        } else {
            div().flex_1().min_h_0().min_w_0().flex().flex_row()
        };
        layout
            .child(rail)
            .child(
                div()
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
                    .children((has_patch && !self.diff_wrap).then(|| {
                        diff_controls::scrollbars("azure-pull", &self.diff_scroll, split)
                    })),
            )
            .into_any_element()
    }

    // ---- Commits ----------------------------------------------------------

    fn commits_tab(
        &self,
        open: &crate::store::OpenAzurePull,
        _cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(commits) = open.commits.as_ref() else {
            return diff_controls::state("Reading the commits…");
        };
        if commits.is_empty() {
            return diff_controls::state("No commits yet");
        }
        let mut list = div()
            .id("azure-pull-commits")
            .debug_selector(|| "azure-pull-commits".into())
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px(px(shell::DETAIL_PADDING))
            .py(px(18.));
        for commit in commits {
            let age = commit
                .when
                .as_deref()
                .and_then(warp_station_core::rel_time::format_rfc3339)
                .unwrap_or_default();
            let who = commit.author.clone().unwrap_or_else(|| "Unknown".into());
            list = list.child(
                div()
                    .debug_selector(|| "azure-pull-commit".into())
                    .flex()
                    .items_start()
                    .gap_3()
                    .py_2()
                    .border_b_1()
                    .border_color(theme::row_line())
                    .child(
                        div()
                            .flex_shrink_0()
                            .font_family(theme::code_font())
                            .text_size(px(theme::font_mono_meta()))
                            .text_color(theme::text_muted())
                            .child(commit.short_id()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap_0p5()
                            .child(
                                div()
                                    .text_size(px(theme::font_body()))
                                    .text_color(theme::text())
                                    .child(commit.message.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(theme::font_meta()))
                                    .text_color(theme::text_muted())
                                    .child(if age.is_empty() {
                                        who
                                    } else {
                                        format!("{who} · {age}")
                                    }),
                            ),
                    ),
            );
        }
        list.into_any_element()
    }

    // ---- menus ------------------------------------------------------------

    fn primary_menu_card(
        &self,
        pull: Option<&AzurePull>,
        policies: &[AzurePolicy],
        acting: bool,
        this: &Entity<Self>,
    ) -> AnyElement {
        let mut menu = menu_shell("azure-primary-menu");
        if let Some(pull) = pull {
            let active = pull.is_active();
            if active && pull.ready_to_complete(policies) && !pull.auto_complete {
                let this_click = this.clone();
                let store_click = self.store.clone();
                menu = menu.child(menu_item(
                    "azure-menu-complete",
                    "icons/icon-check.svg",
                    "Complete",
                    !acting,
                    move |_, _, cx| {
                        store_click.update(cx, |store, cx| {
                            store.act_on_azure_pull(AzurePullAction::Complete, cx);
                        });
                        this_click.update(cx, |view, cx| {
                            view.primary_menu = false;
                            cx.notify();
                        });
                    },
                ));
            }
            if active {
                let reviewer = pull.me.clone();
                let this_click = this.clone();
                let store_click = self.store.clone();
                let armed = pull.auto_complete;
                menu = menu.child(menu_item(
                    "azure-menu-autocomplete",
                    "icons/icon-clock.svg",
                    if armed {
                        "Cancel auto-complete"
                    } else {
                        "Set auto-complete"
                    },
                    !acting && reviewer.is_some(),
                    move |_, _, cx| {
                        let action = if armed {
                            AzurePullAction::CancelAutoComplete
                        } else if let Some(reviewer) = reviewer.clone() {
                            AzurePullAction::SetAutoComplete { reviewer }
                        } else {
                            return;
                        };
                        store_click.update(cx, |store, cx| {
                            store.act_on_azure_pull(action, cx);
                        });
                        this_click.update(cx, |view, cx| {
                            view.primary_menu = false;
                            cx.notify();
                        });
                    },
                ));
                let this_click = this.clone();
                let store_click = self.store.clone();
                menu = menu.child(menu_divider()).child(menu_item(
                    "azure-menu-abandon",
                    "icons/icon-trash.svg",
                    "Abandon",
                    !acting,
                    move |_, _, cx| {
                        store_click.update(cx, |store, cx| store.ask_abandon_azure(cx));
                        this_click.update(cx, |view, cx| {
                            view.primary_menu = false;
                            cx.notify();
                        });
                    },
                ));
            }
        }
        gpui::deferred(menu).into_any_element()
    }

    fn review_menu_card(
        &self,
        pull: Option<&AzurePull>,
        acting: bool,
        this: &Entity<Self>,
    ) -> AnyElement {
        let mut menu = menu_shell("azure-review-menu");
        if let Some(pull) = pull {
            let reviewer = pull.my_reviewer().map(|r| r.id.clone());
            let vote = |vote: i64| {
                let this = this.clone();
                let store = self.store.clone();
                let reviewer = reviewer.clone();
                move |_: &gpui::ClickEvent, _: &mut Window, cx: &mut gpui::App| {
                    if let Some(reviewer) = reviewer.clone() {
                        store.update(cx, |store, cx| {
                            store.act_on_azure_pull(AzurePullAction::Vote { reviewer, vote }, cx);
                        });
                    }
                    this.update(cx, |view, cx| {
                        view.review_menu = false;
                        cx.notify();
                    });
                }
            };
            menu = menu
                .child(menu_item(
                    "azure-menu-approve",
                    "icons/icon-check.svg",
                    "Approve",
                    !acting && reviewer.is_some(),
                    vote(10),
                ))
                .child(menu_item(
                    "azure-menu-decline",
                    "icons/icon-close.svg",
                    "Decline",
                    !acting && reviewer.is_some(),
                    vote(-10),
                ));
        }
        gpui::deferred(menu).into_any_element()
    }
}

/// The state pill: Active, Draft, Completed or Abandoned.
fn state_pill(pull: Option<&AzurePull>) -> AnyElement {
    let (label, color) = match pull {
        Some(pull) if pull.is_completed() => ("Completed", theme::PR),
        Some(pull) if pull.is_abandoned() => ("Abandoned", theme::text_muted()),
        Some(pull) if pull.is_draft => ("Draft", theme::text_muted()),
        _ => ("Active", theme::PR),
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
            if matches!(label, "Completed") {
                "icons/icon-check.svg"
            } else if matches!(label, "Abandoned") {
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

fn subtitle(
    open: &crate::store::OpenAzurePull,
    pull: Option<&AzurePull>,
    repo_name: &str,
) -> AnyElement {
    let item = &open.item;
    let author = pull
        .and_then(|pull| pull.author.clone())
        .or_else(|| item.author.clone())
        .unwrap_or_else(|| "Unknown".into());
    let source = pull.map(AzurePull::source_branch).unwrap_or_default();
    let target = pull.map(AzurePull::target_branch).unwrap_or_default();
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
        .child(format!("{repo_name} !{}", item.number));
    if !source.is_empty() {
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
                        .child(format!("{source} → {target}")),
                ),
        );
    }
    row.children(
        pull.and_then(|pull| pull.merge_status.as_deref())
            .and_then(|status| {
                (status == "conflicts").then(|| {
                    div()
                        .flex()
                        .items_center()
                        .gap_1p5()
                        .text_color(theme::DANGER)
                        .child(icon(
                            "icons/icon-close.svg",
                            px(theme::icon_tiny()),
                            theme::DANGER,
                        ))
                        .child("Merge conflicts")
                        .into_any_element()
                })
            }),
    )
    .child(source_tag("Azure"))
    .into_any_element()
}

/// The 44 status strip: approvals, the open threads and the age.
fn status_strip(
    pull: Option<&AzurePull>,
    threads: &[AzureThread],
    open: &crate::store::OpenAzurePull,
) -> AnyElement {
    let strip = div()
        .debug_selector(|| "azure-pull-status".into())
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
                "Azure DevOps did not answer."
            } else {
                "Reading the pull request…"
            })
            .into_any_element();
    };
    let open_threads = threads.iter().filter(|thread| thread.is_open()).count();
    let mut facts = div()
        .flex()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap(px(18.));
    if pull.rejected() > 0 {
        facts = facts.child(
            div()
                .flex()
                .items_center()
                .gap_1p5()
                .text_color(theme::DANGER)
                .child(icon(
                    "icons/icon-close.svg",
                    px(theme::icon_tiny()),
                    theme::DANGER,
                ))
                .child("Rejected"),
        );
    }
    facts = facts.child(match pull.approved() {
        0 => "No approvals".to_string(),
        1 => "1 approval".to_string(),
        n => format!("{n} approvals"),
    });
    facts = facts.child(format!(
        "{open_threads} open thread{}",
        plural(open_threads)
    ));
    facts = facts.children(
        pull.closed
            .as_deref()
            .or(pull.created.as_deref())
            .and_then(warp_station_core::rel_time::format_rfc3339)
            .map(|when| {
                if pull.closed.is_some() {
                    format!("Closed {when}")
                } else {
                    format!("Created {when}")
                }
            }),
    );

    let mut right = div().flex().flex_row().items_center().gap_3();
    if pull.auto_complete {
        right = right.child(
            div()
                .flex()
                .items_center()
                .gap_1p5()
                .text_color(theme::BEHIND)
                .child(icon(
                    "icons/icon-clock.svg",
                    px(theme::icon_tiny()),
                    theme::BEHIND,
                ))
                .child("Auto-complete set"),
        );
    }
    if !pull.reviewers.is_empty() {
        let mut stack = div()
            .debug_selector(|| "azure-pull-reviewers".into())
            .flex()
            .flex_row()
            .items_center();
        for (ix, reviewer) in pull.reviewers.iter().take(3).enumerate() {
            let mut chip = div().flex_shrink_0();
            if ix > 0 {
                chip = chip.ml(px(-6.));
            }
            stack = stack.child(chip.child(ringed_avatar(&reviewer.name, 22.)));
        }
        if pull.reviewers.len() > 3 {
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
                    .child(format!("+{}", pull.reviewers.len() - 3)),
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

/// The policies Azure evaluates: a required reviewer, a build, the merge
/// itself. A rejected or running policy is what stands in the way.
fn policies_card(pull: &AzurePull, policies: &[AzurePolicy]) -> AnyElement {
    let blocked = policies.iter().any(AzurePolicy::is_blocking) || pull.has_conflicts();
    let mut rows = div().flex().flex_col();
    if policies.is_empty() {
        rows = rows.child(policy_row(
            "icons/icon-clock.svg",
            theme::text_muted(),
            "No policies required".to_string(),
            "The repository does not gate this pull request.".into(),
        ));
    }
    for policy in policies {
        let (path, color) = if policy.status == "rejected" {
            ("icons/icon-close.svg", theme::DANGER)
        } else if policy.is_running() {
            ("icons/icon-clock.svg", theme::BEHIND)
        } else if policy.status == "approved" {
            ("icons/icon-check.svg", theme::SUCCESS_BRIGHT)
        } else {
            ("icons/icon-clock.svg", theme::text_muted())
        };
        rows = rows.child(policy_row(
            path,
            color,
            policy.name.clone(),
            format!("{} · {}", policy.kind, policy.word()),
        ));
    }
    rows = rows.child(if pull.has_conflicts() {
        policy_row(
            "icons/icon-close.svg",
            theme::DANGER,
            "Merge conflicts must be resolved".to_string(),
            "The target branch moved into this one.".into(),
        )
    } else {
        policy_row(
            "icons/icon-check.svg",
            theme::SUCCESS_BRIGHT,
            "No merge conflicts".to_string(),
            String::new(),
        )
    });
    card(
        "azure-pull-policies",
        "Policies",
        if pull.is_completed() {
            "Completed"
        } else if pull.is_abandoned() {
            "Abandoned"
        } else if blocked || pull.is_draft {
            "Blocked"
        } else {
            "Ready"
        },
        rows.into_any_element(),
    )
}

/// The pull request's own words, as markdown.
fn description_card(pull: &AzurePull) -> AnyElement {
    let body = match pull.description.as_deref() {
        Some(text) if !text.trim().is_empty() => div()
            .p_4()
            .text_size(px(theme::font_body()))
            .text_color(theme::text())
            .child(
                TextView::markdown(
                    gpui::ElementId::Name("azure-pull-description-body".into()),
                    SharedString::from(text.to_string()),
                )
                .selectable(true),
            )
            .into_any_element(),
        _ => div()
            .p_4()
            .text_size(px(theme::font_meta()))
            .text_color(theme::text_muted())
            .child("No description.")
            .into_any_element(),
    };
    card("azure-pull-description", "Description", "", body)
}

fn reviewers_card(pull: &AzurePull) -> AnyElement {
    let mut body = div().px_4().py_2().flex().flex_col();
    if pull.reviewers.is_empty() {
        body = body.child(
            div()
                .py_2()
                .text_size(px(theme::font_meta()))
                .text_color(theme::text_muted())
                .child("No reviewers yet."),
        );
    }
    let required: Vec<_> = pull.reviewers.iter().filter(|r| r.required).collect();
    let optional: Vec<_> = pull.reviewers.iter().filter(|r| !r.required).collect();
    for (label, rows) in [("Required", required), ("Optional", optional)] {
        if rows.is_empty() {
            continue;
        }
        let mut group = div()
            .py_1()
            .flex()
            .flex_col()
            .border_t_1()
            .border_color(theme::row_line())
            .child(
                div()
                    .py(px(2.))
                    .text_size(px(theme::font_meta()))
                    .text_color(theme::text_muted())
                    .child(label),
            );
        for reviewer in rows {
            let color = if reviewer.is_rejected() {
                theme::DANGER
            } else if reviewer.is_approved() {
                theme::SUCCESS_BRIGHT
            } else {
                theme::text_muted()
            };
            let path = if reviewer.is_rejected() {
                "icons/icon-close.svg"
            } else if reviewer.is_approved() {
                "icons/icon-check.svg"
            } else {
                "icons/icon-clock.svg"
            };
            group = group.child(
                div()
                    .h(px(44.))
                    .flex()
                    .items_center()
                    .gap_2p5()
                    .child(avatar(&reviewer.name, 24.))
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
                                    .child(reviewer.name.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(theme::font_meta()))
                                    .text_color(theme::text_muted())
                                    .child(reviewer.vote_word()),
                            ),
                    )
                    .child(icon(path, px(theme::icon_small()), color)),
            );
        }
        body = body.child(group);
    }
    div()
        .debug_selector(|| "azure-pull-reviewers-card".into())
        .rounded(px(8.))
        .border_1()
        .border_color(theme::hairline())
        .bg(theme::surface())
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
                        .child(format!(
                            "{} required",
                            pull.reviewers.iter().filter(|r| r.required).count()
                        )),
                ),
        )
        .child(body)
        .into_any_element()
}

/// The comment threads, with Azure's resolution state on each.
fn threads_card(threads: &[AzureThread], cx: &mut Context<AzurePullView>) -> AnyElement {
    let mut body = div().flex().flex_col().gap_3();
    if threads.is_empty() {
        body = body.child(
            div()
                .text_size(px(theme::font_meta()))
                .text_color(theme::text_muted())
                .child("No comments yet."),
        );
    }
    for (ix, thread) in threads.iter().enumerate() {
        for (comment_ix, comment) in thread.comments.iter().enumerate() {
            let when = comment
                .when
                .as_deref()
                .and_then(warp_station_core::rel_time::format_rfc3339)
                .map(|when| format!("commented {when}"))
                .unwrap_or_else(|| "commented".to_string());
            let verb = if comment_ix == 0 && !thread.is_open() {
                format!("{when} · thread {}", thread.status)
            } else {
                when
            };
            body = body.child(comment_card(
                ix * 10 + comment_ix,
                &comment.author,
                verb,
                &comment.text,
                warpui_kit_core::text_menu::focus(cx),
            ));
        }
    }
    div()
        .debug_selector(|| "azure-pull-threads".into())
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
                        .child(format!(
                            "{}",
                            threads
                                .iter()
                                .map(|thread| thread.comments.len())
                                .sum::<usize>()
                        )),
                ),
        )
        .child(body)
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
        .bg(theme::surface())
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

/// One row of the changed-file rail.
fn file_row(
    index: usize,
    file: &AzureFile,
    selected: bool,
    this: &Entity<AzurePullView>,
) -> AnyElement {
    let this = this.clone();
    let path = file.path.clone();
    let selector = format!("azure-pull-file-{index}");
    let (dir, name) = split_path(&path);
    div()
        .id(SharedString::from(selector.clone()))
        .debug_selector(move || selector.clone())
        .a11y_button(path.clone())
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
            let path = path.clone();
            this.update(cx, |view, cx| {
                view.file_ix = index;
                view.store
                    .update(cx, |store, cx| store.load_azure_pull_diff(&path, cx));
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
                .text_color(diff_controls::status_color(file.letter()))
                .child(file.letter()),
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
                        .flex_shrink(1.)
                        .min_w(px(0.))
                        .text_color(theme::text_muted())
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(dir),
                )
                .child(div().flex_shrink_0().child(name)),
        )
        .into_any_element()
}

fn split_path(path: &str) -> (String, String) {
    match path.rsplit_once('/') {
        Some((dir, name)) => (format!("{dir}/"), name.to_string()),
        None => (String::new(), path.to_string()),
    }
}

fn ringed_avatar(name: &str, size: f32) -> AnyElement {
    div()
        .flex_shrink_0()
        .rounded_full()
        .border_2()
        .border_color(theme::surface())
        .child(avatar(name, size))
        .into_any_element()
}
