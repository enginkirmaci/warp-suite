//! The Overview tab and the detail head above every tab: the repository's
//! identity, its one primary launch and the repo menu up top, then what needs a
//! person, where to open it, its status and what GitHub says about it, as cards
//! in two columns that stack when the pane gets narrow.

use crate::net::GitAction;
use crate::store::{Store, WorkspaceTab};
use crate::theme;
use crate::ui::A11y;
use crate::ui::icon;
use crate::ui::launchers::{self, Launcher};
use crate::ui::repos::ReposPage;
use gpui::prelude::*;
use gpui::{
    AnyElement, App, ClipboardItem, Entity, FontWeight, MouseButton, Rgba, SharedString, div, px,
};
use gpui_kit::component::Sizable as _;
use gpui_kit::component::input::{Input, InputState};
use warp_station_core::rel_time;
use warp_station_core::repo_store::RepoEntry;
use warp_station_core::settings::AppSettings;
use warp_station_core::terminal;
use warpui_kit_core::motion;

/// The open "⋯" menu: the repo it opened on, because a moved selection closes
/// it and it must never act on another one, and whether Remove took its first
/// click.
pub struct RepoMenu {
    pub folder: String,
    pub remove_armed: bool,
}

/// What the detail head needs beyond the repo.
pub struct HeadState {
    /// The open tab holds no action of its own, so the launch is the primary.
    pub filled: bool,
    /// The repo already has a tab; the button goes to it.
    pub tabbed: bool,
    /// The repo is on screen as a tab, so there is nothing left to open.
    pub in_tab: bool,
    pub launch_menu_open: bool,
    /// Bumped by each press on the star, so only a press makes it jump.
    pub star_press: Option<u64>,
}

/// The Overview tab for the selected repo: what needs a person, where to open
/// it, then its status and what GitHub says about it, as cards in two columns
/// that stack when the pane gets narrow.
pub fn render(
    store: &Store,
    entities: &Entity<Store>,
    page: &Entity<ReposPage>,
    tag_draft: Option<&Entity<InputState>>,
) -> AnyElement {
    let body = div()
        .id("overview")
        .debug_selector(|| "overview".into())
        .size_full()
        .overflow_y_scroll();
    let Some(repo) = store.selected_entry() else {
        return body.child(empty_state()).into_any_element();
    };
    let folder = repo.folder_path.to_string_lossy().to_string();
    let settings = store.settings();
    let launchers = launchers::launchers(&settings, repo);
    // The head owns the primary launch; the card lists every other way in.
    let others = launchers.get(1..).unwrap_or_default();
    let left = div()
        .flex_1()
        .min_w(px(320.))
        .flex()
        .flex_col()
        .gap_4()
        .child(needs_you_section(store, repo, &settings, entities))
        .when(!others.is_empty(), |column| {
            column.child(open_in_section(others, &folder, entities))
        });
    let right = div()
        .flex_1()
        .min_w(px(320.))
        .flex()
        .flex_col()
        .gap_4()
        .child(status_section(
            repo, page, tag_draft, entities, &folder, store,
        ))
        .children(details_section(store));
    body.child(
        div()
            .max_w(px(warpui_kit_core::shell::GRID_WIDTH))
            .p(px(warpui_kit_core::shell::DETAIL_PADDING))
            .flex()
            .flex_wrap()
            .items_start()
            .gap_4()
            .child(left)
            .child(right),
    )
    .into_any_element()
}

pub(crate) fn empty_state() -> AnyElement {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(theme::font_meta()))
        .text_color(theme::TEXT_MUTED)
        .child("No repositories yet")
        .into_any_element()
}

/// A square quiet button of the head.
fn head_icon(id: &'static str, label: &'static str) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .a11y_button(label)
        .cursor_pointer()
        .flex_shrink_0()
        .size(px(warpui_kit_core::shell::CONTROL_HEIGHT))
        .rounded(px(6.))
        .border_1()
        .border_color(theme::LINE_STRONG)
        .bg(theme::PANEL)
        .flex()
        .items_center()
        .justify_center()
        .hover(|d| d.bg(theme::RAISED))
}

/// The head's top row: star, name, then the verbs. The open tab owns the one
/// filled action, so Open in VS Code only fills on Overview.
pub(crate) fn header(
    repo: &RepoEntry,
    launchers: &[Launcher],
    state: &HeadState,
    entities: &Entity<Store>,
    page: &Entity<ReposPage>,
    folder: &str,
) -> AnyElement {
    let favorite = repo.is_favorite();
    let star_press = state.star_press;
    let star_target = entities.clone();
    let star_folder = folder.to_string();
    let tab_target = entities.clone();
    let tab_folder = folder.to_string();
    div()
        .flex()
        .items_center()
        .gap(px(10.))
        .min_h(px(36.))
        .child(
            div()
                .id("insp-star")
                .debug_selector(|| "insp-star".into())
                .a11y_button(if favorite {
                    "Remove from favorites"
                } else {
                    "Add to favorites"
                })
                .cursor_pointer()
                .flex_shrink_0()
                .size(px(24.))
                .rounded(px(4.))
                .flex()
                .items_center()
                .justify_center()
                .hover(|d| d.bg(theme::RAISED))
                .on_click(move |_, _, cx| {
                    star_target.update(cx, |s, cx| s.toggle_favorite(&star_folder, cx));
                })
                .child(match star_press.filter(|_| favorite) {
                    Some(press) => motion::pop(
                        SharedString::from(format!("insp-star-pop-{press}")),
                        icon(
                            "icons/icon-star-filled.svg",
                            px(theme::icon_medium()),
                            theme::ACCENT_INK,
                        ),
                    ),
                    None => icon(
                        if favorite {
                            "icons/icon-star-filled.svg"
                        } else {
                            "icons/icon-star.svg"
                        },
                        px(theme::icon_medium()),
                        if favorite {
                            theme::ACCENT_INK
                        } else {
                            theme::STAR_MUTED
                        },
                    )
                    .into_any_element(),
                }),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(px(theme::font_screen_title()))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::TEXT)
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(repo.name.clone()),
        )
        .when(!state.in_tab, |row| {
            let label = if state.tabbed {
                "Go to tab"
            } else {
                "Open in tab"
            };
            row.child(
                div()
                    .id("detail-open-tab")
                    .debug_selector(|| "detail-open-tab".into())
                    .a11y_button(label)
                    .cursor_pointer()
                    .flex_shrink_0()
                    .h(px(warpui_kit_core::shell::CONTROL_HEIGHT))
                    .px_3()
                    .rounded(px(6.))
                    .border_1()
                    .border_color(theme::LINE_STRONG)
                    .bg(theme::PANEL)
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_size(px(theme::font_control()))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme::TEXT)
                    .hover(|d| d.bg(theme::RAISED))
                    .on_click(move |_, _, cx| {
                        tab_target.update(cx, |s, cx| s.open_repo_tab(&tab_folder, cx));
                    })
                    .child(icon(
                        if state.tabbed {
                            "icons/icon-tab.svg"
                        } else {
                            "icons/icon-tab-plus.svg"
                        },
                        px(theme::icon_small()),
                        theme::TEXT_DIM,
                    ))
                    .child(label),
            )
        })
        .children(
            launchers
                .first()
                .map(|primary| primary_action(primary, state.filled, folder, entities)),
        )
        .when(launchers.len() > 1, |row| {
            row.child(launch_toggle(state.filled, page))
        })
        .child(
            head_icon("insp-more", "Repository menu")
                // keep the window's outside-click dismissal from seeing this
                // press: it must toggle, not close-then-reopen
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click({
                    let target = page.clone();
                    let path = folder.to_string();
                    move |_, _, cx| {
                        target.update(cx, |this, cx| this.toggle_repo_menu(&path, cx));
                    }
                })
                .child(icon(
                    "icons/icon-ellipsis-vertical.svg",
                    px(theme::icon_small()),
                    theme::TEXT_DIM,
                )),
        )
        .into_any_element()
}

/// The primary's split half: every other launcher.
fn launch_toggle(filled: bool, page: &Entity<ReposPage>) -> AnyElement {
    let target = page.clone();
    let (fill, hover, ink, edge) = if filled {
        (
            theme::ACCENT,
            theme::ACCENT_HOVER,
            theme::ON_ACCENT,
            theme::ACCENT,
        )
    } else {
        (
            theme::PANEL,
            theme::RAISED,
            theme::TEXT_DIM,
            theme::LINE_STRONG,
        )
    };
    div()
        .id("insp-launch-menu")
        .debug_selector(|| "insp-launch-menu".into())
        .a11y_button("Other apps")
        .cursor_pointer()
        .flex_shrink_0()
        .ml(px(-10.))
        .w(px(30.))
        .h(px(warpui_kit_core::shell::CONTROL_HEIGHT))
        .rounded_r(px(6.))
        .border_1()
        .border_l_0()
        .border_color(edge)
        .bg(fill)
        .flex()
        .items_center()
        .justify_center()
        .hover(move |d| d.bg(hover))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(move |_, _, cx| {
            target.update(cx, |this, cx| this.toggle_launch_menu(cx));
        })
        .child(icon(
            "icons/icon-chevron-down.svg",
            px(theme::icon_tiny()),
            ink,
        ))
        .into_any_element()
}

/// The split button's menu: every launcher with its key.
pub(crate) fn launch_menu(
    launchers: &[Launcher],
    folder: &str,
    entities: &Entity<Store>,
    page: &Entity<ReposPage>,
) -> AnyElement {
    let mut menu = menu_panel("insp-launch-list");
    for launcher in launchers {
        let target = entities.clone();
        let close = page.clone();
        let path = folder.to_string();
        let launch = launcher.launch.clone();
        menu = menu.child(
            div()
                .id(SharedString::from(format!("launch-{}", launcher.label)))
                .a11y_menu_item(launcher.label)
                .cursor_pointer()
                .h(px(32.))
                .px_2()
                .rounded(px(6.))
                .flex()
                .items_center()
                .gap_2()
                .text_size(px(theme::font_control()))
                .text_color(theme::TEXT)
                .hover(|d| d.bg(theme::LINE_STRONG))
                .on_click(move |_, _, cx| {
                    launch.run(&path, &target, cx);
                    close.update(cx, |this, cx| this.close_popups(cx));
                })
                .child(icon(
                    launcher.icon,
                    px(theme::icon_small()),
                    theme::TEXT_DIM,
                ))
                .child(div().flex_1().child(launcher.label))
                .children(
                    launcher
                        .key
                        .map(|key| keycap(key, theme::TEXT_DIM, theme::KEY_BORDER)),
                ),
        );
    }
    menu.into_any_element()
}

/// A menu's panel, hanging from the detail head's top right corner.
fn menu_panel(id: &'static str) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .absolute()
        .top(px(56.))
        .right(px(warpui_kit_core::shell::DETAIL_PADDING))
        .w(px(300.))
        .p(px(4.))
        .rounded(px(8.))
        .border_1()
        .border_color(theme::LINE_STRONG)
        .bg(theme::OVERLAY)
        .shadow_lg()
        // presses inside must not reach the pane under it, or the window's
        // outside-click dismissal before the row's own click lands
        .occlude()
        .flex()
        .flex_col()
        .gap(px(2.))
}

type MenuAction = Box<dyn Fn(&mut App)>;

/// The header's "⋯" menu: the repo verbs, in a popup hanging from the header.
pub(crate) fn popup_menu(
    repo: &RepoEntry,
    state: &RepoMenu,
    store: &Store,
    entities: &Entity<Store>,
    page: &Entity<ReposPage>,
    folder: &str,
) -> AnyElement {
    let archived = repo.is_archived();
    let mut menu = menu_panel("insp-repo-menu");

    // every row closes the menu behind its action
    let item = |id: &'static str, label: &'static str, action: MenuAction| {
        let close = page.clone();
        menu_row(id, label).on_click(move |_, _, cx| {
            action(cx);
            close.update(cx, |this, cx| this.close_repo_menu(cx));
        })
    };

    if let Some(url) = store.selected_github_url() {
        let target = entities.clone();
        let page_url = url.clone();
        menu = menu.child(item(
            "insp-menu-github",
            "Open in GitHub",
            Box::new(move |cx| {
                if let Err(err) = terminal::open_url(&page_url) {
                    target.update(cx, |s, _| s.notify_error(err));
                }
            }),
        ));
        let target = entities.clone();
        menu = menu.child(item(
            "insp-menu-copy-url",
            "Copy GitHub URL",
            Box::new(move |cx| copy_with_toast(url.clone(), "GitHub URL copied", &target, cx)),
        ));
    }
    {
        let target = entities.clone();
        let path = folder.to_string();
        menu = menu.child(item(
            "insp-menu-copy-path",
            "Copy folder path",
            Box::new(move |cx| copy_with_toast(path.clone(), "Folder path copied", &target, cx)),
        ));
    }
    {
        let target = entities.clone();
        menu = menu.child(item(
            "insp-menu-refresh",
            "Refresh repository",
            Box::new(move |cx| target.update(cx, |s, cx| s.refresh_panel(cx))),
        ));
    }
    {
        let target = entities.clone();
        let path = folder.to_string();
        let name = repo.name.clone();
        menu = menu.child(item(
            "insp-menu-archive",
            if archived { "Restore" } else { "Archive" },
            Box::new(move |cx| {
                target.update(cx, |s, cx| {
                    s.set_repo_archived(&path, !archived, cx);
                    s.notify_success(if archived {
                        format!("Restored {name}")
                    } else {
                        format!("Archived {name}")
                    });
                    cx.notify();
                });
            }),
        ));
    }
    menu.child(div().h(px(1.)).mx(px(4.)).my(px(2.)).bg(theme::HAIRLINE))
        .child(remove_row(state.remove_armed, entities, page))
        .into_any_element()
}

fn menu_row(id: &'static str, label: &'static str) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .a11y_button(label)
        .cursor_pointer()
        .rounded(px(6.))
        .px_2()
        .h(px(32.))
        .flex()
        .items_center()
        .text_size(px(theme::font_control()))
        .text_color(theme::TEXT)
        .hover(|d| d.bg(theme::LINE_STRONG))
        .child(label)
}

/// Destructive, so it takes two clicks: the first arms the row, the second
/// removes. Closing the menu disarms it, whatever closed it.
fn remove_row(armed: bool, entities: &Entity<Store>, page: &Entity<ReposPage>) -> AnyElement {
    let target = entities.clone();
    let menu = page.clone();
    div()
        .id("insp-menu-remove")
        .debug_selector(|| "insp-menu-remove".into())
        .a11y_button(if armed {
            "Confirm remove from repositories"
        } else {
            "Remove from repositories"
        })
        .cursor_pointer()
        .rounded(px(6.))
        .px_2()
        .py_1p5()
        .flex()
        .flex_col()
        .gap_0p5()
        .text_size(px(theme::font_control()))
        .text_color(if armed {
            theme::DANGER
        } else {
            theme::TEXT_DIM
        })
        .when(armed, |row| row.bg(theme::tint(theme::DANGER)))
        .hover(|d| d.bg(theme::tint(theme::DANGER)).text_color(theme::DANGER))
        .on_click(move |_, _, cx| {
            if armed {
                target.update(cx, |s, cx| s.remove_from_repositories(cx));
                menu.update(cx, |this, cx| this.close_repo_menu(cx));
            } else {
                menu.update(cx, |this, cx| this.arm_repo_remove(cx));
            }
        })
        .child(if armed {
            "Remove from repositories?"
        } else {
            "Remove from repositories"
        })
        .when(armed, |row| {
            row.child(
                div()
                    .text_size(px(theme::font_mono_meta()))
                    .child("Click again to confirm"),
            )
        })
        .into_any_element()
}

/// Copies `text` and confirms it with a toast.
fn copy_with_toast(text: String, toast: &'static str, store: &Entity<Store>, cx: &mut App) {
    cx.write_to_clipboard(ClipboardItem::new_string(text));
    store.update(cx, |s, cx| {
        s.notify_success(toast);
        cx.notify();
    });
}

/// `~/developments/foo` rather than the absolute path: the home prefix carries
/// no information.
pub(crate) fn display_path(repo: &RepoEntry) -> String {
    let path = repo.folder_path.to_string_lossy().to_string();
    for key in ["HOME", "USERPROFILE"] {
        let Some(home) = std::env::var_os(key) else {
            continue;
        };
        let home = home.to_string_lossy();
        if let Some(rest) = path.strip_prefix(home.as_ref()) {
            return format!("~{rest}");
        }
    }
    path
}

fn tag_row(
    repo: &RepoEntry,
    page: &Entity<ReposPage>,
    tag_draft: Option<&Entity<InputState>>,
    entities: &Entity<Store>,
    folder: &str,
) -> AnyElement {
    let mut row = div()
        .flex()
        .items_center()
        .flex_wrap()
        .gap(px(6.))
        .min_h(px(24.));
    for tag in repo.tags.iter().filter(|tag| !is_system_tag(tag)) {
        row = row.child(tag_chip(tag, entities, folder));
    }
    if let Some(input) = tag_draft {
        row = row.child(
            div()
                .w(px(120.))
                .child(Input::new(input).with_size(theme::field_size())),
        );
    } else {
        let target = page.clone();
        row = row.child(
            div()
                .id("insp-add-tag")
                .debug_selector(|| "insp-add-tag".into())
                .a11y_button("Add tag")
                .cursor_pointer()
                .h(px(24.))
                .flex()
                .items_center()
                .text_size(px(theme::font_control()))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme::ACCENT_INK)
                .on_click(move |_, _, cx| {
                    target.update(cx, |this, cx| this.begin_tag_edit(cx));
                })
                .child("Add tag"),
        );
    }
    row.into_any_element()
}

/// A user tag. The × stays quiet until the chip is hovered, but its target
/// runs the full chip height.
fn tag_chip(tag: &str, entities: &Entity<Store>, folder: &str) -> AnyElement {
    let target = entities.clone();
    let path = folder.to_string();
    let name = tag.to_string();
    let selector = format!("insp-tag-remove-{tag}");
    div()
        .group("insp-tag")
        .h(px(24.))
        .pl(px(8.))
        .flex()
        .items_center()
        .rounded(px(12.))
        .bg(theme::RAISED)
        .text_size(px(theme::font_row_meta()))
        .text_color(theme::TEXT_DIM)
        .child(SharedString::from(tag.to_string()))
        .child(
            div()
                .id(SharedString::from(selector.clone()))
                .debug_selector(move || selector.clone())
                .a11y_button(format!("Remove tag {tag}"))
                .cursor_pointer()
                .w(px(18.))
                .h_full()
                .flex()
                .items_center()
                .justify_center()
                .text_color(theme::TEXT_MUTED)
                .opacity(0.45)
                .group_hover("insp-tag", |d| d.opacity(1.))
                .hover(|d| d.text_color(theme::TEXT))
                .on_click(move |_, _, cx| {
                    target.update(cx, |s, cx| s.toggle_repo_tag(&path, &name, cx));
                })
                .child("×"),
        )
        .into_any_element()
}

/// `favorites` and `archived` are derived flags, not user tags — showing them
/// as removable chips would invite removing them.
fn is_system_tag(tag: &str) -> bool {
    tag.eq_ignore_ascii_case(warp_station_core::repo_store::FAVORITES_TAG)
        || tag.eq_ignore_ascii_case(warp_station_core::repo_store::ARCHIVED_TAG)
}

/// One label and value of a card.
fn fact(label: &'static str, value: impl IntoElement) -> gpui::Div {
    div()
        .mx_4()
        .min_h(px(34.))
        .flex()
        .items_center()
        .gap_3()
        .child(
            div()
                .w(px(112.))
                .flex_shrink_0()
                .text_size(px(theme::font_meta()))
                .text_color(theme::TEXT_DIM)
                .child(label),
        )
        .child(div().flex_1().min_w_0().child(value))
}

/// Branch, how far it is from the remote, HEAD and the tags. The syncing
/// verbs live in Needs you, where the counts that ask for them are.
fn status_section(
    repo: &RepoEntry,
    page: &Entity<ReposPage>,
    tag_draft: Option<&Entity<InputState>>,
    entities: &Entity<Store>,
    folder: &str,
    store: &Store,
) -> AnyElement {
    let branch = repo
        .git_branch_name
        .clone()
        .unwrap_or_else(|| "detached".into());
    let ahead = repo.git_to_push_count;
    let behind = repo.git_to_pull_count;
    let fetched = repo
        .git_last_fetch_at
        .map(rel_time::format)
        .unwrap_or_else(|| "never fetched".into());

    let branch_line = div()
        .id("insp-branch")
        .debug_selector(|| "insp-branch".into())
        .flex()
        .items_center()
        .gap(px(5.))
        .font_family(theme::code_font())
        .text_size(px(theme::font_mono_meta()))
        .text_color(theme::TEXT_DIM)
        .when_some(repo.git_branch_name.clone(), |line, name| {
            let target = entities.clone();
            line.a11y_button(format!("Copy branch name {name}"))
                .cursor_pointer()
                .hover(|d| d.text_color(theme::TEXT))
                .on_click(move |_, _, cx| {
                    copy_with_toast(name.clone(), "Branch name copied", &target, cx);
                })
        })
        .child(icon(
            "icons/icon-source-branch.svg",
            px(theme::icon_tiny()),
            theme::BRANCH,
        ))
        .child(SharedString::from(branch));

    let sync_state = |path: &'static str, count: u32, word: &'static str, tint: Rgba| {
        div()
            .flex()
            .items_center()
            .gap(px(5.))
            .text_size(px(theme::font_meta()))
            .font_weight(FontWeight::MEDIUM)
            .text_color(tint)
            .child(icon(path, px(theme::icon_tiny()), tint))
            .child(format!("{count} {word}"))
    };
    let sync = div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(10.))
        .when(!repo.git_has_remote, |row| {
            row.child(
                div()
                    .text_size(px(theme::font_meta()))
                    .text_color(theme::TEXT_MUTED)
                    .child("No remote"),
            )
        })
        .when(repo.git_has_remote && !repo.git_branch_published, |row| {
            row.child(
                div()
                    .text_size(px(theme::font_meta()))
                    .text_color(theme::TEXT_MUTED)
                    .child("Not published"),
            )
        })
        .when(repo.git_has_remote && repo.git_branch_published, |row| {
            row.when(behind > 0, |row| {
                row.child(sync_state(
                    "icons/icon-arrow-down.svg",
                    behind,
                    "to pull",
                    theme::BEHIND,
                ))
            })
            .when(ahead > 0, |row| {
                row.child(sync_state(
                    "icons/icon-arrow-up.svg",
                    ahead,
                    "to push",
                    theme::AHEAD,
                ))
            })
            .when(ahead == 0 && behind == 0, |row| {
                row.child(
                    div()
                        .text_size(px(theme::font_meta()))
                        .text_color(theme::SUCCESS_BRIGHT)
                        .child("in sync"),
                )
            })
        })
        .child(
            div()
                .text_size(px(theme::font_meta()))
                .text_color(theme::TEXT_MUTED)
                .child(format!("fetched {fetched}")),
        );

    // HEAD's subject once the changes pass has answered; the probe's cached
    // commit time is the honest fallback before that.
    let last_commit = store
        .recent_commits
        .first()
        .map(|commit| commit.subject.clone())
        .or_else(|| repo.git_last_commit_at.map(rel_time::format))
        .unwrap_or_else(|| "—".into());
    let last_commit_line = div()
        .min_w_0()
        .text_size(px(theme::font_body()))
        .text_color(theme::TEXT)
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis()
        .child(last_commit);

    card(section_title("Status".to_string()))
        .child(fact("Branch", branch_line))
        .child(fact("Sync", sync))
        .child(fact("Last commit", last_commit_line))
        .child(fact(
            "Tags",
            tag_row(repo, page, tag_draft, entities, folder),
        ))
        .into_any_element()
}

/// The first launcher, the default editor unless the settings turned it off:
/// the head's split button, filled when the open tab has no action of its own.
fn primary_action(
    launcher: &Launcher,
    filled: bool,
    folder: &str,
    entities: &Entity<Store>,
) -> AnyElement {
    let target = entities.clone();
    let path = folder.to_string();
    let launch = launcher.launch.clone();
    let (fill, hover, ink, edge) = if filled {
        (
            theme::ACCENT,
            theme::ACCENT_HOVER,
            theme::ON_ACCENT,
            theme::ACCENT,
        )
    } else {
        (theme::PANEL, theme::RAISED, theme::TEXT, theme::LINE_STRONG)
    };
    div()
        .id("insp-primary")
        .debug_selector(|| "insp-primary".into())
        .a11y_button(launcher.label)
        .cursor_pointer()
        .flex_shrink_0()
        .h(px(warpui_kit_core::shell::CONTROL_HEIGHT))
        .px_3()
        .rounded_l(px(6.))
        .border_1()
        .border_color(edge)
        .bg(fill)
        .flex()
        .items_center()
        .gap_2()
        .text_size(px(theme::font_control()))
        .font_weight(if filled {
            FontWeight::SEMIBOLD
        } else {
            FontWeight::MEDIUM
        })
        .text_color(ink)
        .hover(move |el| el.bg(hover))
        .on_click(move |_, _, cx| launch.run(&path, &target, cx))
        .child(icon(launcher.icon, px(theme::icon_small()), ink))
        .child(launcher.label)
        .into_any_element()
}

/// Every other way to open the repo, keys included.
fn open_in_section(launchers: &[Launcher], folder: &str, entities: &Entity<Store>) -> AnyElement {
    let mut grid = div().mx_4().my_3().flex().flex_wrap().gap_2();
    for launcher in launchers {
        let target = entities.clone();
        let path = folder.to_string();
        let launch = launcher.launch.clone();
        grid = grid.child(
            div()
                .id(SharedString::from(format!(
                    "insp-launch-{}",
                    launcher.label
                )))
                .a11y_button(launcher.label)
                .cursor_pointer()
                .flex_1()
                .min_w(px(150.))
                .h(px(warpui_kit_core::shell::CONTROL_HEIGHT))
                .px_2p5()
                .rounded(px(6.))
                .border_1()
                .border_color(theme::LINE_STRONG)
                .bg(theme::PANEL)
                .flex()
                .items_center()
                .gap_1p5()
                .text_size(px(theme::font_control()))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme::TEXT)
                .hover(|el| el.bg(theme::RAISED))
                .on_click(move |_, _, cx| launch.run(&path, &target, cx))
                .child(icon(
                    launcher.icon,
                    px(theme::icon_small()),
                    theme::TEXT_DIM,
                ))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(launcher.short),
                )
                .children(
                    launcher
                        .key
                        .map(|key| keycap(key, theme::TEXT_DIM, theme::KEY_BORDER)),
                ),
        );
    }
    card(section_title("Open in".to_string()))
        .child(grid)
        .into_any_element()
}

/// A click handler for a control in a card.
type Action = Box<dyn Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App)>;

/// Goes to `tab`.
fn open_tab(entities: &Entity<Store>, tab: WorkspaceTab) -> Action {
    let target = entities.clone();
    Box::new(move |_, _, cx| target.update(cx, |s, cx| s.open_detail_tab(tab, cx)))
}

/// A card: the base surface on the detail's panel, its head on top.
fn card(head: gpui::Div) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .pb_2()
        .rounded(px(8.))
        .border_1()
        .border_color(theme::HAIRLINE)
        .bg(theme::SURFACE)
        .child(head)
}

/// A card's head: its title in sentence case.
fn section_title(label: String) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap_3()
        .min_h(px(44.))
        .px_4()
        .mb_1()
        .border_b_1()
        .border_color(theme::BORDER_LIGHT)
        .child(
            div()
                .text_size(px(theme::font_name()))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::TEXT)
                .child(label),
        )
}

/// The Overview's first card: one row per thing that wants a person, and
/// nothing else. A tree that is clean and in sync says so instead.
///
/// The counts come from the repo entry immediately; the file split and the
/// latest GitHub item need the passes' answers, so a count row stands in until
/// they land rather than claiming the list is empty.
fn needs_you_section(
    store: &Store,
    repo: &RepoEntry,
    settings: &AppSettings,
    entities: &Entity<Store>,
) -> AnyElement {
    let folder = repo.folder_path.to_string_lossy();
    let loaded = store.changes_loaded_for.as_deref() == Some(folder.as_ref());
    let groups = if loaded {
        store.changes().map(|snapshot| snapshot.change_groups())
    } else {
        None
    };
    let (unstaged, staged) = groups
        .as_ref()
        .map(|(unstaged, staged)| (unstaged.len(), staged.len()))
        .unwrap_or((0, 0));
    let github = settings.repos.enable_git_hub;
    let snapshot = github
        .then(|| store.github_by_folder.get(folder.as_ref()))
        .flatten();
    let azure = store
        .azure_by_folder
        .get(folder.as_ref())
        .filter(|snapshot| snapshot.available);
    let job = store.git_jobs.get(folder.as_ref()).copied();
    let finished = store.finished_job();
    let branch = repo.git_branch_name.clone().unwrap_or_default();

    let mut rows = div().flex().flex_col();
    let mut any = false;
    let next = |row: AnyElement, any: &mut bool| {
        let first = !*any;
        *any = true;
        div()
            .when(!first, |row| {
                row.border_t_1().border_color(theme::BORDER_LIGHT)
            })
            .child(row)
    };

    if repo.git_modified_count > 0 {
        let sub = if groups.is_some() {
            format!("{staged} staged · {unstaged} unstaged")
        } else {
            "Review the working tree".to_string()
        };
        rows = rows.child(next(
            needy_row(
                "insp-uncommitted",
                "icons/icon-file-diff.svg",
                theme::CHANGES,
                format!(
                    "{} uncommitted {}",
                    repo.git_modified_count,
                    count_word(repo.git_modified_count, "file", "files")
                ),
                sub,
                needy_link(
                    "insp-commit",
                    "Review",
                    open_tab(entities, WorkspaceTab::Changes),
                ),
            ),
            &mut any,
        ));
    }
    if repo.git_to_pull_count > 0 {
        let sub = match repo.git_last_fetch_at {
            Some(at) => format!("Fetched {}", rel_time::format(at)),
            None => format!("{branch} has commits the local branch does not"),
        };
        rows = rows.child(next(
            needy_row(
                "insp-behind",
                "icons/icon-arrow-down.svg",
                theme::BEHIND,
                format!(
                    "{} {} to pull",
                    repo.git_to_pull_count,
                    count_word(repo.git_to_pull_count, "commit", "commits")
                ),
                sub,
                needy_job_button(
                    "insp-pull",
                    "Pull",
                    GitAction::Pull,
                    "pull",
                    entities,
                    job,
                    finished,
                ),
            ),
            &mut any,
        ));
    }
    if repo.git_to_push_count > 0 {
        rows = rows.child(next(
            needy_row(
                "insp-ahead",
                "icons/icon-arrow-up.svg",
                theme::AHEAD,
                format!(
                    "{} {} to push",
                    repo.git_to_push_count,
                    count_word(repo.git_to_push_count, "commit", "commits")
                ),
                "Not on the remote yet".to_string(),
                needy_job_button(
                    "insp-push",
                    "Push",
                    GitAction::Push,
                    "push",
                    entities,
                    job,
                    finished,
                ),
            ),
            &mut any,
        ));
    }
    if let Some(pull) = snapshot.and_then(|snapshot| snapshot.pulls.first()) {
        let mut bits = Vec::new();
        match (pull.head_ref_name.as_deref(), pull.base_ref_name.as_deref()) {
            (Some(head), Some(base)) if !head.is_empty() && !base.is_empty() => {
                bits.push(format!("{head} → {base}"));
            }
            _ => {}
        }
        if let Some(age) = pull
            .updated_at
            .as_deref()
            .and_then(rel_time::format_rfc3339)
        {
            bits.push(format!("updated {age}"));
        }
        let item = pull.clone();
        let target = entities.clone();
        rows = rows.child(next(
            needy_row(
                &format!("insp-pr-{}", pull.number),
                "icons/icon-git-pull-request.svg",
                theme::PR,
                format!("#{} {}", pull.number, pull.title),
                bits.join(" · "),
                needy_link("insp-open-pr", "Open", {
                    Box::new(move |_, _, cx| {
                        target.update(cx, |s, cx| s.open_pull_request(item.clone(), cx));
                    })
                }),
            ),
            &mut any,
        ));
    } else if github && repo.github_pr_count > 0 {
        rows = rows.child(next(
            needy_row(
                "insp-pulls-unloaded",
                "icons/icon-git-pull-request.svg",
                theme::PR,
                format!(
                    "{} open {}",
                    repo.github_pr_count,
                    count_word(repo.github_pr_count, "pull request", "pull requests")
                ),
                "Waiting for the GitHub pass".to_string(),
                needy_link(
                    "insp-open-prs",
                    "Open",
                    open_tab(entities, WorkspaceTab::PullRequests),
                ),
            ),
            &mut any,
        ));
    }
    if let Some(issue) = snapshot.and_then(|snapshot| snapshot.issues.first()) {
        let mut bits = issue.labels.clone();
        if let Some(age) = issue
            .updated_at
            .as_deref()
            .and_then(rel_time::format_rfc3339)
        {
            bits.push(age);
        }
        rows = rows.child(next(
            needy_row(
                &format!("insp-issue-{}", issue.number),
                "icons/icon-circle-dot.svg",
                theme::ISSUE,
                format!("#{} {}", issue.number, issue.title),
                bits.join(" · "),
                needy_link(
                    "insp-github-open",
                    "Open",
                    open_tab(entities, WorkspaceTab::Issues),
                ),
            ),
            &mut any,
        ));
    } else if github && repo.github_issue_count > 0 {
        rows = rows.child(next(
            needy_row(
                "insp-issues-unloaded",
                "icons/icon-circle-dot.svg",
                theme::ISSUE,
                format!(
                    "{} open {}",
                    repo.github_issue_count,
                    count_word(repo.github_issue_count, "issue", "issues")
                ),
                "Waiting for the GitHub pass".to_string(),
                needy_link(
                    "insp-open-issues",
                    "Open",
                    open_tab(entities, WorkspaceTab::Issues),
                ),
            ),
            &mut any,
        ));
    }
    if let Some(snapshot) = azure.filter(|snapshot| snapshot.pipeline_failed())
        && let Some(run) = snapshot.pipelines.first()
    {
        let sub = [run.definition_name.clone(), Some(run.build_number.clone())]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · ");
        rows = rows.child(next(
            needy_row(
                "insp-azure-row",
                "icons/icon-close.svg",
                theme::DANGER,
                "Latest run failed".to_string(),
                sub,
                needy_link(
                    "insp-azure-open",
                    "Open",
                    open_tab(entities, WorkspaceTab::Pipelines),
                ),
            ),
            &mut any,
        ));
    }

    if !any {
        rows = rows.child(
            div()
                .debug_selector(|| "insp-needs-clean".into())
                .mx_4()
                .py_3()
                .flex()
                .items_center()
                .gap_3()
                .child(icon(
                    "icons/icon-check.svg",
                    px(theme::icon_small()),
                    theme::SUCCESS_BRIGHT,
                ))
                .child(
                    div()
                        .text_size(px(theme::font_body()))
                        .text_color(theme::TEXT_DIM)
                        .child("Working tree clean and in sync. Nothing to do here."),
                ),
        );
    }

    card(section_title("Needs you".to_string()))
        .child(rows)
        .into_any_element()
}

/// One Needs-you row: the entity's glyph, what it is and why, then its verb.
fn needy_row(
    id: &str,
    icon_path: &'static str,
    tint: Rgba,
    title: String,
    sub: String,
    action: AnyElement,
) -> AnyElement {
    let selector = id.to_string();
    div()
        .debug_selector(move || selector.clone())
        .mx_4()
        .py_2()
        .min_h(px(46.))
        .flex()
        .items_center()
        .gap_3()
        .child(icon(icon_path, px(theme::icon_small()), tint))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(
                    div()
                        .min_w_0()
                        .text_size(px(theme::font_body()))
                        .text_color(theme::TEXT)
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(title),
                )
                .when(!sub.is_empty(), |column| {
                    column.child(
                        div()
                            .min_w_0()
                            .text_size(px(theme::font_meta()))
                            .text_color(theme::TEXT_MUTED)
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(sub),
                    )
                }),
        )
        .child(action)
        .into_any_element()
}

/// The row's quiet verb: Review, Open.
fn needy_link(id: &'static str, label: &'static str, on_click: Action) -> AnyElement {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .a11y_button(label)
        .cursor_pointer()
        .flex_shrink_0()
        .h(px(26.))
        .px_2p5()
        .flex()
        .items_center()
        .rounded(px(6.))
        .text_size(px(theme::font_control()))
        .font_weight(FontWeight::MEDIUM)
        .text_color(theme::ACCENT_INK)
        .hover(|el| el.bg(theme::RAISED).text_color(theme::ACCENT_HOVER))
        .on_click(on_click)
        .child(label)
        .into_any_element()
}

/// The row's git verb: a button that says what it is doing while the job runs
/// and what it did for a moment after.
fn needy_job_button(
    id: &'static str,
    idle: &'static str,
    action: GitAction,
    job_label: &'static str,
    entities: &Entity<Store>,
    job: Option<&'static str>,
    finished: Option<&'static str>,
) -> AnyElement {
    let running = job == Some(job_label);
    let busy = job.is_some() && !running;
    let done = job.is_none() && finished == Some(job_label);
    let label = if running {
        crate::net::git_working_word(job_label)
    } else if done {
        crate::net::git_done_word(job_label)
    } else {
        idle
    };
    let target = entities.clone();
    div()
        .id(id)
        .debug_selector(move || id.into())
        .a11y_button(label)
        .flex_shrink_0()
        .h(px(30.))
        .px_2p5()
        .rounded(px(6.))
        .border_1()
        .border_color(if done {
            theme::tint(theme::SUCCESS_BRIGHT)
        } else {
            theme::LINE_STRONG.into()
        })
        .flex()
        .items_center()
        .gap_1p5()
        .text_size(px(theme::font_control()))
        .font_weight(FontWeight::MEDIUM)
        .when(running, |d| {
            d.bg(theme::PANEL)
                .text_color(theme::TEXT_MUTED)
                .child(motion::spinner(
                    SharedString::from(format!("{id}-spin")),
                    "icons/icon-spinner.svg",
                    px(theme::icon_tiny()),
                    theme::TEXT_MUTED,
                ))
        })
        .when(done, |d| {
            d.bg(theme::PANEL)
                .text_color(theme::SUCCESS_BRIGHT)
                .child(icon(
                    "icons/icon-check.svg",
                    px(theme::icon_tiny()),
                    theme::SUCCESS_BRIGHT,
                ))
        })
        .when(!running && !done, |d| {
            d.bg(theme::PANEL).text_color(theme::TEXT).when(!busy, |d| {
                d.cursor_pointer()
                    .hover(|el| el.bg(theme::RAISED))
                    .on_click(move |_, _, cx| {
                        target.update(cx, |s, cx| s.run_git(action.clone(), cx));
                    })
            })
        })
        .child(label)
        .into_any_element()
}

/// "1 commit" / "2 commits".
fn count_word(count: u32, one: &'static str, many: &'static str) -> &'static str {
    if count == 1 { one } else { many }
}
/// What GitHub says about the repo. Absent until the details request answers, and for repos not on GitHub.
fn details_section(store: &Store) -> Option<AnyElement> {
    let details = store.github_details.as_ref()?;
    let facts = [
        ("Owner", details.owner.clone()),
        ("Language", details.primary_language.clone()),
        ("License", details.license.clone()),
        ("Default", details.default_branch.clone()),
        ("Created", details.created_at.as_deref().map(format_created)),
    ];

    let mut grid = div().flex().flex_col();
    let mut empty = true;
    for (label, value) in facts {
        let Some(value) = value else { continue };
        empty = false;
        grid = grid.child(fact(
            label,
            div()
                .text_size(px(theme::font_body()))
                .text_color(theme::TEXT)
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(value),
        ));
    }
    if empty && details.topics.is_empty() {
        return None;
    }

    let topics =
        div()
            .mx_4()
            .flex()
            .flex_wrap()
            .gap(px(6.))
            .pt_1()
            .children(details.topics.iter().map(|topic| {
                div()
                    .h(px(24.))
                    .px_2()
                    .flex()
                    .items_center()
                    .rounded(px(12.))
                    .bg(theme::RAISED)
                    .text_size(px(theme::font_row_meta()))
                    .text_color(theme::TEXT_DIM)
                    .child(SharedString::from(topic.clone()))
            }));

    Some(
        card(section_title("Details".to_string()))
            .debug_selector(|| "insp-details".into())
            .child(grid)
            .child(topics)
            .into_any_element(),
    )
}

/// gh returns ISO timestamps; the details show "27 Nov 2023".
fn format_created(iso: &str) -> String {
    chrono::NaiveDate::parse_from_str(&iso[..10.min(iso.len())], "%Y-%m-%d")
        .map(|d| d.format("%-d %b %Y").to_string())
        .unwrap_or_else(|_| iso.to_string())
}

fn keycap(label: impl Into<SharedString>, color: Rgba, border: Rgba) -> AnyElement {
    div()
        .flex()
        .items_center()
        .justify_center()
        .h(px(20.))
        .min_w(px(20.))
        .px_1()
        .rounded(px(4.))
        .border_1()
        .border_color(border)
        .font_family(theme::code_font())
        .text_size(px(theme::font_micro()))
        .text_color(color)
        .child(label.into())
        .into_any_element()
}
