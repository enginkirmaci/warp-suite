//! The settings page's row vocabulary: the design's `.setting` rows.
//!
//! Every app's Settings page is the same shape — a flat list of 60-high rows,
//! label and description on the left and the control on the right, separated
//! by a hairline. There is no card around them: `DESIGN.md` says "one row per
//! setting, label left and control right, 60 high", and the prototype draws
//! exactly that (`.setting` in `warp.css`).
//!
//! The apps had each grown their own version of this, and they drifted apart —
//! some wrapped the rows in cards, some put the label in `text-muted` at body
//! size instead of the design's 15/500 `text`, and some used bare checkboxes
//! where the design has a switch. These helpers are the one implementation, so
//! a settings page cannot disagree with the contract again.

use crate::a11y::A11y as _;
use crate::{fonts, palette, shell};
use gpui::prelude::*;
use gpui::{AnyElement, ElementId, FontWeight, SharedString, Window, div, px};

/// One option of a [`segmented`] control. `key` is what the caller gets back
/// and what its element id is built from, so a probe can name the button
/// (`group-depth-2`, `auto-fetch-30`, `warp-harness-dsh`).
#[derive(Clone)]
pub struct Choice {
    pub key: SharedString,
    pub label: SharedString,
    pub selected: bool,
}

impl Choice {
    pub fn new(
        key: impl Into<SharedString>,
        label: impl Into<SharedString>,
        selected: bool,
    ) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            selected,
        }
    }
}

/// The design's `.setting`: one row, label left, control right, 60 high.
/// `id` doubles as the debug selector, so a probe reaches the row by name.
pub fn row(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    control: impl IntoElement,
    first: bool,
) -> AnyElement {
    let label: SharedString = label.into();
    let id = id.into();
    let selector = id.to_string();
    div()
        .id(id)
        .debug_selector(move || selector.clone())
        .flex()
        .flex_row()
        .items_center()
        .gap_6()
        .min_h(px(shell::scaled(60.)))
        .py_2p5()
        // the design's `.setting { border-top: 1px solid var(--line-soft) }`,
        // skipped on a page's first row
        .when(!first, |d| {
            d.border_t_1().border_color(palette::border_light())
        })
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(px(fonts::font_name()))
                .font_weight(FontWeight::MEDIUM)
                .text_color(palette::text())
                .child(label),
        )
        .child(div().flex_shrink_0().child(control))
        .into_any_element()
}

/// A row that carries a description under its label, as the prototype's
/// `setting(key, title, desc)` does. The description is the `.t-meta` step
/// (13/18 in `text-2`), not an italic hint.
pub fn row_with_desc(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    desc: impl Into<SharedString>,
    control: impl IntoElement,
    first: bool,
) -> AnyElement {
    let label: SharedString = label.into();
    let desc: SharedString = desc.into();
    let id = id.into();
    let selector = id.to_string();
    div()
        .id(id)
        .debug_selector(move || selector.clone())
        .flex()
        .flex_row()
        .items_center()
        .gap_6()
        .min_h(px(shell::scaled(60.)))
        .py_2p5()
        // the design's `.setting { border-top: 1px solid var(--line-soft) }`,
        // skipped on a page's first row
        .when(!first, |d| {
            d.border_t_1().border_color(palette::border_light())
        })
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap_0p5()
                .child(
                    div()
                        .text_size(px(fonts::font_name()))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(palette::text())
                        .child(label),
                )
                .child(
                    // the description wraps inside the label column: a single
                    // long line would set the column's width, grow the row
                    // past the pane, and push the control off the window
                    div()
                        .min_w_0()
                        .whitespace_normal()
                        .text_size(px(fonts::font_meta()))
                        .text_color(palette::text_dim())
                        .child(desc),
                ),
        )
        .child(div().flex_shrink_0().child(control))
        .into_any_element()
}

/// A stacked row: the prototype's `.setting.stack` — the label over a control
/// that takes the row's full width. A folder list or a template body is not a
/// 340-wide value beside its name; it is its own block.
pub fn stack_row(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    control: impl IntoElement,
    first: bool,
) -> AnyElement {
    let label: SharedString = label.into();
    let id = id.into();
    let selector = id.to_string();
    div()
        .id(id)
        .debug_selector(move || selector.clone())
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .items_stretch()
        .gap_2()
        .py_2p5()
        .when(!first, |d| {
            d.border_t_1().border_color(palette::border_light())
        })
        .child(
            div()
                .text_size(px(fonts::font_name()))
                .font_weight(FontWeight::MEDIUM)
                .text_color(palette::text())
                .child(label),
        )
        .child(div().w_full().min_w_0().child(control))
        .into_any_element()
}

/// A switch row: the prototype's `setting()` helper — label left, the 36×20
/// `.switch` at the trailing edge. `selector` is the row's probe name.
pub fn switch_row(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    selector: impl Into<SharedString>,
    checked: bool,
    first: bool,
    on_toggle: impl Fn(&bool, &mut Window, &mut gpui::App) + 'static,
) -> AnyElement {
    let label: SharedString = label.into();
    let selector: SharedString = selector.into();
    let switch = gpui_kit::component::switch::Switch::new(id)
        .checked(checked)
        .accessibility_label(label.clone())
        .on_click(on_toggle);
    let boxed = switch_box(&label, switch);
    row(selector, label, boxed, first)
}

/// A switch row with a description line under the label.
pub fn switch_row_with_desc(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    selector: impl Into<SharedString>,
    desc: impl Into<SharedString>,
    checked: bool,
    first: bool,
    on_toggle: impl Fn(&bool, &mut Window, &mut gpui::App) + 'static,
) -> AnyElement {
    let label: SharedString = label.into();
    let selector: SharedString = selector.into();
    let switch = gpui_kit::component::switch::Switch::new(id)
        .checked(checked)
        .accessibility_label(label.clone())
        .on_click(on_toggle);
    let boxed = switch_box(&label, switch);
    row_with_desc(selector, label, desc, boxed, first)
}

/// The switch alone in a box a probe can name: the kit's `Switch` takes an
/// element id but exposes no debug selector of its own.
fn switch_box(label: &SharedString, switch: impl IntoElement) -> AnyElement {
    let selector = format!("switch-{label}");
    div()
        .debug_selector(move || selector.clone())
        .flex_shrink_0()
        .child(switch)
        .into_any_element()
}

/// A segmented choice: the prototype's `.seg` — a recessed `well` trough, the
/// chosen option lifted onto `raised`. Each button's element id is
/// `{prefix}-{key}`, and `on_pick` gets that key back.
///
/// A plain mode switch is never the accent: `DESIGN.md` reserves the accent
/// fill for the one primary action on a view.
pub fn segmented(
    prefix: &'static str,
    choices: Vec<Choice>,
    on_pick: impl Fn(SharedString, &mut Window, &mut gpui::App) + 'static,
) -> AnyElement {
    let on_pick = std::rc::Rc::new(on_pick);
    let mut trough = div()
        .flex()
        .flex_row()
        .items_center()
        .gap_0p5()
        .h(px(shell::scaled(34.)))
        .p(px(3.))
        .rounded(px(8.))
        .bg(palette::well());
    for Choice {
        key,
        label,
        selected,
    } in choices
    {
        let element_id = SharedString::from(format!("{prefix}-{key}"));
        let selector = element_id.to_string();
        let a11y = label.clone();
        let pick = on_pick.clone();
        let click_key = key.clone();
        trough = trough.child(
            div()
                .id(element_id)
                .debug_selector(move || selector.clone())
                .a11y_toggle(a11y, selected)
                .cursor_pointer()
                .h(px(28.))
                .px(px(12.))
                .flex()
                .items_center()
                .rounded(px(6.))
                .text_size(px(fonts::font_meta()))
                .when(selected, |d| {
                    d.bg(palette::raised())
                        .text_color(palette::text())
                        .font_weight(FontWeight::SEMIBOLD)
                })
                .when(!selected, |d| {
                    d.text_color(palette::text_dim())
                        .hover(|s| s.text_color(palette::text()))
                })
                .on_click(move |_, window, cx| pick(click_key.clone(), window, cx))
                .child(label),
        );
    }
    trough.into_any_element()
}

/// A group heading inside a settings page: the prototype's `subhead()`, a
/// `section` step (16/600) with air above it (`OpenCode`, `DeepSeek`, `Prompt
/// templates`).
pub fn subhead(title: impl Into<SharedString>) -> AnyElement {
    div()
        .pt_5()
        .pb_0p5()
        .text_size(px(fonts::font_heading()))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(palette::text())
        .child(title.into())
        .into_any_element()
}

/// A section's rows as one column: what a settings section returns instead of
/// a list of cards. Rows carry their own rules, so the column needs no chrome.
pub fn rows(children: Vec<AnyElement>) -> AnyElement {
    // `overflow_hidden` gives the column a definite width: without it a row
    // sizes to its content's max width, so a wide control (a palette or a
    // segmented picker) widens the whole page past the pane
    let mut column = div().flex().flex_col().w_full().min_w_0().overflow_hidden();
    for child in children {
        column = column.child(child);
    }
    column.into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The design's row is 60 high at Normal text size; the measure rides the
    /// text factor like every other shell metric.
    #[test]
    fn a_settings_row_is_sixty_high() {
        assert_eq!(shell::scaled(60.), 60.);
    }

    /// The choice key is what the button's element id is built from, so the
    /// probe names stay stable whatever the label says.
    #[test]
    fn a_choice_keeps_its_key_and_label_apart() {
        let choice = Choice::new("30", "30m", true);
        assert_eq!(choice.key.as_ref(), "30");
        assert_eq!(choice.label.as_ref(), "30m");
        assert!(choice.selected);
    }
}
