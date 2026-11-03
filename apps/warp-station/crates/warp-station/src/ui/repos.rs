//! The Repos section: the strip of repo tabs over the list and detail panes.
//! The list pane's head holds the totals, the list's actions, its scope, tags
//! and sort. A repo tab hides the list so the detail takes the full width.
//! Everything reads the store at render time, so filter, sort and status
//! changes land with the next frame.

use crate::store::Store;
use crate::theme;
use crate::ui::A11y;
use crate::ui::detail::{DetailProps, DetailViews};
use crate::ui::dialogs::{AddRepoDialog, CloneDialog, Dialog};
use crate::ui::icon;
use crate::ui::overview::RepoMenu;
use gpui::prelude::*;
use gpui::{
    App, Context, DragMoveEvent, Entity, FontWeight, MouseButton, ScrollHandle, SharedString,
    Window, div, px,
};
use gpui_kit::component::input::{InputEvent, InputState};
use warp_station_core::settings::RepoSortMode;
use warpui_kit_core::{motion, shell};

/// The pane head's running totals over the live (non-archived) list.
#[derive(Default)]
struct ListStats {
    total: usize,
    /// Repos with uncommitted work, an open issue, or commits to push.
    attention: usize,
    favorites: usize,
    changes: u32,
    issues: u32,
}

pub struct ReposPage {
    pub store: Entity<Store>,
    pub dialog: Option<Dialog>,
    /// The tabs' views.
    pub views: DetailViews,
    /// The Status card's inline tag editor: the flag is set by the Add tag
    /// link and the input is created on the next frame, where the window is
    /// in hand.
    tag_editing: bool,
    tag_draft: Option<Entity<InputState>>,
    /// The detail head's "⋯" menu while it is open.
    repo_menu: Option<RepoMenu>,
    /// The split button's launcher menu.
    launch_menu_open: bool,
    /// The list head's sort menu.
    sort_menu_open: bool,
    /// The list head's tag menu, and what it lists: the tags with a live repo.
    tag_menu_open: bool,
    tag_counts: Vec<(String, usize)>,
    /// Ctrl+\\ on the Repositories tab: the list is out of the way.
    list_hidden: bool,
    /// Ctrl+\\ inside a repo tab: the list is back.
    list_in_tab: bool,
    /// Keeps the active tab in view when the strip overflows.
    strip_scroll: ScrollHandle,
    strip_active: Option<usize>,
}

impl ReposPage {
    pub fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let views = DetailViews::new(&store, window, cx);
        cx.observe(&store, |this, store, cx| {
            store.update(cx, |s, cx| s.ensure_row_selected(cx));
            // the menu belongs to the repo it opened on
            let selected = store.read(cx).selected_repo.clone();
            if this
                .repo_menu
                .as_ref()
                .is_some_and(|menu| Some(&menu.folder) != selected.as_ref())
            {
                this.close_repo_menu(cx);
            }
            cx.notify();
        })
        .detach();
        store.update(cx, |s, cx| s.ensure_row_selected(cx));
        Self {
            store,
            dialog: None,
            views,
            tag_editing: false,
            tag_draft: None,
            repo_menu: None,
            launch_menu_open: false,
            sort_menu_open: false,
            tag_menu_open: false,
            tag_counts: Vec::new(),
            list_hidden: false,
            strip_scroll: ScrollHandle::new(),
            strip_active: None,
            list_in_tab: false,
        }
    }
}

impl ReposPage {
    /// The head's "⋯" menu.
    pub fn toggle_repo_menu(&mut self, folder: &str, cx: &mut Context<Self>) {
        self.repo_menu = match self.repo_menu {
            Some(_) => None,
            None => Some(RepoMenu {
                folder: folder.to_string(),
                remove_armed: false,
            }),
        };
        cx.notify();
    }

    pub fn toggle_launch_menu(&mut self, cx: &mut Context<Self>) {
        self.launch_menu_open = !self.launch_menu_open;
        cx.notify();
    }

    /// Ctrl+\\ and the title bar's toggle: the list follows the view it is on.
    pub fn toggle_list(&mut self, cx: &mut Context<Self>) {
        if self.store.read(cx).active_repo_tab.is_some() {
            self.list_in_tab = !self.list_in_tab;
        } else {
            self.list_hidden = !self.list_hidden;
        }
        cx.notify();
    }

    /// Whether the list pane draws. A repo tab hides it unless asked back; a
    /// wide tab hides it on a narrow window.
    fn list_visible(&self, window: &Window, cx: &App) -> bool {
        let store = self.store.read(cx);
        let wide_tab = matches!(
            store.active_detail_tab(),
            crate::store::WorkspaceTab::Changes | crate::store::WorkspaceTab::History
        );
        let compact = window.viewport_size().width < px(shell::scaled(shell::COMPACT_WINDOW_WIDTH));
        let wanted = if store.active_repo_tab.is_some() {
            self.list_in_tab
        } else {
            !self.list_hidden
        };
        wanted && !(wide_tab && compact)
    }

    pub fn close_repo_menu(&mut self, cx: &mut Context<Self>) {
        if self.repo_menu.take().is_some() {
            cx.notify();
        }
    }

    /// The first click on the menu's Remove row; the second one removes.
    pub fn arm_repo_remove(&mut self, cx: &mut Context<Self>) {
        if let Some(menu) = &mut self.repo_menu {
            menu.remove_armed = true;
            cx.notify();
        }
    }

    fn toggle_sort_menu(&mut self, cx: &mut Context<Self>) {
        self.sort_menu_open = !self.sort_menu_open;
        self.tag_menu_open = false;
        cx.notify();
    }

    /// An outside press closes whichever of the page's menus is open.
    pub fn close_popups(&mut self, cx: &mut Context<Self>) {
        self.close_repo_menu(cx);
        if self.sort_menu_open || self.tag_menu_open || self.launch_menu_open {
            self.sort_menu_open = false;
            self.tag_menu_open = false;
            self.launch_menu_open = false;
            cx.notify();
        }
        self.views
            .changes
            .update(cx, crate::ui::changes::ChangesView::dismiss_branch_menu);
    }

    /// Whether something sits on top of the list right now, so the window's
    /// row keymap stays out of it.
    pub fn layer_open(&self) -> bool {
        self.dialog.is_some()
            || self.repo_menu.is_some()
            || self.sort_menu_open
            || self.tag_menu_open
            || self.launch_menu_open
    }

    /// The Add tag link asks for the inline editor; the input itself is
    /// created on the next frame, where the window is available.
    pub fn begin_tag_edit(&mut self, cx: &mut Context<Self>) {
        self.tag_editing = true;
        cx.notify();
    }

    pub fn cancel_tag_edit(&mut self, cx: &mut Context<Self>) {
        self.tag_editing = false;
        self.tag_draft = None;
        cx.notify();
    }

    /// Creates the tag input the frame the chip asked for it.
    fn ensure_tag_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.tag_editing || self.tag_draft.is_some() {
            return;
        }
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("tag name"));
        input.update(cx, |state, cx| state.focus(window, cx));
        cx.subscribe(&input, |this: &mut ReposPage, _, event: &InputEvent, cx| {
            if !matches!(event, InputEvent::PressEnter { .. }) {
                return;
            }
            let value = this
                .tag_draft
                .as_ref()
                .map(|input| input.read(cx).value().trim().to_string())
                .unwrap_or_default();
            let folder = this.store.read(cx).selected_repo.clone();
            if let (Some(folder), false) = (folder, value.is_empty()) {
                this.store
                    .update(cx, |s, cx| s.add_repo_tag(&folder, &value, cx));
            }
            this.tag_editing = false;
            this.tag_draft = None;
            cx.notify();
        })
        .detach();
        self.tag_draft = Some(input);
    }

    /// Escape closes the topmost page overlay. Returns whether one was
    /// actually open, so the window can fall through to its own layers.
    pub fn dismiss_overlay(&mut self, cx: &mut Context<Self>) -> bool {
        if self.dialog.is_some() {
            self.dialog = None;
            cx.notify();
            return true;
        }
        // the inline editor closes on Escape before anything else
        if self.tag_editing || self.tag_draft.is_some() {
            self.tag_editing = false;
            self.tag_draft = None;
            cx.notify();
            return true;
        }
        if self.repo_menu.take().is_some() {
            cx.notify();
            return true;
        }
        if self.sort_menu_open || self.tag_menu_open || self.launch_menu_open {
            self.sort_menu_open = false;
            self.tag_menu_open = false;
            self.launch_menu_open = false;
            cx.notify();
            return true;
        }
        let tab = self.store.read(cx).active_detail_tab();
        if self.store.read(cx).open_pull.is_none() && self.views.dismiss_list(tab, cx) {
            return true;
        }
        if self.store.update(cx, |s, cx| s.close_pull_request(cx)) {
            return true;
        }
        self.views
            .changes
            .update(cx, crate::ui::changes::ChangesView::dismiss_branch_menu)
    }

    /// Sends the Add-Repositories dialog's deep scan.
    pub fn kick_deep_scan(&mut self, cx: &mut Context<Self>) {
        let Some(Dialog::AddRepo(dialog)) = &mut self.dialog else {
            return;
        };
        // Enter and the Rescan button both route here; a scan already in
        // flight must not be re-sent (two settles would flash two result sets)
        if dialog.scanning {
            return;
        }
        let root = dialog.root.read(cx).value().to_string();
        if root.trim().is_empty() {
            return;
        }
        let depth = dialog.depth;
        dialog.scanning = true;
        dialog.scanned = false;
        dialog.found.clear();
        let settings = self.store.read(cx).settings();
        let _ = self
            .store
            .read(cx)
            .net
            .send_blocking(crate::net::NetCommand::DeepScan {
                root: root.trim().to_string(),
                depth,
                settings: settings.repos.clone(),
            });
        cx.notify();
    }

    /// Adds every checked entry from the Add-Repositories dialog.
    pub fn commit_add_dialog(&mut self, cx: &mut Context<Self>) {
        let Some(Dialog::AddRepo(dialog)) = &mut self.dialog else {
            return;
        };
        let selected: Vec<warp_station_core::scan::ScannedRepo> = dialog
            .found
            .iter()
            .filter(|(_, checked, tracked)| *checked && !*tracked)
            .map(|(repo, _, _)| repo.clone())
            .collect();
        if selected.is_empty() {
            self.dialog = None;
            cx.notify();
            return;
        }
        self.store
            .update(cx, |store, cx| store.register_repos(&selected, cx));
        self.dialog = None;
        cx.notify();
    }

    /// Sends the Clone dialog's clone (with inline validation).
    pub fn commit_clone(&mut self, cx: &mut Context<Self>) {
        let Some(Dialog::Clone(dialog)) = &mut self.dialog else {
            return;
        };
        let mut validation_error: Option<String> = None;
        {
            let url = dialog.url.read(cx).value().to_string();
            let name = dialog.folder_name(cx);
            let destination = dialog.destination.read(cx).value().to_string();
            if url.trim().is_empty() || (!url.contains('/') && !url.contains(':')) {
                validation_error = Some("Enter a repository URL".into());
            } else if name.is_empty()
                || name == "."
                || name == ".."
                || name.contains('/')
                || name.contains('\\')
            {
                validation_error = Some("Enter a usable repository name".into());
            } else if destination.trim().is_empty() || !std::path::Path::new(&destination).is_dir()
            {
                validation_error = Some("Destination folder does not exist".into());
            } else if std::path::Path::new(&destination).join(&name).exists() {
                validation_error = Some("Target folder already exists".into());
            }
        }
        if let Some(error) = validation_error {
            dialog.busy = false;
            dialog.error = Some(error);
            cx.notify();
            return;
        }
        let url = dialog.url.read(cx).value().to_string();
        let name = dialog.folder_name(cx);
        let destination = dialog.destination.read(cx).value().to_string();
        dialog.busy = true;
        dialog.error = None;
        self.store.update(cx, |s, cx| {
            s.cloning.push(name.clone());
            cx.notify();
        });
        let _ = self
            .store
            .read(cx)
            .net
            .send_blocking(crate::net::NetCommand::Clone {
                url: url.trim().to_string(),
                parent_dir: destination,
                name,
            });
        cx.notify();
    }
}

impl Render for ReposPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        // drain dialog-bound results the moment we are about to draw
        let drained_deep_scan = self
            .store
            .update(cx, |store, _| store.deep_scan_result.take());
        if let Some(found) = drained_deep_scan
            && let Some(Dialog::AddRepo(dialog)) = &mut self.dialog
        {
            dialog.scanning = false;
            dialog.scanned = true;
            dialog.found = found
                .into_iter()
                .map(|repo| {
                    let tracked = self
                        .store
                        .read(cx)
                        .repos
                        .iter()
                        .any(|r| r.folder_path == repo.folder_path);
                    (repo, !tracked, tracked)
                })
                .collect();
        }
        if let Some(path) = self
            .store
            .update(cx, |store, _| store.folder_pick_result.take())
            && let Some(Dialog::AddRepo(dialog)) = &mut self.dialog
        {
            dialog
                .root
                .update(cx, |field, cx| field.set_value(&path, window, cx));
        }
        let drained_clone = self.store.update(cx, |store, _| store.clone_result.take());
        if let Some((parent_dir, name, result)) = drained_clone {
            if result.success {
                self.store
                    .update(cx, |store, cx| store.adopt_clone(&parent_dir, &name, cx));
                self.dialog = None;
            } else if let Some(Dialog::Clone(dialog)) = &mut self.dialog {
                dialog.busy = false;
                dialog.error = result.error.or_else(|| Some("clone failed".into()));
            }
        }
        self.ensure_tag_editor(window, cx);
        let list_width = self.store.read(cx).list_width();
        let list_visible = self.list_visible(window, cx);
        let strip = self.render_strip(cx);
        let page = cx.entity();
        let list = list_visible.then(|| {
            let head = self.render_list_head(cx);
            let sort_menu = self.render_sort_menu(cx);
            let tag_menu = self.render_tag_menu(cx);
            let store = self.store.read(cx);
            shell::list_pane(list_width)
                .relative()
                .debug_selector(|| "repos-list-pane".into())
                .child(head)
                .child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .relative()
                        .child(crate::ui::repos_list::render(store, &self.store))
                        // last, so they paint over the rows
                        .children(sort_menu)
                        .children(tag_menu),
                )
                .child(resize_handle())
        });
        let detail = {
            let store = self.store.read(cx);
            crate::ui::detail::render(
                store,
                &self.store,
                &page,
                &self.views,
                DetailProps {
                    tag_draft: self.tag_draft.as_ref(),
                    repo_menu: self.repo_menu.as_ref(),
                    launch_menu_open: self.launch_menu_open,
                },
                window,
                cx,
            )
        };
        let mut stage = shell::stage().relative().child(strip).child(
            shell::panes()
                .on_drag_move(
                    cx.listener(|this, event: &DragMoveEvent<ListResize>, _, cx| {
                        let width = f32::from(event.event.position.x - event.bounds.left());
                        this.store.update(cx, |s, cx| s.set_list_width(width, cx));
                    }),
                )
                .on_drop(cx.listener(|this, _: &ListResize, _, cx| {
                    this.store.read(cx).save_list_width();
                }))
                .children(list)
                .child(detail),
        );
        // a dialog is a layer over the page
        if let Some(dialog) = &mut self.dialog {
            stage = stage.child(dialog.render(window, cx));
        }
        stage
    }
}

impl ReposPage {
    /// The strip of opened repositories over the panes.
    fn render_strip(&mut self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let store = self.store.read(cx);
        let active = store.active_repo_tab.clone();
        // the strip's children count from Repositories
        let active_ix = active
            .as_ref()
            .and_then(|folder| store.repo_tabs.iter().position(|tab| &tab.folder == folder))
            .map(|ix| ix + 1);
        if std::mem::replace(&mut self.strip_active, active_ix) != active_ix {
            self.strip_scroll.scroll_to_item(active_ix.unwrap_or(0));
        }
        let home_target = self.store.clone();
        let mut strip = shell::tab_strip()
            .id("repo-tab-strip")
            .debug_selector(|| "repo-tab-strip".into())
            .overflow_x_scroll()
            .track_scroll(&self.strip_scroll)
            .child(
                shell::strip_tab(
                    "repo-tab-home",
                    "Repositories",
                    active.is_none(),
                    theme::surface(),
                )
                .debug_selector(|| "repo-tab-home".into())
                .flex_shrink_0()
                .on_click(move |_, _, cx| {
                    home_target.update(cx, |s, cx| s.show_repo_tab(None, cx));
                })
                .child(icon(
                    "icons/icon-folder.svg",
                    px(theme::icon_small()),
                    theme::text_dim(),
                ))
                .child("Repositories"),
            );
        for (ix, tab) in store.repo_tabs.iter().enumerate() {
            let selected = active.as_deref() == Some(tab.folder.as_str());
            let repo = store
                .repos
                .iter()
                .find(|repo| repo.folder_path.to_string_lossy() == tab.folder.as_str());
            let name = repo.map_or_else(|| tab.folder.clone(), |repo| repo.name.clone());
            let changes = repo.map_or(0, |repo| repo.git_modified_count);
            let job = store.git_jobs.contains_key(&tab.folder);
            let show_target = self.store.clone();
            let close_target = self.store.clone();
            let middle_target = self.store.clone();
            let show_folder = tab.folder.clone();
            let close_folder = tab.folder.clone();
            let middle_folder = tab.folder.clone();
            let group = SharedString::from(format!("strip-tab-{ix}"));
            strip = strip.child(motion::enter(
                SharedString::from(format!("repo-tab-enter-{}", tab.folder)),
                shell::strip_tab(
                    SharedString::from(format!("repo-tab-{ix}-id")),
                    name.clone(),
                    selected,
                    theme::panel(),
                )
                .debug_selector(move || format!("repo-strip-tab-{ix}"))
                .group(group.clone())
                .on_click(move |_, _, cx| {
                    show_target.update(cx, |s, cx| s.show_repo_tab(Some(show_folder.clone()), cx));
                })
                .on_mouse_down(MouseButton::Middle, move |_, _, cx| {
                    cx.stop_propagation();
                    middle_target.update(cx, |s, cx| s.close_repo_tab(&middle_folder, cx));
                })
                .when(job, |tab| {
                    let selector = format!("repo-strip-job-{ix}");
                    tab.debug_selector(move || selector.clone())
                        .child(motion::spinner(
                            SharedString::from(format!("repo-strip-spinner-{ix}")),
                            "icons/icon-spinner.svg",
                            px(theme::icon_tiny()),
                            theme::text_dim(),
                        ))
                })
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(name.clone()),
                )
                .when(changes > 0, |tab| {
                    tab.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.))
                            .text_size(px(theme::font_meta()))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme::CHANGES)
                            .child(icon(
                                "icons/icon-file-diff.svg",
                                px(theme::icon_tiny()),
                                theme::CHANGES,
                            ))
                            .child(format!("{changes}")),
                    )
                })
                .child(
                    div()
                        .id(SharedString::from(format!("repo-tab-close-{ix}")))
                        .debug_selector(move || format!("repo-strip-close-{ix}"))
                        .a11y_button(format!("Close {name}"))
                        .cursor_pointer()
                        .flex_shrink_0()
                        .ml_auto()
                        .size(px(22.))
                        .rounded(px(4.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .when(!selected, |x| {
                            x.invisible().group_hover(group, |s| s.visible())
                        })
                        .hover(|s| s.bg(theme::raised()))
                        .on_click(move |_, _, cx| {
                            cx.stop_propagation();
                            close_target.update(cx, |s, cx| s.close_repo_tab(&close_folder, cx));
                        })
                        .child(icon(
                            "icons/icon-close.svg",
                            px(theme::icon_tiny()),
                            theme::text_muted(),
                        )),
                ),
            ));
        }
        strip.into_any_element()
    }

    /// The list pane's head: totals and the list's actions, the scope, then
    /// tags and sort. The tab strip already names the list, so it has no title.
    fn render_list_head(&mut self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let (tags, active_tags, scanning) = {
            let store = self.store.read(cx);
            (store.all_tags(), store.tag_filters.clone(), store.scanning)
        };

        let (archived_count, show_archived) = {
            let store = self.store.read(cx);
            (
                store.repos.iter().filter(|r| r.is_archived()).count(),
                store.show_archived,
            )
        };

        let stats = {
            let store = self.store.read(cx);
            // totals describe the live list: archived repos leave every sum
            let mut stats = ListStats::default();
            for repo in store.repos.iter().filter(|r| !r.is_archived()) {
                stats.total += 1;
                stats.changes += repo.git_modified_count;
                stats.issues += repo.github_issue_count;
                if repo.is_favorite() {
                    stats.favorites += 1;
                }
                if crate::store::projection::needs_attention(repo) {
                    stats.attention += 1;
                }
            }
            stats
        };
        let attention_only = self.store.read(cx).attention_only;
        let filters_active =
            !active_tags.is_empty() || self.store.read(cx).group_filter.is_some() || attention_only;

        // Tag filters read as chips: raised when active, quiet otherwise (OR
        // semantics across the checked set); each carries its live count. User
        // tags only: the derived flags have their own scope segments, and a
        // zero-count chip is noise.
        let tag_counts: Vec<(String, usize)> = {
            let store = self.store.read(cx);
            tags.iter()
                .filter(|tag| {
                    !tag.eq_ignore_ascii_case(warp_station_core::repo_store::FAVORITES_TAG)
                        && !tag.eq_ignore_ascii_case(warp_station_core::repo_store::ARCHIVED_TAG)
                })
                .map(|tag| {
                    let count = store
                        .repos
                        .iter()
                        .filter(|repo| !repo.is_archived())
                        .filter(|repo| repo.tags.iter().any(|t| t.eq_ignore_ascii_case(tag)))
                        .count();
                    (tag.clone(), count)
                })
                .filter(|(_, count)| *count > 0)
                .collect()
        };
        let tag_label = match active_tags.as_slice() {
            [] => "Any".to_string(),
            [tag] => tag.clone(),
            tags => format!("{} tags", tags.len()),
        };
        let tag_button = div()
            .id("tag-button")
            .debug_selector(|| "tag-button".into())
            .a11y_button(format!("Tag: {tag_label}"))
            .cursor_pointer()
            .flex_shrink_0()
            .h(px(26.))
            .px_2()
            .rounded(px(6.))
            .flex()
            .items_center()
            .gap(px(6.))
            .text_size(px(theme::font_meta()))
            .text_color(theme::text_dim())
            .hover(|s| s.bg(theme::raised()))
            // as the Sort button: a press toggles instead of close-then-reopen
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(|this, _, _, cx| {
                this.tag_menu_open = !this.tag_menu_open;
                this.sort_menu_open = false;
                cx.notify();
            }))
            .child("Tag:")
            .child(
                div()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme::text())
                    .child(tag_label),
            )
            .child(icon(
                "icons/icon-chevron-down.svg",
                px(theme::icon_tiny()),
                theme::text_muted(),
            ));
        self.tag_counts = tag_counts;

        let store_refresh = self.store.clone();
        let actions = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .flex_shrink_0()
            .child(icon_action(
                "refresh-button",
                if scanning { "Refreshing" } else { "Refresh" },
                "icons/icon-refresh.svg",
                scanning,
                move |_, _, cx| {
                    store_refresh.update(cx, |s, cx| s.refresh(cx));
                },
            ))
            .child(icon_action(
                "add-repo-button",
                "Add repo",
                "icons/icon-plus.svg",
                false,
                cx.listener(|this, _, window, cx| {
                    let depth = this.store.read(cx).settings().repos.max_scan_depth;
                    this.dialog = Some(Dialog::AddRepo(AddRepoDialog::new(window, depth, cx)));
                    cx.notify();
                }),
            ))
            .child(icon_action(
                "clone-button",
                "Clone",
                "icons/icon-cloud-download.svg",
                !self.store.read(cx).cloning.is_empty(),
                cx.listener(|this, _, window, cx| {
                    let store = this.store.clone();
                    this.dialog = Some(Dialog::Clone(CloneDialog::new(&store, window, cx)));
                    cx.notify();
                }),
            ));

        // The summary line: what needs a hand first, then the two totals that
        // make a repo look busy. The phrase stays at zero so the line always
        // answers the same question; only its weight goes quiet.
        let attention_label = div()
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(if stats.attention > 0 {
                theme::CHANGES
            } else {
                theme::text_dim()
            })
            .child(format!(
                "{} of {} need attention",
                stats.attention, stats.total
            ));
        let summary = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(5.))
            .overflow_hidden()
            .whitespace_nowrap()
            .text_size(px(theme::font_sub()))
            .text_color(theme::text_dim())
            .child(attention_label)
            .child(format!(
                "· {} changes · {} issues",
                stats.changes, stats.issues
            ));

        // Scope segmented control: one row of filters, counts included.
        let scope = segment_row()
            .child(segment(
                "scope-all",
                "All",
                Some(stats.total),
                !attention_only && active_tags.is_empty(),
                cx.listener(|this, _, _, cx| {
                    this.store.update(cx, |s, cx| {
                        s.clear_filters(cx);
                        if s.show_archived {
                            s.toggle_show_archived(cx);
                        }
                    });
                }),
            ))
            .child(segment(
                "scope-attention",
                "Needs attention",
                Some(stats.attention),
                attention_only,
                cx.listener(|this, _, _, cx| {
                    this.store
                        .update(cx, |s, cx| s.set_attention_only(true, cx));
                }),
            ))
            .child(segment(
                "scope-favorites",
                "Favorites",
                Some(stats.favorites),
                active_tags
                    .iter()
                    .any(|t| t.eq_ignore_ascii_case(warp_station_core::repo_store::FAVORITES_TAG)),
                cx.listener(|this, _, _, cx| {
                    this.store.update(cx, |s, cx| {
                        s.toggle_tag_filter(warp_station_core::repo_store::FAVORITES_TAG, cx);
                    });
                }),
            ))
            .child(segment(
                "archive-chip",
                "Archive",
                Some(archived_count),
                show_archived,
                cx.listener(|this, _, _, cx| {
                    this.store.update(cx, |s, cx| s.toggle_show_archived(cx));
                }),
            ));

        let sort_mode = self.store.read(cx).sort_mode;
        let sort_button = div()
            .id("sort-button")
            .debug_selector(|| "sort-button".into())
            .a11y_button(format!("Sort: {}", sort_label(sort_mode)))
            .cursor_pointer()
            .flex_shrink_0()
            .h(px(26.))
            .px_2()
            .rounded(px(6.))
            .flex()
            .items_center()
            .gap(px(6.))
            .text_size(px(theme::font_meta()))
            .text_color(theme::text_dim())
            .hover(|s| s.bg(theme::raised()))
            // keep the window's outside-click dismissal from seeing this
            // press: it must toggle, not close-then-reopen
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(|this, _, _, cx| this.toggle_sort_menu(cx)))
            .child("Sort:")
            .child(
                div()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme::text())
                    .child(sort_label(sort_mode)),
            )
            .child(icon(
                "icons/icon-chevron-down.svg",
                px(theme::icon_tiny()),
                theme::text_muted(),
            ));

        let store_clear = self.store.clone();
        let tools = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .min_h(px(26.))
            .child(tag_button)
            .child(div().flex_1())
            .when(filters_active, |row| {
                row.child(
                    div()
                        .id("clear-button")
                        .debug_selector(|| "clear-button".into())
                        .a11y_button("Clear filters")
                        .cursor_pointer()
                        .flex_shrink_0()
                        .text_size(px(theme::font_meta()))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme::accent_ink())
                        .hover(|s| s.text_color(theme::accent_hover()))
                        .on_click(move |_, _, cx| {
                            store_clear.update(cx, |s, cx| s.clear_filters(cx));
                        })
                        .child("Clear filters"),
                )
            })
            .child(sort_button);

        div()
            .relative()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap_2p5()
            .px_4()
            .pt_3p5()
            .pb_2()
            .border_b_1()
            .border_color(theme::hairline())
            .when(scanning, |head| {
                head.child(motion::progress_line("list-progress", theme::accent()))
            })
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .min_h(px(30.))
                    .child(summary)
                    .child(actions),
            )
            .child(scope)
            .child(tools)
            .into_any_element()
    }

    /// The list head's sort menu, hanging under its button at the list's top
    /// right. Picking the active mode flips its direction.
    /// Any, then every tag with a live repo. A pick stays open: tags add up.
    fn render_tag_menu(&self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        if !self.tag_menu_open {
            return None;
        }
        let active = self.store.read(cx).tag_filters.clone();
        let row = |id: SharedString, label: String, on: bool| {
            let selector = id.clone();
            div()
                .id(id)
                .debug_selector(move || selector.to_string())
                .a11y_toggle(label.clone(), on)
                .cursor_pointer()
                .flex_shrink_0()
                .rounded(px(6.))
                .h(px(32.))
                .px_2()
                .flex()
                .items_center()
                .gap_2()
                .text_size(px(theme::font_control()))
                .text_color(theme::text())
                .hover(|d| d.bg(theme::line_strong()))
                .child(div().flex_1().min_w_0().child(label))
                .when(on, |d| {
                    d.child(icon(
                        "icons/icon-check.svg",
                        px(theme::icon_small()),
                        theme::accent_ink(),
                    ))
                })
        };
        let mut menu = div()
            .id("tag-menu")
            .debug_selector(|| "tag-menu".into())
            .absolute()
            .top(px(4.))
            .left(px(16.))
            .w(px(220.))
            .max_h(px(320.))
            .overflow_y_scroll()
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
            .child(
                row("tag-any".into(), "Any".to_string(), active.is_empty()).on_click(cx.listener(
                    |this, _, _, cx| {
                        this.store.update(cx, |s, cx| s.clear_tag_filters(cx));
                        this.tag_menu_open = false;
                        cx.notify();
                    },
                )),
            );
        for (tag, count) in &self.tag_counts {
            let on = active.iter().any(|t| t.eq_ignore_ascii_case(tag));
            let target = tag.clone();
            menu = menu.child(
                row(
                    SharedString::from(format!("tag-pick-{tag}")),
                    format!("{tag}  {count}"),
                    on,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.store
                        .update(cx, |s, cx| s.toggle_tag_filter(&target, cx));
                })),
            );
        }
        Some(menu.into_any_element())
    }

    fn render_sort_menu(&self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        if !self.sort_menu_open {
            return None;
        }
        let (current, ascending, github) = {
            let store = self.store.read(cx);
            (
                store.sort_mode,
                store.sort_ascending,
                store.settings().repos.enable_git_hub,
            )
        };
        let mut menu = div()
            .id("sort-menu")
            .debug_selector(|| "sort-menu".into())
            .absolute()
            .top(px(4.))
            .right(px(16.))
            .w(px(200.))
            .p(px(4.))
            .rounded(px(8.))
            .border_1()
            .border_color(theme::line_strong())
            .bg(theme::overlay())
            .shadow_lg()
            // presses inside must not reach the rows under it, or the
            // window's outside-click dismissal before the row's click lands
            .occlude()
            .flex()
            .flex_col()
            .gap(px(2.));
        for (mode, label) in SORT_MODES {
            if mode == RepoSortMode::GitHub && !github {
                continue;
            }
            let active = mode == current;
            let selector = format!("sort-mode-{label}");
            menu = menu.child(
                div()
                    .id(SharedString::from(selector.clone()))
                    .debug_selector(move || selector.clone())
                    .a11y_toggle(label, active)
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
                        this.store.update(cx, |s, cx| {
                            let ascending = if s.sort_mode == mode {
                                !s.sort_ascending
                            } else {
                                mode.default_ascending()
                            };
                            s.set_sort_direction(mode, ascending, cx);
                        });
                        this.sort_menu_open = false;
                        cx.notify();
                    }))
                    .child(label)
                    .when(active, |row| row.child(sort_arrow(ascending))),
            );
        }
        Some(menu.into_any_element())
    }
}

/// The sort menu's modes: the design's three first, then the app's own
/// orders (they rank by data the design's list does not show).
const SORT_MODES: [(RepoSortMode, &str); 6] = [
    (RepoSortMode::LastActivity, "Last activity"),
    (RepoSortMode::Name, "Name"),
    (RepoSortMode::Attention, "Attention first"),
    (RepoSortMode::Changes, "Changes"),
    (RepoSortMode::GitHub, "GitHub"),
    (RepoSortMode::Branch, "Branch"),
];

fn sort_label(mode: RepoSortMode) -> &'static str {
    SORT_MODES
        .iter()
        .find(|(candidate, _)| *candidate == mode)
        .map_or("Name", |(_, label)| label)
}

fn sort_arrow(ascending: bool) -> &'static str {
    if ascending { "↑" } else { "↓" }
}

/// A list action: a quiet square with its name in the accessibility tree. Busy,
/// its icon turns.
fn icon_action(
    id: &'static str,
    label: &'static str,
    icon_path: &'static str,
    busy: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> gpui::AnyElement {
    div()
        .id(id)
        .a11y_button(label)
        .debug_selector(move || id.into())
        .cursor_pointer()
        .flex_shrink_0()
        .size(px(30.))
        .rounded(px(6.))
        .flex()
        .items_center()
        .justify_center()
        .hover(|d| d.bg(theme::raised()))
        .on_click(on_click)
        .child(if busy {
            motion::spinner(
                SharedString::from(format!("{id}-spin")),
                icon_path,
                px(theme::icon_medium()),
                theme::BEHIND,
            )
            .into_any_element()
        } else {
            icon(icon_path, px(theme::icon_medium()), theme::text_dim()).into_any_element()
        })
        .into_any_element()
}

/// The segmented control's trough.
/// What a drag on the list's edge carries. It draws nothing: the list itself is
/// the feedback.
struct ListResize;

impl Render for ListResize {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl gpui::IntoElement {
        gpui::Empty
    }
}

/// The list's right edge, on top of its hairline.
fn resize_handle() -> impl IntoElement {
    div()
        .id("list-resize")
        .debug_selector(|| "list-resize".into())
        .absolute()
        .top_0()
        .bottom_0()
        .right(px(-3.))
        .w(px(6.))
        .cursor_col_resize()
        .hover(|s| s.bg(theme::line_strong()))
        .on_drag(ListResize, |_, _, _, cx| {
            cx.stop_propagation();
            cx.new(|_| ListResize)
        })
}

pub(crate) fn segment_row() -> gpui::Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_0p5()
        .h(px(36.))
        .p(px(3.))
        .rounded(px(8.))
        .bg(theme::seg_bg())
}

/// One segment: label, count when it has one, and the raised fill when it is
/// the active choice.
pub(crate) fn segment(
    id: &'static str,
    label: &'static str,
    count: Option<usize>,
    active: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> gpui::AnyElement {
    div()
        .id(id)
        .a11y_toggle(
            match count {
                Some(count) => format!("{label} ({count})"),
                None => label.to_string(),
            },
            active,
        )
        .debug_selector(move || id.into())
        .cursor_pointer()
        .rounded(px(6.))
        .h(px(30.))
        .px(px(7.))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(5.))
        .text_size(px(theme::font_row_meta()))
        .font_weight(FontWeight::MEDIUM)
        .when(active, |d| {
            d.bg(theme::seg_active())
                .text_color(theme::seg_active_fg())
                .font_weight(FontWeight::SEMIBOLD)
        })
        .when(!active, |d| {
            d.text_color(theme::text_dim())
                .hover(|s| s.text_color(theme::text()))
        })
        .on_click(on_click)
        .child(label)
        .children(count.map(|count| {
            div()
                .text_color(if active {
                    theme::accent_ink()
                } else {
                    theme::text_muted()
                })
                .child(format!("{count}"))
        }))
        .into_any_element()
}
