//! The New pull request form: the Pull requests tab's detail column while a
//! pull request is being written. It is not an overlay — the work list stays
//! beside it on a wide pane, and on a narrow one the form takes the pane with
//! Back returning to the list. The header and the Create footer stay fixed; the
//! form body scrolls.
//!
//! The form keeps to Title, Description and an optional Reviewer. Files and
//! Commits are previews of what the source branch would bring, read from the
//! repository itself — no network, no push needed to see the shape.

use crate::store::Store;
use crate::theme;
use crate::ui::pull_request::{
    Shape, plural, primary_button_in, short_sha, source_tag, split_button,
};
use crate::ui::{A11y, icon};
use gpui::prelude::*;
use gpui::{AnyElement, Context, Entity, FontWeight, SharedString, Window, div, px};
use gpui_kit::component::input::{InputEvent, InputState, TextareaState};
use warp_station_core::model::ComparePreview;
use warpui_kit_core::{field, motion, shell};

/// The form body's measure: one comfortable column of fields.
const FORM_WIDTH: f32 = 760.;

/// The form's own tabs. They are the pull request's three, before it exists.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NewPullTab {
    #[default]
    Overview,
    Files,
    Commits,
}

impl NewPullTab {
    pub const ALL: [NewPullTab; 3] = [NewPullTab::Overview, NewPullTab::Files, NewPullTab::Commits];

    pub fn label(self) -> &'static str {
        match self {
            NewPullTab::Overview => "Overview",
            NewPullTab::Files => "Files",
            NewPullTab::Commits => "Commits",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            NewPullTab::Overview => "overview",
            NewPullTab::Files => "files",
            NewPullTab::Commits => "commits",
        }
    }
}

pub struct NewPullView {
    store: Entity<Store>,
    tab: NewPullTab,
    title: Entity<InputState>,
    description: Entity<TextareaState>,
    reviewer: Entity<InputState>,
    /// Source and base are free text, as the design draws them.
    head: Entity<InputState>,
    base: Entity<InputState>,
    /// The branches last handed to the store, so typing is not a loop.
    branches: (String, String),
    /// The Create split's menu.
    primary_menu: bool,
    /// The form is the whole detail pane: the workbench's list is not beside it.
    pub stacked: bool,
}

impl NewPullView {
    pub fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        let title = cx.new(|cx| InputState::new(window, cx).placeholder("What changed?"));
        let description =
            cx.new(|cx| TextareaState::new(window, cx).placeholder("Describe the change…"));
        let reviewer =
            cx.new(|cx| InputState::new(window, cx).placeholder("Search people or teams"));
        let head = cx.new(|cx| InputState::new(window, cx).placeholder("branch"));
        let base = cx.new(|cx| InputState::new(window, cx).placeholder("branch"));
        // the preview follows the branches as they are typed
        cx.subscribe(&head, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.sync_branches(cx);
            }
        })
        .detach();
        cx.subscribe(&base, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.sync_branches(cx);
            }
        })
        .detach();
        Self {
            store,
            tab: NewPullTab::Overview,
            title,
            description,
            reviewer,
            head,
            base,
            branches: (String::new(), String::new()),
            primary_menu: false,
            stacked: false,
        }
    }

    /// Pushes what the two branch fields say into the store, which owns the
    /// compare request.
    fn sync_branches(&mut self, cx: &mut Context<Self>) {
        let head = self.head.read(cx).value().trim().to_string();
        let base = self.base.read(cx).value().trim().to_string();
        if (head.clone(), base.clone()) == self.branches {
            return;
        }
        self.branches = (head.clone(), base.clone());
        self.store
            .update(cx, |store, cx| store.set_new_pull_branches(head, base, cx));
        cx.notify();
    }

    /// Seeds the branch fields from the store the first time the form draws,
    /// and again when a different form is opened over a different repository.
    fn seed_branches(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(form) = self.store.read(cx).new_pull.clone() else {
            return;
        };
        if (form.head.clone(), form.base.clone()) == self.branches {
            return;
        }
        self.branches = (form.head.clone(), form.base.clone());
        self.head
            .update(cx, |input, cx| input.set_value(&form.head, window, cx));
        self.base
            .update(cx, |input, cx| input.set_value(&form.base, window, cx));
    }

    fn swap(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (head, base) = (self.base.read(cx).value(), self.head.read(cx).value());
        self.head
            .update(cx, |input, cx| input.set_value(&head, window, cx));
        self.base
            .update(cx, |input, cx| input.set_value(&base, window, cx));
        self.sync_branches(cx);
    }

    /// Create, or Create as draft. The form always answers on Overview, where
    /// the error is visible.
    fn create(&mut self, draft: bool, cx: &mut Context<Self>) {
        let title = self.title.read(cx).value().to_string();
        let body = self.description.read(cx).value().to_string();
        let reviewer = self.reviewer.read(cx).value().to_string();
        self.primary_menu = false;
        self.tab = NewPullTab::Overview;
        self.store.update(cx, |store, cx| {
            store.create_pull(title, body, reviewer, draft, cx)
        });
        cx.notify();
    }

    pub fn dismiss(&mut self, cx: &mut Context<Self>) -> bool {
        if self.primary_menu {
            self.primary_menu = false;
            cx.notify();
            return true;
        }
        false
    }
}

impl Render for NewPullView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        let this = cx.entity();
        self.seed_branches(window, cx);
        self.store.update(cx, |store, _| store.load_compare());

        let store = self.store.read(cx);
        let Some(form) = store.new_pull.clone() else {
            return div().into_any_element();
        };
        let pair = (form.head.trim().to_string(), form.base.trim().to_string());
        let pair_ready = !pair.0.is_empty() && !pair.1.is_empty() && pair.0 != pair.1;
        let preview = if !pair_ready {
            PreviewState::NoPair
        } else if !store.compare_answered() {
            PreviewState::Waiting
        } else {
            match store.compare_preview() {
                Some(preview) => PreviewState::Ready(preview),
                None => PreviewState::Nothing,
            }
        };
        let repo_name = store
            .selected_entry()
            .map(|repo| repo.name.clone())
            .unwrap_or_default();
        let creating = form.creating;
        let error = form.error.clone();
        let files = preview.counts().0;
        let commits = preview.counts().1;

        let body = match self.tab {
            NewPullTab::Overview => self.overview(&error, window, cx),
            NewPullTab::Files => preview_body(
                "new-pull-files",
                &preview,
                "file",
                "No files changed",
                "This branch has no changes against the base yet.",
                files_body,
            ),
            NewPullTab::Commits => preview_body(
                "new-pull-commits",
                &preview,
                "commit",
                "No commits yet",
                "Commits on this branch will show here.",
                commits_body,
            ),
        };

        let pane = div()
            .debug_selector(|| "new-pull".into())
            .relative()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(theme::panel())
            .child(self.head_row(&repo_name, &this))
            .child(self.branch_row(window, cx))
            .child(self.tab_strip(files, commits, &this))
            .child(body)
            .child(self.footer(creating, &this));
        pane.children(self.primary_menu.then(|| self.create_menu(creating, &this)))
            .into_any_element()
    }
}

impl NewPullView {
    // ---- head -------------------------------------------------------------

    fn head_row(&self, repo_name: &str, this: &Entity<Self>) -> AnyElement {
        let mut title_row = div()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .gap_2()
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
                    .child("New pull request"),
            )
            .child(source_tag("GitHub"));
        if self.stacked {
            let store = self.store.clone();
            let this = this.clone();
            title_row = title_row.child(
                div()
                    .id("new-pull-back")
                    .debug_selector(|| "new-pull-back".into())
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
                        this.update(cx, |this, cx| {
                            this.primary_menu = false;
                            cx.notify();
                        });
                        store.update(cx, |store, cx| store.close_new_pull(cx));
                    })
                    .child(icon(
                        "icons/icon-arrow-left.svg",
                        px(theme::icon_small()),
                        theme::text_dim(),
                    )),
            );
        }

        div()
            .flex_shrink_0()
            .px(px(shell::DETAIL_PADDING))
            .pt_4()
            .pb(px(14.))
            .flex()
            .flex_col()
            .child(title_row)
            .child(
                div()
                    .mt_2()
                    .text_size(px(theme::font_meta()))
                    .text_color(theme::text_dim())
                    .child(format!("Open a pull request in {repo_name}.")),
            )
            .into_any_element()
    }

    // ---- branches ---------------------------------------------------------

    fn branch_row(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let this = cx.entity();
        let swap = this.clone();
        div()
            .debug_selector(|| "new-pull-branches".into())
            .flex_shrink_0()
            .px(px(shell::DETAIL_PADDING))
            .pb(px(16.))
            .flex()
            .flex_row()
            .items_end()
            .gap_2()
            .child(labeled_field(
                "Source",
                field::mono("new-pull-head", &self.head, window, cx),
            ))
            .child(
                div()
                    .id("new-pull-swap")
                    .debug_selector(|| "new-pull-swap".into())
                    .a11y_button("Swap source and base")
                    .cursor_pointer()
                    .flex_shrink_0()
                    .size(px(shell::CONTROL_HEIGHT))
                    .rounded(px(6.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .mb(px(1.))
                    .hover(|s| s.bg(theme::raised()))
                    .on_click(move |_, window, cx| {
                        swap.update(cx, |view, cx| view.swap(window, cx));
                    })
                    .child(icon(
                        "icons/icon-refresh.svg",
                        px(theme::icon_small()),
                        theme::text_muted(),
                    )),
            )
            .child(labeled_field(
                "Base",
                field::mono("new-pull-base", &self.base, window, cx),
            ))
            .into_any_element()
    }

    // ---- tabs -------------------------------------------------------------

    fn tab_strip(&self, files: usize, commits: usize, this: &Entity<Self>) -> AnyElement {
        let mut strip = div()
            .debug_selector(|| "new-pull-tabs".into())
            .flex_shrink_0()
            .px(px(shell::DETAIL_PADDING))
            .flex()
            .flex_row()
            .border_t_1()
            .border_b_1()
            .border_color(theme::hairline());
        for tab in NewPullTab::ALL {
            let count = match tab {
                NewPullTab::Files => files,
                NewPullTab::Commits => commits,
                NewPullTab::Overview => 0,
            };
            let selector = format!("new-pull-tab-{}", tab.key());
            let this = this.clone();
            strip = strip.child(
                shell::detail_tab(selector.clone(), tab.label(), self.tab == tab)
                    .debug_selector(move || selector.clone())
                    .on_click(move |_, _, cx| {
                        this.update(cx, |view, cx| {
                            view.tab = tab;
                            view.primary_menu = false;
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
        strip.into_any_element()
    }

    // ---- Overview ---------------------------------------------------------

    fn overview(
        &self,
        error: &Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let fields = div()
            .w_full()
            .max_w(px(FORM_WIDTH))
            .flex()
            .flex_col()
            .gap(px(14.))
            .children(error.as_ref().map(|error| {
                div()
                    .debug_selector(|| "new-pull-error".into())
                    .rounded(px(6.))
                    .border_1()
                    .border_color(theme::DANGER)
                    .bg(theme::tint(theme::DANGER))
                    .px_3()
                    .py_2()
                    .text_size(px(theme::font_body()))
                    .text_color(theme::DANGER)
                    .child(error.clone())
            }))
            .child(labeled_field(
                "Title (required)",
                field::text("new-pull-title", &self.title, window, cx),
            ))
            .child(labeled_field(
                "Description",
                field::area("new-pull-description", &self.description, window, cx),
            ))
            .child(labeled_field(
                "Reviewer (optional)",
                field::text("new-pull-reviewer", &self.reviewer, window, cx),
            ));
        div()
            .id("new-pull-overview")
            .debug_selector(|| "new-pull-overview".into())
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px(px(shell::DETAIL_PADDING))
            .py(px(18.))
            .child(fields)
            .into_any_element()
    }

    // ---- footer -----------------------------------------------------------

    fn footer(&self, creating: bool, this: &Entity<Self>) -> AnyElement {
        let cancel = {
            let store = self.store.clone();
            let this = this.clone();
            move |_: &gpui::ClickEvent, _: &mut Window, cx: &mut gpui::App| {
                this.update(cx, |this, cx| {
                    this.primary_menu = false;
                    cx.notify();
                });
                store.update(cx, |store, cx| store.close_new_pull(cx));
            }
        };
        let label = if creating {
            "Creating…"
        } else {
            "Create pull request"
        };
        let create = {
            let this = this.clone();
            move |_: &gpui::ClickEvent, _: &mut Window, cx: &mut gpui::App| {
                this.update(cx, |view, cx| view.create(false, cx));
            }
        };
        let menu = {
            let this = this.clone();
            move |_: &gpui::ClickEvent, _: &mut Window, cx: &mut gpui::App| {
                this.update(cx, |view, cx| {
                    view.primary_menu = !view.primary_menu;
                    cx.notify();
                });
            }
        };
        let primary = if creating {
            div()
                .id("new-pull-create")
                .debug_selector(|| "new-pull-create".into())
                .a11y_button(label)
                .flex_shrink_0()
                .h(px(shell::CONTROL_HEIGHT))
                .px_3()
                .rounded_l(px(6.))
                .bg(theme::accent())
                .opacity(0.6)
                .flex()
                .items_center()
                .gap_2()
                .text_size(px(theme::font_control()))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::on_accent())
                .child(motion::spinner(
                    "new-pull-spinner",
                    "icons/icon-spinner.svg",
                    px(theme::icon_small()),
                    theme::on_accent(),
                ))
                .child(label)
                .into_any_element()
        } else {
            primary_button_in("new-pull-create", label, true, Shape::Left, create)
        };

        div()
            .debug_selector(|| "new-pull-foot".into())
            .flex_shrink_0()
            .px(px(shell::DETAIL_PADDING))
            .py(px(14.))
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .border_t_1()
            .border_color(theme::hairline())
            .bg(theme::panel())
            .child(div().flex_1())
            .child(crate::ui::neutral_button(
                "new-pull-cancel",
                "Cancel",
                !creating,
                cancel,
            ))
            .child(split_button("new-pull-primary", primary, true, menu))
            .into_any_element()
    }

    fn create_menu(&self, creating: bool, this: &Entity<Self>) -> AnyElement {
        let mut menu = crate::ui::pull_request::menu_shell("new-pull-menu");
        let create = {
            let this = this.clone();
            move |_: &gpui::ClickEvent, _: &mut Window, cx: &mut gpui::App| {
                this.update(cx, |view, cx| view.create(false, cx));
            }
        };
        let draft = {
            let this = this.clone();
            move |_: &gpui::ClickEvent, _: &mut Window, cx: &mut gpui::App| {
                this.update(cx, |view, cx| view.create(true, cx));
            }
        };
        menu = menu
            .child(crate::ui::pull_request::menu_item(
                "new-pull-menu-create",
                "icons/icon-git-pull-request.svg",
                "Create pull request",
                !creating,
                create,
            ))
            .child(crate::ui::pull_request::menu_item(
                "new-pull-menu-draft",
                "icons/icon-text.svg",
                "Create as draft",
                !creating,
                draft,
            ));
        gpui::deferred(menu).into_any_element()
    }
}

/// A labelled box: the design's `label > span` pair above a field.
fn labeled_field(label: &'static str, field: gpui::Stateful<gpui::Div>) -> AnyElement {
    div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(5.))
        .child(
            div()
                .text_size(px(theme::font_meta()))
                .text_color(theme::text_dim())
                .child(label),
        )
        .child(field)
        .into_any_element()
}

/// What the preview can say: the branch pair is not a pair yet, the compare is
/// still out, it found nothing, or it has rows.
enum PreviewState<'a> {
    NoPair,
    Waiting,
    Nothing,
    Ready(&'a ComparePreview),
}

impl PreviewState<'_> {
    /// The counts the tab labels carry, when there are any.
    fn counts(&self) -> (usize, usize) {
        match self {
            PreviewState::Ready(preview) => (preview.files.len(), preview.commits.len()),
            _ => (0, 0),
        }
    }
}

/// The Files and Commits tabs share one shape: a summary line, then either the
/// rows or an empty state that says which one it is.
fn preview_body(
    selector: &'static str,
    state: &PreviewState<'_>,
    noun: &'static str,
    empty_title: &'static str,
    empty_detail: &'static str,
    rows: impl Fn(&ComparePreview) -> AnyElement,
) -> AnyElement {
    let preview = match state {
        PreviewState::Ready(preview) => preview,
        PreviewState::NoPair => {
            return plain_state(
                "Pick two branches",
                "The preview compares Source against Base.",
            );
        }
        PreviewState::Waiting => return plain_state("Reading the branches…", ""),
        PreviewState::Nothing => {
            return plain_state("Nothing to compare", "These branches bring no changes.");
        }
    };
    if preview.files.is_empty() && preview.commits.is_empty() {
        return plain_state(empty_title, empty_detail);
    }
    let own = if noun == "file" {
        preview.files.len()
    } else {
        preview.commits.len()
    };
    if own == 0 {
        return plain_state(empty_title, empty_detail);
    }
    let body = rows(preview);
    div()
        .id(SharedString::from(selector))
        .debug_selector(move || selector.into())
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .px(px(shell::DETAIL_PADDING))
        .py(px(18.))
        .child(
            div()
                .w_full()
                .max_w(px(FORM_WIDTH))
                .flex()
                .flex_col()
                .gap_3()
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap_2()
                        .pb_3()
                        .border_b_1()
                        .border_color(theme::row_line())
                        .text_size(px(theme::font_body()))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme::text())
                        .child(format!("{own} {noun}{}", plural(own))),
                )
                .child(body),
        )
        .into_any_element()
}

fn files_body(preview: &ComparePreview) -> AnyElement {
    let (adds, dels) = preview.line_counts();
    let mut list = div().flex().flex_col();
    for file in &preview.files {
        let (dir, name) = split_path(&file.path);
        list = list.child(
            div()
                .debug_selector(|| "new-pull-file".into())
                .flex()
                .items_center()
                .gap_2()
                .min_h(px(34.))
                .border_b_1()
                .border_color(theme::row_line())
                .text_size(px(theme::font_meta()))
                .text_color(theme::text())
                .child(
                    div()
                        .flex_shrink_0()
                        .w(px(14.))
                        .font_family(theme::code_font())
                        .text_size(px(theme::font_mono_meta()))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(super::diff_controls::status_color(&file.status_code))
                        .child(file.status_code.clone()),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .overflow_hidden()
                        .whitespace_nowrap()
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
                .child(
                    div()
                        .flex_shrink_0()
                        .flex()
                        .gap_1p5()
                        .font_family(theme::code_font())
                        .child(
                            div()
                                .text_color(theme::SUCCESS_BRIGHT)
                                .child(format!("+{}", file.additions.unwrap_or(0))),
                        )
                        .child(
                            div()
                                .text_color(theme::DANGER)
                                .child(format!("−{}", file.deletions.unwrap_or(0))),
                        ),
                ),
        );
    }
    div()
        .flex()
        .flex_col()
        .child(list)
        .child(
            div()
                .pt_2()
                .flex()
                .gap_2()
                .font_family(theme::code_font())
                .text_size(px(theme::font_meta()))
                .child(
                    div()
                        .text_color(theme::SUCCESS_BRIGHT)
                        .child(format!("+{adds}")),
                )
                .child(div().text_color(theme::DANGER).child(format!("−{dels}"))),
        )
        .into_any_element()
}

fn commits_body(preview: &ComparePreview) -> AnyElement {
    let mut list = div().flex().flex_col();
    for commit in &preview.commits {
        let age = commit
            .committed_at
            .as_deref()
            .and_then(warp_station_core::rel_time::format_rfc3339)
            .unwrap_or_default();
        let who = commit.author.clone().unwrap_or_else(|| "Unknown".into());
        list = list.child(
            div()
                .debug_selector(|| "new-pull-commit".into())
                .flex()
                .items_start()
                .gap_2p5()
                .py_2()
                .border_b_1()
                .border_color(theme::row_line())
                .child(
                    div()
                        .flex_shrink_0()
                        .font_family(theme::code_font())
                        .text_size(px(theme::font_mono_meta()))
                        .text_color(theme::text_muted())
                        .child(short_sha(&commit.hash)),
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
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .child(commit.subject.clone()),
                        )
                        .child(
                            div()
                                .text_size(px(theme::font_meta()))
                                .text_color(theme::text_muted())
                                .child(format!("{who} · {age}")),
                        ),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(theme::font_meta()))
                        .text_color(theme::text_muted())
                        .child(format!("{} file{}", commit.files, plural(commit.files))),
                ),
        );
    }
    list.into_any_element()
}

fn plain_state(title: &'static str, detail: &'static str) -> AnyElement {
    div()
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .child(empty_state(title, detail))
        .into_any_element()
}

fn empty_state(title: &'static str, detail: &'static str) -> AnyElement {
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

fn split_path(path: &str) -> (String, String) {
    match path.rsplit_once('/') {
        Some((dir, name)) => (format!("{dir}/"), name.to_string()),
        None => (String::new(), path.to_string()),
    }
}
