//! The repos list, the shell's list pane. Rows are two lines, identity over
//! context, and colour on a row only means state: each entity keeps one hue, so
//! a healthy row stays quiet and a row that needs a person stands out. Folder
//! bands fold their group away.

use crate::store::Store;
use crate::store::projection::ProjectedRow;
use crate::theme;
use crate::ui::A11y;
use crate::ui::icon;
use crate::ui::launchers::{self, Launcher};
use gpui::prelude::*;
use gpui::{Entity, FontWeight, Rgba, ScrollHandle, SharedString, div, px};
use warp_station_core::azure::AzureSnapshot;
use warp_station_core::rel_time;
use warp_station_core::repo_store::RepoEntry;
use warpui_kit_core::motion;

/// The hover group a band's folder glyph swaps its filter mark under.
const BAND_GROUP: &str = "repo-band";

/// What a row shows beyond the entry itself.
struct RowFacts<'a> {
    /// Whether the GitHub integration is on; its counts leave the row if not.
    github: bool,
    azure: Option<&'a AzureSnapshot>,
    /// The workspaces warp-agent is running in this repo, when it has any.
    warp_titles: Option<String>,
    show_tooltips: bool,
    /// The repo has a tab of its own.
    tabbed: bool,
    /// What runs on this repository right now.
    job: Option<&'static str>,
    /// Bumped by each press on this repo's star, so only that press jumps.
    star_press: Option<u64>,
    /// What the row's open button runs.
    primary: Option<Launcher>,
}

/// A clone in flight: the row it will become, until the scan brings the real one.
fn cloning_row(ix: usize, name: &str) -> impl IntoElement + use<> {
    div()
        .debug_selector(move || format!("repo-cloning-{ix}"))
        .flex_shrink_0()
        .h(px(64.))
        .px(px(14.))
        .flex()
        .items_center()
        .gap_2()
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(px(theme::font_name()))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::text_dim())
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(name.to_string()),
        )
        .child(
            div()
                .id(SharedString::from(format!("repo-cloning-status-{ix}")))
                .role(gpui::Role::Status)
                .aria_label("Cloning repository")
                .flex()
                .items_center()
                .child(motion::spinner(
                    SharedString::from(format!("repo-cloning-spinner-{ix}")),
                    "icons/icon-spinner.svg",
                    px(theme::icon_tiny()),
                    theme::text_dim(),
                )),
        )
        .child(
            div()
                .text_size(px(theme::font_meta()))
                .text_color(theme::text_dim())
                .child("Cloning"),
        )
}

/// The whole list: one quiet header per folder group, then its rows. Rows
/// select on click, and the detail pane follows.
pub fn render(
    store: &Store,
    store_entity: &Entity<Store>,
    scroll: &ScrollHandle,
) -> gpui::AnyElement {
    if store.projected.is_empty() {
        return empty_list(store);
    }
    let mut list = div()
        .id("repos-list")
        .flex()
        .flex_col()
        .size_full()
        .min_h_0()
        .overflow_y_scroll()
        // the page's keyboard cursor scrolls a row into view through this
        .track_scroll(scroll)
        .pt_1();
    for (ix, name) in store.cloning.iter().enumerate() {
        list = list.child(cloning_row(ix, name));
    }

    let settings = store.settings();
    let soloed = store.group_filter.as_deref();
    // one set for the whole list: a per-row `repo_tabs` scan is O(tabs) each
    let tabbed: std::collections::HashSet<&str> = store
        .repo_tabs
        .iter()
        .map(|tab| tab.folder.as_str())
        .collect();

    for (row_ix, row) in store.projected.iter().enumerate() {
        match row {
            ProjectedRow::Group {
                key,
                name,
                count,
                collapsed,
            } => {
                let solo = soloed == Some(key.as_str());
                list = list.child(group_band(
                    key,
                    name,
                    *count,
                    *collapsed,
                    solo,
                    store_entity,
                ));
            }
            ProjectedRow::Repo(ix) => {
                if let Some(repo) = store.repos.get(*ix) {
                    let folder = repo.folder_path.to_string_lossy();
                    let selected = store.selected_repo.as_deref() == Some(folder.as_ref());
                    let azure = store
                        .azure_by_folder
                        .get(folder.as_ref())
                        .filter(|_| settings.repos.enable_azure_devops)
                        .map(|snapshot| snapshot.as_ref());
                    let warp_titles = store
                        .warp_running
                        .get(folder.as_ref())
                        .filter(|status| !status.running.is_empty())
                        .map(|status| status.running.join(", "));
                    let facts = RowFacts {
                        github: settings.repos.enable_github,
                        azure,
                        warp_titles,
                        show_tooltips: settings.general.show_tooltips,
                        tabbed: tabbed.contains(folder.as_ref()),
                        job: store.git_jobs.get(folder.as_ref()).copied(),
                        star_press: store
                            .star_pressed
                            .as_ref()
                            .filter(|(pressed, _)| pressed == folder.as_ref())
                            .map(|(_, presses)| *presses),
                        primary: Some(launchers::primary(&settings, repo)),
                    };
                    list = list.child(repo_row(repo, row_ix, selected, &facts, store_entity));
                }
            }
        }
    }

    list.into_any_element()
}

/// Why the list has no rows.
fn empty_list(store: &Store) -> gpui::AnyElement {
    let filtered =
        !store.tag_filters.is_empty() || store.group_filter.is_some() || store.attention_only;
    let has_folders = store
        .settings()
        .repos
        .repo_scan_folders
        .is_some_and(|folders| !folders.is_empty());
    let text = if store.scanning {
        "Scanning folders…"
    } else if filtered {
        "No repositories match the current filter"
    } else if has_folders {
        "No repositories found under the scan folders"
    } else {
        "No scan folders yet — add a repository to start"
    };
    div()
        .debug_selector(|| "repos-empty".into())
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(theme::font_meta()))
        .text_color(theme::text_muted())
        .child(text)
        .into_any_element()
}

/// A folder band: the export's 30px row on its own step of the ramp. Pressing
/// it folds the group; its folder glyph narrows the list to this group, and
/// turns into the filter mark on hover and for as long as it is soloed.
fn group_band(
    key: &str,
    name: &str,
    count: usize,
    collapsed: bool,
    soloed: bool,
    store: &Entity<Store>,
) -> gpui::AnyElement {
    let band_selector = format!("repo-band-{key}");
    let solo_selector = format!("repo-band-solo-{key}");
    let fold_target = store.clone();
    let fold_key = key.to_string();
    let solo_target = store.clone();
    let solo_key = key.to_string();

    let solo = div()
        .id(SharedString::from(format!("repo-band-solo-id-{key}")))
        .debug_selector(move || solo_selector.clone())
        .a11y_toggle("Show only this group", soloed)
        .relative()
        .cursor_pointer()
        .flex_shrink_0()
        .size(px(18.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(4.))
        .hover(|d| d.bg(theme::border_light()))
        .on_click(move |_, _, cx| {
            // the band's own press folds the group
            cx.stop_propagation();
            solo_target.update(cx, |s, cx| s.solo_group(&solo_key, cx));
        })
        .child(
            icon("icons/icon-folder.svg", px(13.), theme::muted_icon())
                .when(soloed, |glyph| glyph.opacity(0.))
                .group_hover(BAND_GROUP, |s| s.opacity(0.)),
        )
        .child(
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    icon(
                        "icons/icon-filter.svg",
                        px(12.),
                        if soloed {
                            theme::blue_bright()
                        } else {
                            theme::muted_icon()
                        },
                    )
                    .when(!soloed, |glyph| glyph.opacity(0.))
                    .group_hover(BAND_GROUP, |s| s.opacity(1.)),
                ),
        );

    div()
        .id(SharedString::from(format!("repo-band-id-{key}")))
        .debug_selector(move || band_selector.clone())
        .group(BAND_GROUP)
        .a11y_button(format!("{name} — {count} repositories"))
        .aria_expanded(!collapsed)
        .cursor_pointer()
        .flex_shrink_0()
        .flex()
        .items_center()
        .gap(px(6.))
        .h(px(34.))
        .px_4()
        .hover(|d| d.bg(theme::panel()))
        .on_click(move |_, _, cx| {
            fold_target.update(cx, |s, cx| s.toggle_group_collapse(&fold_key, cx));
        })
        .child(icon(
            if collapsed {
                "icons/icon-chevron-right.svg"
            } else {
                "icons/icon-chevron-down.svg"
            },
            px(13.),
            theme::muted_icon(),
        ))
        .child(solo)
        .child(
            div()
                .text_size(px(theme::font_sub()))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::text_dim())
                .child(name.to_string()),
        )
        .child(
            div()
                .text_size(px(theme::font_mono_meta()))
                .text_color(theme::text_muted())
                .child(format!("{count}")),
        )
        .into_any_element()
}

/// A row's quiet button, shown while the row is hovered.
fn row_action(
    id: String,
    label: &'static str,
    icon_path: &'static str,
    on_click: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> gpui::AnyElement {
    let selector = id.clone();
    div()
        .id(SharedString::from(id))
        .debug_selector(move || selector.clone())
        .a11y_button(label)
        .cursor_pointer()
        .flex_shrink_0()
        .w(px(26.))
        .h(px(22.))
        .rounded(px(4.))
        .flex()
        .items_center()
        .justify_center()
        .hover(|d| d.bg(theme::overlay()))
        .on_click(on_click)
        .child(icon(icon_path, px(theme::icon_small()), theme::text_dim()))
        .into_any_element()
}

/// The context line: branch, then what is waiting, each in its own hue. Zero
/// counts leave, so a healthy repo reads as branch and clean. `row_ix` names the
/// parts for the probes on a list row; the detail head passes `None`.
pub fn stats(repo: &RepoEntry, github: bool, azure: bool, row_ix: Option<usize>) -> gpui::Div {
    let branch = repo.git_branch_name.clone().unwrap_or_else(|| "—".into());
    let mut meta = div()
        .flex()
        .items_center()
        .gap(px(12.))
        .min_w_0()
        .text_size(px(theme::font_row_meta()))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(5.))
                .min_w_0()
                .child(icon(
                    "icons/icon-source-branch.svg",
                    px(theme::icon_tiny()),
                    theme::BRANCH,
                ))
                .child(
                    div()
                        .min_w_0()
                        .font_family(theme::code_font())
                        .text_color(theme::text_dim())
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(SharedString::from(branch)),
                ),
        );
    // both platforms add up: a row is a row whichever one answered
    let (issues, pulls) = if github || azure {
        (repo.issue_total(), repo.pull_total())
    } else {
        (0, 0)
    };
    if !repo.git_status_loaded {
        // the cache starts a repo without a status (a fresh add, or a first
        // run): say what is happening instead of claiming "clean"
        meta = meta.child(
            div()
                .when_some(row_ix, |d, ix| {
                    d.debug_selector(move || format!("repo-checking-{ix}"))
                })
                .flex()
                .items_center()
                .gap(px(5.))
                .text_color(theme::text_muted())
                .child(icon(
                    "icons/icon-clock.svg",
                    px(theme::icon_tiny()),
                    theme::text_muted(),
                ))
                .child("checking"),
        );
    } else if repo.git_modified_count > 0 {
        meta = meta.child(counted(
            "icons/icon-file-diff.svg",
            repo.git_modified_count,
            Some("change"),
            theme::CHANGES,
        ));
    } else if repo.git_to_pull_count == 0 {
        meta = meta.child(
            div()
                .when_some(row_ix, |d, ix| {
                    d.debug_selector(move || format!("repo-clean-{ix}"))
                })
                .flex()
                .items_center()
                .gap(px(5.))
                .text_color(theme::text_muted())
                .child(icon(
                    "icons/icon-check.svg",
                    px(theme::icon_tiny()),
                    theme::SUCCESS_BRIGHT,
                ))
                .child("clean"),
        );
    }
    if issues > 0 {
        meta = meta.child(counted(
            "icons/icon-circle-dot.svg",
            issues,
            None,
            theme::ISSUE,
        ));
    }
    if pulls > 0 {
        meta = meta.child(
            div()
                .when_some(row_ix, |d, ix| {
                    d.debug_selector(move || format!("repo-prs-{ix}"))
                })
                .child(counted(
                    "icons/icon-git-pull-request.svg",
                    pulls,
                    None,
                    theme::PR,
                )),
        );
    }
    if repo.git_to_pull_count > 0 {
        meta = meta.child(
            div()
                .when_some(row_ix, |d, ix| {
                    d.debug_selector(move || format!("repo-behind-{ix}"))
                })
                .child(counted(
                    "icons/icon-arrow-down.svg",
                    repo.git_to_pull_count,
                    None,
                    theme::BEHIND,
                )),
        );
    }
    if repo.git_to_push_count > 0 {
        meta = meta.child(counted(
            "icons/icon-arrow-up.svg",
            repo.git_to_push_count,
            None,
            theme::AHEAD,
        ));
    }
    meta
}

/// One repository: identity on the first line, context on the second, the
/// activity stamp on the right and the row's two verbs only while hovered.
fn repo_row(
    repo: &RepoEntry,
    row_ix: usize,
    selected: bool,
    facts: &RowFacts,
    store: &Entity<Store>,
) -> gpui::AnyElement {
    let favorite = repo.is_favorite();
    let star_press = facts.star_press;
    let archived = repo.is_archived();
    let name_color = if archived {
        theme::text_muted()
    } else {
        theme::text()
    };
    let folder = repo.folder_path.to_string_lossy().to_string();
    let select_target = store.clone();
    let select_folder = folder.clone();
    let star_target = store.clone();

    // archived rows carry no star: Restore in the repo menu comes first
    let star = div()
        .id(SharedString::from(format!("repo-star-id-{row_ix}")))
        .debug_selector(move || format!("repo-star-{row_ix}"))
        .a11y_toggle(
            if favorite {
                "Remove from favorites"
            } else {
                "Add to favorites"
            },
            favorite,
        )
        .flex_shrink_0()
        .size(px(22.))
        .rounded(px(4.))
        .flex()
        .items_center()
        .justify_center()
        .when(!archived, |star| {
            star.cursor_pointer()
                .on_click(move |_, _, cx| {
                    // the star is not a press on the row
                    cx.stop_propagation();
                    star_target.update(cx, |s, cx| s.toggle_favorite(&folder, cx));
                })
                .when(favorite, |star| {
                    let glyph = icon(
                        "icons/icon-star-filled.svg",
                        px(theme::icon_tiny()),
                        theme::accent_ink(),
                    );
                    star.child(match star_press {
                        Some(press) => motion::pop(
                            SharedString::from(format!("repo-star-pop-{row_ix}-{press}")),
                            glyph,
                        ),
                        None => glyph.into_any_element(),
                    })
                })
                .when(!favorite, |star| {
                    star.opacity(0.62)
                        .group_hover("repo-row", |s| s.opacity(1.))
                        .child(icon(
                            "icons/icon-star.svg",
                            px(theme::icon_tiny()),
                            theme::star_muted(),
                        ))
                })
        });

    let meta = stats(repo, facts.github, facts.azure.is_some(), Some(row_ix));

    let name_line = div()
        .flex()
        .items_center()
        .gap(px(6.))
        .min_w_0()
        .child(
            div()
                .min_w_0()
                .text_size(px(theme::font_name()))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(name_color)
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(repo.name.clone()),
        )
        .when_some(facts.warp_titles.clone(), |line, titles| {
            line.child(crate::ui::with_tooltip(
                div()
                    .id(SharedString::from(format!("repo-warp-id-{row_ix}")))
                    .debug_selector(move || format!("repo-warp-{row_ix}"))
                    .flex_shrink_0()
                    .rounded_full()
                    .size(px(7.))
                    .bg(theme::SUCCESS_BRIGHT),
                facts.show_tooltips,
                SharedString::from(format!("warp-agent is running: {titles}")),
            ))
        });

    let identity = div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(3.))
        .child(name_line)
        .child(meta);

    // the mockup's stamp is compact ("12m", "1d"); rel_time says "… ago"
    let stamp = repo
        .git_last_commit_at
        .map(|at| rel_time::format(at).trim_end_matches(" ago").to_string())
        .unwrap_or_else(|| "—".into());

    let tab_target = store.clone();
    let tab_folder = repo.folder_path.to_string_lossy().to_string();
    let open_target = store.clone();
    let open_folder = tab_folder.clone();
    let primary = facts.primary.clone();
    let (tab_label, tab_icon) = if facts.tabbed {
        ("Go to tab", "icons/icon-tab.svg")
    } else {
        ("Open in tab", "icons/icon-tab-plus.svg")
    };
    let verbs = div()
        .absolute()
        .top(px(12.))
        .right(px(14.))
        .flex()
        .items_center()
        .gap_0p5()
        .invisible()
        .group_hover("repo-row", |s| s.visible())
        .child(row_action(
            format!("repo-tab-{row_ix}"),
            tab_label,
            tab_icon,
            move |_, _, cx| {
                cx.stop_propagation();
                tab_target.update(cx, |s, cx| s.open_repo_tab(&tab_folder, cx));
            },
        ))
        .when_some(primary, |verbs, primary| {
            verbs.child(row_action(
                format!("repo-open-{row_ix}"),
                primary.label,
                "icons/icon-code.svg",
                move |_, _, cx| {
                    cx.stop_propagation();
                    primary.launch.run(&open_folder, &open_target, cx);
                },
            ))
        });

    let row = div()
        .id(SharedString::from(format!("repo-row-id-{row_ix}")))
        .debug_selector(move || format!("repo-row-{row_ix}"))
        .group("repo-row")
        .a11y_button(repo.name.clone())
        .relative()
        .cursor_pointer()
        .on_click(move |event: &gpui::ClickEvent, _, cx| {
            // a second press keeps the repo: it gets a tab of its own, the
            // explorer's double click
            if event.click_count() > 1 {
                select_target.update(cx, |s, cx| s.open_repo_tab(&select_folder, cx));
            } else {
                select_target.update(cx, |s, cx| s.pick_repo(&select_folder, cx));
            }
        })
        .on_mouse_down(gpui::MouseButton::Middle, {
            let store = store.clone();
            let folder = repo.folder_path.to_string_lossy().to_string();
            move |_, _, cx| {
                cx.stop_propagation();
                store.update(cx, |s, cx| s.open_repo_tab(&folder, cx));
            }
        })
        .flex()
        .items_center()
        .gap(px(8.))
        .h(px(64.))
        .pl(px(13.))
        .pr(px(16.))
        .when(archived, |row| row.opacity(0.55))
        .when(selected, |row| row.bg(theme::raised()))
        .when(!selected, |row| row.hover(|s| s.bg(theme::panel())))
        .child(star)
        .child(identity)
        .child(
            div()
                .flex_shrink_0()
                .flex()
                .items_center()
                .gap_2()
                .self_start()
                .pt(px(14.))
                .group_hover("repo-row", |s| s.invisible())
                .when(facts.tabbed, |line| {
                    line.child(icon(
                        "icons/icon-tab.svg",
                        px(theme::icon_tiny()),
                        theme::accent_ink(),
                    ))
                })
                .when_some(facts.job, |line, job| {
                    line.debug_selector(move || format!("repo-row-job-{row_ix}"))
                        .child(motion::spinner(
                            SharedString::from(format!("repo-row-spinner-{row_ix}")),
                            "icons/icon-spinner.svg",
                            px(theme::icon_tiny()),
                            theme::text_dim(),
                        ))
                        .child(
                            div()
                                .text_size(px(theme::font_meta()))
                                .text_color(theme::text_dim())
                                .child(crate::net::git_working_word(job)),
                        )
                })
                .when(facts.job.is_none(), |line| {
                    line.child(
                        div()
                            .text_size(px(theme::font_meta()))
                            .text_color(theme::text_muted())
                            .child(stamp),
                    )
                }),
        )
        .child(verbs);

    // the selection is a 3px accent edge; a wrapper owns it so the row's fill
    // reads as one band
    div()
        .flex_shrink_0()
        .border_l_3()
        .border_color(if selected {
            theme::accent()
        } else {
            theme::surface()
        })
        .child(row)
        .into_any_element()
}

/// A count in its entity's hue: `21 changes`, or the bare number with its icon.
fn counted(
    icon_path: &'static str,
    count: u32,
    noun: Option<&'static str>,
    tint: Rgba,
) -> gpui::AnyElement {
    div()
        .flex_shrink_0()
        .flex()
        .items_center()
        .gap(px(5.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(tint)
        .child(icon(icon_path, px(theme::icon_tiny()), tint))
        .child(match noun {
            Some(noun) if count == 1 => format!("{count} {noun}"),
            Some(noun) => format!("{count} {noun}s"),
            None => format!("{count}"),
        })
        .into_any_element()
}
