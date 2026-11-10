//! ui module: shared helpers + the main window.

pub mod azure_pull;
pub mod azure_tab;
pub mod changes;
pub mod clipboard_password;
pub mod detail;
pub mod dialogs;
pub mod diff_controls;
pub mod formatters;
pub mod issue_detail;
pub mod launchers;
pub mod markdown;
pub mod new_pull;
pub mod nuget;
pub mod overview;
pub mod pr_issues;
pub mod pull_request;
pub mod readme;
pub mod repos;
pub mod repos_list;
pub mod settings;
pub mod summary;
pub mod window;
pub mod windows_tools;

pub use warpui_kit_core::{A11y, icon};

use gpui::prelude::*;
use gpui::{
    AnyView, Context, FontWeight, IntoElement, Render, SharedString, Stateful, Styled, Window, div,
    px,
};

use crate::theme;

// ---- tooltips --------------------------------------------------------------
// The family tooltip paints the shared palette, which this app no longer uses;
// this one draws in the redesign's tokens. Every `tooltip_text` /
// `with_tooltip` call site in the app goes through here.

struct AppTooltip {
    text: SharedString,
}

impl Render for AppTooltip {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .rounded(px(4.))
            .border_1()
            .border_color(theme::hairline())
            .bg(theme::popup())
            .px(px(8.))
            .py(px(4.))
            .text_size(px(theme::font_row_meta()))
            .text_color(theme::text_dim())
            .shadow_md()
            .whitespace_nowrap()
            .child(self.text.clone())
    }
}

/// The app's tooltip body builder (grey-on-popup, 9.5px, 4px corners).
pub fn tooltip_text(
    text: impl Into<SharedString>,
) -> impl Fn(&mut Window, &mut gpui::App) -> AnyView + 'static {
    let text = text.into();
    move |_, cx| {
        let text = text.clone();
        cx.new(move |_| AppTooltip { text }).into()
    }
}

/// Attaches a tooltip when the host's "show tooltips" setting is on.
pub fn with_tooltip(
    element: Stateful<gpui::Div>,
    show: bool,
    text: impl Into<SharedString> + 'static,
) -> Stateful<gpui::Div> {
    if show {
        let text = text.into();
        element.tooltip(tooltip_text(text))
    } else {
        element
    }
}

/// Branch labels display at most this many characters before the head is
/// trimmed; the repos branch column owns the budget, and the branch picker
/// falls back to it once the full name outgrows the selector.
pub const BRANCH_LABEL_CHARS: usize = 18;

/// Width the toolbar's selector leaves for its label; the pill's tighter
/// budget exists to cap the table column and does not apply here.
const LABEL_MAX_PX: f32 = 190.;

/// The branch picker's label: the full name while its shaped width fits the
/// rail, the head-trimmed pill label once it would overflow.
pub fn branch_label(branch: &str, window: &gpui::Window) -> String {
    let run = gpui::TextRun {
        len: branch.len(),
        font: gpui::font(theme::ui_font()),
        color: gpui::Hsla::default(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let width = window
        .text_system()
        .layout_line(branch, px(theme::font_body()), &[run], None)
        .width;
    if width <= px(LABEL_MAX_PX) {
        branch.to_string()
    } else {
        trim_branch_label(branch, BRANCH_LABEL_CHARS)
    }
}

/// Long branch names shed whole leading path segments —
/// `users/enginkirmaci/test` → `…enginkirmaci/test` — and a name whose last
/// segment alone exceeds the budget sheds characters, so the tail that
/// disambiguates the branch stays visible.
pub fn trim_branch_label(branch: &str, budget: usize) -> String {
    if branch.chars().count() <= budget {
        return branch.to_string();
    }
    let mut tail = branch;
    while let Some((_, rest)) = tail.split_once('/') {
        tail = rest;
        if rest.chars().count() < budget {
            return format!("…{rest}");
        }
    }
    let keep = budget.saturating_sub(1);
    let skipped = tail.chars().count() - keep;
    let mut label: String = tail.chars().skip(skipped).collect();
    label.insert(0, '…');
    label
}

/// A tool page's title row: its title and a one-line hint.
pub fn tool_header(title: &'static str, hint: &'static str) -> gpui::AnyElement {
    let mut header = div().flex().flex_col().gap_0p5().child(
        div()
            .text_size(px(theme::font_screen_title()))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(theme::text())
            .child(title),
    );
    if !hint.is_empty() {
        header = header.child(
            div()
                .text_size(px(theme::font_meta()))
                // the prototype's `.detail-sub` is `t-meta` on `text-2`, not
                // the quietest ink
                .text_color(theme::text_dim())
                .child(hint),
        );
    }
    div()
        .flex()
        .flex_col()
        .gap_1()
        .pb_3()
        .border_b_1()
        .border_color(theme::border_light())
        .child(header)
        .into_any_element()
}

/// [`tool_header`] with the head's own actions (the design gives every tool
/// head its buttons: Copy on Clipboard Password, Register source and the
/// watch toggle on NuGet). The title shrinks and the actions wrap, so a
/// narrow pane squeezes the words instead of pushing the buttons past the
/// window edge.
pub fn tool_header_actions(
    title: &'static str,
    hint: impl IntoElement,
    actions: impl IntoElement,
) -> gpui::AnyElement {
    // the design's detail head: title and actions share one row (the title
    // clips, the actions keep their width) and the hint sits under it
    div()
        .flex()
        .flex_col()
        .gap_1()
        .pb_3()
        .border_b_1()
        .border_color(theme::border_light())
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(px(theme::font_screen_title()))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::text())
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(title),
                )
                .child(actions.into_any_element()),
        )
        .child(
            div()
                .text_size(px(theme::font_meta()))
                // the prototype's `.detail-sub` is `t-meta` on `text-2`, not
                // the quietest ink
                .text_color(theme::text_dim())
                .child(hint.into_any_element()),
        )
        .into_any_element()
}

/// The head's action cluster. Each action is its own child of this row so
/// every one of them reaches the accessibility tree: nesting two buttons in a
/// single `div` child left the second one out of the tree, which made the
/// primary unreachable for a screen reader.
pub fn head_actions(actions: Vec<gpui::AnyElement>) -> gpui::AnyElement {
    let mut row = div()
        .flex()
        .flex_row()
        .items_center()
        .flex_shrink_0()
        .gap_2();
    for (index, action) in actions.into_iter().enumerate() {
        // gpui's accesskit pass only names the first interactive child of a
        // row, so each action goes in its own id-ed wrapper: the wrapper is
        // the row's child and the button inside it is the first of its own
        // parent, which is what the tree walks
        row = row.child(
            div()
                .id(("head-action", index as u64))
                .flex_shrink_0()
                .flex()
                .items_center()
                .child(action),
        );
    }
    row.into_any_element()
}

/// The design's plain button: `panel` fill, `line-strong` edge, one control
/// height. The family's buttons are neutral; only the page's one primary
/// takes the accent fill.
pub fn neutral_button(
    id: impl Into<SharedString>,
    label: impl Into<SharedString>,
    enabled: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> gpui::AnyElement {
    let id = id.into();
    let label = label.into();
    let selector = id.to_string();
    div()
        .id(id)
        .debug_selector(move || selector.clone())
        .a11y_button(label.clone())
        .when(enabled, |d| {
            d.cursor_pointer()
                .hover(|s| s.bg(theme::raised()))
                .on_click(on_click)
        })
        .flex_shrink_0()
        .flex()
        .items_center()
        .gap_1p5()
        .h(warpui_kit_core::shell::control_height())
        .px_3()
        .rounded(px(6.))
        .border_1()
        .border_color(theme::line_strong())
        .bg(theme::panel())
        .text_size(px(theme::font_control()))
        .font_weight(FontWeight::MEDIUM)
        .text_color(theme::text())
        .when(!enabled, |d| d.opacity(0.45))
        .child(label)
        .into_any_element()
}

/// The design's primary button: the one `accent` fill a view carries, with
/// `on-accent` text.
pub fn primary_button(
    id: impl Into<SharedString>,
    label: impl Into<SharedString>,
    enabled: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> gpui::AnyElement {
    let id = id.into();
    let label = label.into();
    let selector = id.to_string();
    div()
        .id(id)
        .debug_selector(move || selector.clone())
        .a11y_button(label.clone())
        .when(enabled, |d| {
            d.cursor_pointer()
                .hover(|s| s.bg(theme::accent_hover()))
                .on_click(on_click)
        })
        .flex_shrink_0()
        .flex()
        .items_center()
        .gap_1p5()
        .h(warpui_kit_core::shell::control_height())
        .px_3()
        .rounded(px(6.))
        .bg(theme::accent())
        .text_size(px(theme::font_control()))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme::on_accent())
        .when(!enabled, |d| d.opacity(0.45))
        .child(label)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_branches_pass_through() {
        assert_eq!(trim_branch_label("main", BRANCH_LABEL_CHARS), "main");
        assert_eq!(
            trim_branch_label("users/enginkirma", BRANCH_LABEL_CHARS),
            "users/enginkirma"
        );
    }

    #[test]
    fn long_branches_shed_whole_segments() {
        assert_eq!(
            trim_branch_label("users/enginkirmaci/test", BRANCH_LABEL_CHARS),
            "…enginkirmaci/test"
        );
        assert_eq!(
            trim_branch_label("feature/one/two/three/four", BRANCH_LABEL_CHARS),
            "…two/three/four"
        );
    }

    #[test]
    fn oversized_last_segment_sheds_characters() {
        assert_eq!(
            trim_branch_label("a-very-long-branch-name-without-slashes", 10),
            "…t-slashes"
        );
    }
}
