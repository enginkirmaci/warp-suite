//! The detail pane: the one place a repository opens. A head with its identity
//! and verbs, a tab strip, and the open tab under it. What used to be a rail and
//! a bottom sheet are tabs here, so nothing ever floats over the list.

use crate::store::{Store, WorkspaceTab};
use crate::theme;
use crate::ui::azure_tab::AzureView;
use crate::ui::changes::ChangesView;
use crate::ui::launchers;
use crate::ui::overview::{self, HeadState, RepoMenu};
use crate::ui::pr_issues::ItemListView;
use crate::ui::pull_request;
use crate::ui::readme::ReadmeView;
use crate::ui::repos::ReposPage;
use gpui::prelude::*;
use gpui::{AnyElement, App, Context, Entity, FontWeight, SharedString, Window, div, px};
use gpui_kit::component::input::InputState;
use warpui_kit_core::{motion, shell};

/// The tabs' views. They read the selected repo from the store, so one set
/// serves every repo and repo tab.
pub struct DetailViews {
    pub changes: Entity<ChangesView>,
    pulls: Entity<ItemListView>,
    issues: Entity<ItemListView>,
    pipelines: Entity<AzureView>,
    readme: Entity<ReadmeView>,
}

impl DetailViews {
    pub fn new(store: &Entity<Store>, window: &mut Window, cx: &mut Context<ReposPage>) -> Self {
        Self {
            changes: cx.new(|cx| ChangesView::new(store.clone(), window, cx)),
            pulls: cx.new(|cx| ItemListView::new(store.clone(), true, window, cx)),
            issues: cx.new(|cx| ItemListView::new(store.clone(), false, window, cx)),
            pipelines: cx.new(|_| AzureView {
                store: store.clone(),
            }),
            readme: cx.new(|cx| ReadmeView::new(store.clone(), cx)),
        }
    }

    /// Escape, for whichever list tab is showing: its menu, then its filter.
    pub fn dismiss_list(&self, tab: WorkspaceTab, cx: &mut App) -> bool {
        let view = match tab {
            WorkspaceTab::PullRequests => &self.pulls,
            WorkspaceTab::Issues => &self.issues,
            _ => return false,
        };
        view.update(cx, |view, cx| view.dismiss(cx))
    }

    fn panel(&self, tab: WorkspaceTab) -> Option<AnyElement> {
        Some(match tab {
            WorkspaceTab::Overview => return None,
            WorkspaceTab::Changes | WorkspaceTab::History => {
                self.changes.clone().into_any_element()
            }
            WorkspaceTab::PullRequests => self.pulls.clone().into_any_element(),
            WorkspaceTab::Issues => self.issues.clone().into_any_element(),
            WorkspaceTab::Pipelines => self.pipelines.clone().into_any_element(),
            WorkspaceTab::Readme => self.readme.clone().into_any_element(),
        })
    }
}

/// The page's state the pane draws: the inline tag editor and the head's menus.
pub struct DetailProps<'a> {
    pub tag_draft: Option<&'a Entity<InputState>>,
    pub repo_menu: Option<&'a RepoMenu>,
    pub launch_menu_open: bool,
}

pub fn render(
    store: &Store,
    entities: &Entity<Store>,
    page: &Entity<ReposPage>,
    views: &DetailViews,
    props: DetailProps,
) -> AnyElement {
    let pane = shell::detail_pane()
        .debug_selector(|| "repos-detail".into())
        .relative();
    let Some(repo) = store.selected_entry() else {
        return pane.child(overview::empty_state()).into_any_element();
    };

    if let Some(open) = store.open_pull.as_ref() {
        let folder = repo.folder_path.to_string_lossy();
        let tabbed = store.repo_tabs.iter().any(|tab| tab.folder == folder);
        return pane
            .child(pull_request::render(
                repo,
                open,
                entities,
                tabbed,
                store.active_repo_tab.is_some(),
            ))
            .into_any_element();
    }

    let folder = repo.folder_path.to_string_lossy().to_string();
    let settings = store.settings();
    let launchers = launchers::launchers(&settings, repo);
    let active = store.active_detail_tab();
    let head_state = HeadState {
        filled: matches!(active, WorkspaceTab::Overview),
        tabbed: store.repo_tabs.iter().any(|tab| tab.folder == folder),
        in_tab: store.active_repo_tab.is_some(),
        launch_menu_open: props.launch_menu_open,
        star_press: store
            .star_pressed
            .as_ref()
            .filter(|(pressed, _)| *pressed == folder)
            .map(|(_, presses)| *presses),
    };

    let mut strip = div().flex().flex_row().mt_3().ml(px(-12.));
    for tab in store.detail_tabs() {
        let count = match tab {
            WorkspaceTab::Changes => Some(repo.git_modified_count),
            WorkspaceTab::PullRequests => Some(repo.github_pr_count),
            WorkspaceTab::Issues => Some(repo.github_issue_count),
            _ => None,
        };
        let target = entities.clone();
        let selector = tab.selector();
        strip = strip.child(
            shell::detail_tab(selector, tab.label(), tab == active)
                .debug_selector(move || selector.into())
                .on_click(move |_, _, cx| {
                    target.update(cx, |s, cx| s.open_detail_tab(tab, cx));
                })
                .children(count.filter(|count| *count > 0).map(|count| {
                    div()
                        .text_size(px(theme::font_meta()))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme::text_muted())
                        .child(format!("{count}"))
                })),
        );
    }

    let sub = div()
        .mt_1()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(14.))
        .child(
            div()
                .font_family(theme::code_font())
                .text_size(px(theme::font_mono_meta()))
                .text_color(theme::text_dim())
                .child(overview::display_path(repo)),
        )
        .child(crate::ui::repos_list::stats(
            repo,
            settings.repos.enable_git_hub,
            None,
        ));

    let head = div()
        .relative()
        .flex_shrink_0()
        .px(px(shell::DETAIL_PADDING))
        .pt_4()
        .border_b_1()
        .border_color(theme::hairline())
        .child(overview::header(
            repo,
            &launchers,
            &head_state,
            entities,
            page,
            &folder,
        ))
        .child(sub)
        .child(strip)
        .when(store.git_jobs.contains_key(&folder), |head| {
            head.child(motion::progress_line("detail-progress", theme::accent()))
        });

    let body = div()
        .debug_selector(move || format!("{}-panel", active.selector()))
        .flex_1()
        .min_h_0()
        .child(match views.panel(active) {
            Some(panel) => panel,
            None => overview::render(store, entities, page, props.tag_draft),
        });
    // keyed on what is shown, so a redraw never replays it
    let view = SharedString::from(format!("detail-enter-{folder}-{}", active.selector()));

    pane.child(head)
        .child(motion::enter(view, body))
        // last, so they paint over the tabs they hang above
        .when(head_state.launch_menu_open, |pane| {
            pane.child(overview::launch_menu(&launchers, &folder, entities, page))
        })
        .children(
            props
                .repo_menu
                .map(|menu| overview::popup_menu(repo, menu, store, entities, page, &folder)),
        )
        .into_any_element()
}
