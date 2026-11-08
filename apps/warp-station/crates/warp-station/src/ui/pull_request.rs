//! A pull request in the detail pane. It is the one view that replaces the repo
//! head, with the same regions: a title row, a subtitle, a status strip where the
//! tabs would be, then the conversation and the checks. Back or Escape returns to
//! the list. Merge is the view's primary and takes a second click.

use crate::store::{OpenPull, Store};
use crate::theme;
use crate::ui::{A11y, icon};
use gpui::prelude::*;
use gpui::{AnyElement, Entity, FontWeight, Rgba, SharedString, div, px};
use gpui_kit::base::text::TextView;
use warp_station_core::github::{CheckState, GitHubCheck, GitHubPull};
use warp_station_core::repo_store::RepoEntry;
use warpui_kit_core::{motion, shell};

/// The body's measure, wider than prose: comments sit in cards beside an avatar.
const BODY_WIDTH: f32 = 920.;
const STRIP_HEIGHT: f32 = 44.;

pub fn render(
    repo: &RepoEntry,
    open: &OpenPull,
    store: &Entity<Store>,
    tabbed: bool,
    in_tab: bool,
    cx: &gpui::App,
) -> AnyElement {
    div()
        .debug_selector(|| "pull-request".into())
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .child(head(repo, open, store, tabbed, in_tab))
        .child(status_strip(open))
        .child(body(open, cx))
        .into_any_element()
}

fn head(
    repo: &RepoEntry,
    open: &OpenPull,
    store: &Entity<Store>,
    tabbed: bool,
    in_tab: bool,
) -> impl IntoElement + use<> {
    let item = &open.item;
    let pull = open.pull.as_ref();
    let url = item.url.clone();
    let back = store.clone();
    let title = pull.map_or(item.title.as_str(), |p| p.title.as_str());

    let title_row = div()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .child(
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
                    back.update(cx, |s, cx| {
                        s.close_pull_request(cx);
                    });
                })
                .child(icon(
                    "icons/icon-arrow-left.svg",
                    px(theme::icon_small()),
                    theme::text_dim(),
                )),
        )
        .child(
            div()
                .min_w_0()
                .text_size(px(theme::font_screen_title()))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::text())
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(title.to_string()),
        )
        .child(state_pill(pull))
        .child(div().flex_1())
        .when(!in_tab, |row| {
            let target = store.clone();
            let folder = repo.folder_path.to_string_lossy().to_string();
            row.child(
                div()
                    .id("pull-open-tab")
                    .debug_selector(|| "pull-open-tab".into())
                    .a11y_button(if tabbed { "Go to tab" } else { "Open in tab" })
                    .cursor_pointer()
                    .flex_shrink_0()
                    .size(px(shell::CONTROL_HEIGHT))
                    .rounded(px(6.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .hover(|s| s.bg(theme::raised()))
                    .on_click(move |_, _, cx| {
                        target.update(cx, |s, cx| s.open_repo_tab(&folder, cx));
                    })
                    .child(icon(
                        if tabbed {
                            "icons/icon-tab.svg"
                        } else {
                            "icons/icon-tab-plus.svg"
                        },
                        px(theme::icon_small()),
                        theme::text_dim(),
                    )),
            )
        })
        .child(
            div()
                .id("pull-github")
                .a11y_button("Open on GitHub")
                .cursor_pointer()
                .flex_shrink_0()
                .h(px(shell::CONTROL_HEIGHT))
                .px_3()
                .rounded(px(6.))
                .border_1()
                .border_color(theme::line_strong())
                .flex()
                .items_center()
                .gap_2()
                .text_size(px(theme::font_control()))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme::text())
                .hover(|s| s.bg(theme::raised()))
                .on_click(move |_, _, _| {
                    if !url.is_empty() {
                        let _ = warp_station_core::terminal::open_folder(&url);
                    }
                })
                .child(icon(
                    "icons/icon-open-in-new.svg",
                    px(theme::icon_small()),
                    theme::text_dim(),
                ))
                .child("Open on GitHub"),
        )
        .children(
            pull.filter(|p| p.state == "OPEN")
                .map(|p| merge_button(p, open, store)),
        );

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

    let subtitle = div()
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
        .child(format!("{} #{}", repo.name, item.number))
        .child(
            div()
                .flex()
                .items_center()
                .gap_1p5()
                .text_color(theme::BRANCH)
                .child(icon(
                    "icons/icon-source-branch.svg",
                    px(theme::icon_tiny()),
                    theme::BRANCH,
                ))
                .font_family(theme::code_font())
                .text_size(px(theme::font_mono_meta()))
                .child(format!("{head_ref} → {base_ref}")),
        )
        .children(pull.and_then(check_summary))
        .children(pull.map(|p| {
            div()
                .flex()
                .gap_1p5()
                .font_family(theme::code_font())
                .text_size(px(theme::font_mono_meta()))
                .child(
                    div()
                        .text_color(theme::ADD)
                        .child(format!("+{}", p.additions)),
                )
                .child(
                    div()
                        .text_color(theme::DEL)
                        .child(format!("−{}", p.deletions)),
                )
        }));

    div()
        .flex_shrink_0()
        .px(px(shell::DETAIL_PADDING))
        .pt_4()
        .pb(px(14.))
        .border_b_1()
        .border_color(theme::hairline())
        .child(title_row)
        .child(subtitle)
}

fn state_pill(pull: Option<&GitHubPull>) -> impl IntoElement + use<> {
    let (label, color) = match pull {
        Some(p) if p.state == "MERGED" => ("Merged", theme::PR),
        Some(p) if p.state == "CLOSED" => ("Closed", theme::DANGER),
        Some(p) if p.is_draft => ("Draft", theme::text_muted()),
        _ => ("Open", theme::PR),
    };
    div()
        .flex_shrink_0()
        .flex()
        .items_center()
        .gap_1()
        .px_2()
        .py_0p5()
        .rounded_full()
        .bg(theme::tint(color))
        .text_size(px(theme::font_meta()))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(color)
        .child(icon(
            "icons/icon-git-pull-request.svg",
            px(theme::icon_tiny()),
            color,
        ))
        .child(label)
}

/// The view's primary. It stays pressable only when the checks allow a merge,
/// and the first press arms it.
fn merge_button(
    pull: &GitHubPull,
    open: &OpenPull,
    store: &Entity<Store>,
) -> impl IntoElement + use<> {
    let ready = pull.ready_to_merge() && !open.merging;
    let store = store.clone();
    let armed = open.merge_armed;
    let label = if open.merging {
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
        .rounded(px(6.))
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
                .on_click(move |_, _, cx| store.update(cx, |s, cx| s.press_merge(cx)))
        })
        .when(!ready, |d| {
            d.bg(theme::raised()).text_color(theme::text_muted())
        })
        .when(open.merging, |d| {
            d.child(motion::spinner(
                "pull-merge-spinner",
                "icons/icon-spinner.svg",
                px(theme::icon_small()),
                theme::text_muted(),
            ))
        })
        .child(label)
}

fn check_summary(pull: &GitHubPull) -> Option<AnyElement> {
    if pull.checks.is_empty() {
        return None;
    }
    let failed = pull.count_checks(CheckState::Failed);
    let (path, color, word) = if failed > 0 {
        let word = if failed == 1 {
            "check failing"
        } else {
            "checks failing"
        };
        (
            "icons/icon-close.svg",
            theme::DANGER,
            format!("{failed} {word}"),
        )
    } else if pull.count_checks(CheckState::Running) > 0 {
        (
            "icons/icon-clock.svg",
            theme::WARN,
            "Checks running".to_string(),
        )
    } else {
        (
            "icons/icon-check.svg",
            theme::SUCCESS_BRIGHT,
            "All checks passing".to_string(),
        )
    };
    Some(labelled(path, color, word).into_any_element())
}

fn labelled(path: &str, color: Rgba, word: impl Into<SharedString>) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .gap_1p5()
        .text_color(color)
        .child(icon(path, px(theme::icon_tiny()), color))
        .child(word.into())
}

/// The sentence the strip leads with: why this can or can't merge.
fn merge_state(pull: &GitHubPull) -> (&'static str, Rgba, &'static str) {
    if pull.state == "MERGED" {
        return ("icons/icon-check.svg", theme::PR, "Merged");
    }
    if pull.state == "CLOSED" {
        return ("icons/icon-close.svg", theme::DANGER, "Closed");
    }
    if pull.is_draft {
        return ("icons/icon-clock.svg", theme::text_muted(), "Draft");
    }
    if pull.has_conflicts() {
        return (
            "icons/icon-close.svg",
            theme::DANGER,
            "Blocked by conflicts",
        );
    }
    if pull.count_checks(CheckState::Failed) > 0 {
        return ("icons/icon-close.svg", theme::DANGER, "Blocked by checks");
    }
    if pull.count_checks(CheckState::Running) > 0 {
        return ("icons/icon-clock.svg", theme::WARN, "Waiting for checks");
    }
    (
        "icons/icon-check.svg",
        theme::SUCCESS_BRIGHT,
        "Ready to merge",
    )
}

fn status_strip(open: &OpenPull) -> impl IntoElement + use<> {
    let strip = div()
        .flex_shrink_0()
        .h(px(STRIP_HEIGHT))
        .px(px(shell::DETAIL_PADDING))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(18.))
        .bg(theme::surface())
        .border_b_1()
        .border_color(theme::hairline())
        .text_size(px(theme::font_meta()))
        .text_color(theme::text_dim());
    let Some(pull) = open.pull.as_ref() else {
        return strip.child(if open.failed {
            "GitHub did not answer."
        } else {
            "Loading the pull request…"
        });
    };

    let (path, color, word) = merge_state(pull);
    let approvals = match pull.approvals() {
        0 => "No approvals".to_string(),
        1 => "1 approval".to_string(),
        n => format!("{n} approvals"),
    };
    let mut people = div().flex().items_center().gap_3();
    for review in pull.reviews.iter().take(4) {
        let verb = match review.state.as_str() {
            "APPROVED" => "approved",
            "CHANGES_REQUESTED" => "asked for changes",
            _ => "commented",
        };
        people = people.child(
            div()
                .flex()
                .items_center()
                .gap_1p5()
                .child(avatar(&review.author, 22.))
                .child(format!("{} {verb}", review.author)),
        );
    }
    strip
        .child(labelled(path, color, word))
        .child(approvals)
        .child(if pull.has_conflicts() {
            "Has conflicts"
        } else {
            "No conflicts"
        })
        .child(format!("Base {}", pull.base_ref_name))
        .children(
            pull.updated_at
                .as_deref()
                .and_then(warp_station_core::rel_time::format_rfc3339)
                .map(|when| format!("Updated {when}")),
        )
        .child(div().flex_1())
        .child(people)
}

fn body(open: &OpenPull, cx: &gpui::App) -> impl IntoElement + use<> {
    let focus = warpui_kit_core::text_menu::focus(cx);
    let mut thread = div()
        .debug_selector(|| "pull-thread".into())
        .max_w(px(BODY_WIDTH))
        .flex()
        .flex_col()
        .gap_4();
    if let Some(pull) = open.pull.as_ref() {
        if !pull.body.trim().is_empty() {
            let author = pull.author.clone().unwrap_or_else(|| "ghost".into());
            thread = thread.child(comment(
                0,
                &author,
                "opened this pull request".into(),
                &pull.body,
                focus.clone(),
            ));
        }
        for (ix, note) in pull.comments.iter().enumerate() {
            let verb = note
                .created_at
                .as_deref()
                .and_then(warp_station_core::rel_time::format_rfc3339)
                .map_or_else(
                    || "commented".to_string(),
                    |when| format!("commented {when}"),
                );
            thread = thread.child(comment(
                ix + 1,
                &note.author,
                verb,
                &note.body,
                focus.clone(),
            ));
        }
        if !pull.checks.is_empty() {
            thread = thread.child(checks_card(pull));
        }
    }
    div()
        .id("pull-body")
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .p(px(shell::DETAIL_PADDING))
        .child(thread)
}

fn comment(
    ix: usize,
    author: &str,
    verb: String,
    text: &str,
    focus: Option<gpui::FocusHandle>,
) -> impl IntoElement + use<> {
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
                        .border_color(theme::hairline())
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
}

fn checks_card(pull: &GitHubPull) -> impl IntoElement + use<> {
    let failed = pull.count_checks(CheckState::Failed);
    let parts = [
        (pull.count_checks(CheckState::Passed), "passed"),
        (failed, "failed"),
        (pull.count_checks(CheckState::Running), "running"),
        (pull.count_checks(CheckState::Skipped), "skipped"),
    ]
    .iter()
    .filter(|(count, _)| *count > 0)
    .map(|(count, word)| format!("{count} {word}"))
    .collect::<Vec<_>>()
    .join(" · ");

    let mut card = div()
        .debug_selector(|| "pull-checks".into())
        .ml(px(44.))
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
                .h(px(44.))
                .child(icon(
                    if failed > 0 {
                        "icons/icon-close.svg"
                    } else {
                        "icons/icon-check.svg"
                    },
                    px(theme::icon_small()),
                    if failed > 0 {
                        theme::DANGER
                    } else {
                        theme::SUCCESS_BRIGHT
                    },
                ))
                .child(
                    div()
                        .text_size(px(theme::font_heading()))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::text())
                        .child("Checks"),
                )
                .child(
                    div()
                        .text_size(px(theme::font_meta()))
                        .text_color(theme::text_muted())
                        .child(parts),
                ),
        );
    for (ix, check) in pull.checks.iter().enumerate() {
        card = card.child(check_row(ix, check));
    }
    card
}

fn check_row(ix: usize, check: &GitHubCheck) -> impl IntoElement + use<> {
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
            theme::WARN,
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
    div()
        .id(SharedString::from(format!("pull-check-{ix}")))
        .a11y_button(check.name.clone())
        .h(px(40.))
        .px_4()
        .flex()
        .items_center()
        .gap_2()
        .border_t_1()
        .border_color(theme::hairline())
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
}

/// Initials on a circle: gh hands out no picture the window could draw offline.
fn avatar(name: &str, size: f32) -> impl IntoElement + use<> {
    let initials: String = name
        .split(['-', '_', ' '])
        .filter_map(|part| part.chars().next())
        .take(2)
        .flat_map(char::to_uppercase)
        .collect();
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
        .child(initials)
}
