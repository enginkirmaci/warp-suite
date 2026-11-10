//! The Summary section: the daily summaries already written to the notes store,
//! newest first, each under the day it covers, and the one verb that writes
//! another. A summary covers yesterday's commits across every tracked
//! repository, and its file opens in the notes app from the detail head.

use crate::store::Store;
use crate::theme;
use crate::ui::{A11y, icon};
use gpui::prelude::*;
use gpui::{Context, Entity, FontWeight, SharedString, Window, div, px};
use warpui_kit_markdown::LiveEditorState;

use crate::ui::markdown as md;
use std::path::PathBuf;
use warpui_kit_core::shell;

pub struct SummaryPage {
    store: Entity<Store>,
    /// Newest first: the names are dates.
    files: Vec<PathBuf>,
    selected: Option<PathBuf>,
    text: Option<SharedString>,
    /// The read-only kit editor rendering `fed_text` (the selected summary).
    editor: Option<Entity<LiveEditorState>>,
    fed_text: Option<SharedString>,
    /// A run that finishes adds a file, so the list reloads on the falling edge.
    was_busy: bool,
    loaded: bool,
}

impl SummaryPage {
    pub fn new(store: Entity<Store>, cx: &mut Context<Self>) -> Self {
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        Self {
            store,
            files: Vec::new(),
            selected: None,
            text: None,
            editor: None,
            fed_text: None,
            was_busy: false,
            loaded: false,
        }
    }

    fn reload(&mut self, cx: &Context<Self>) {
        let settings = self.store.read(cx).settings();
        let folder =
            warp_station_core::notes::resolve_store_root(&settings.general.notes_store_path)
                .join("Summaries");
        let mut files: Vec<PathBuf> = std::fs::read_dir(&folder)
            .map(|entries| {
                entries
                    .flatten()
                    .map(|entry| entry.path())
                    .filter(|path| path.extension().is_some_and(|ext| ext == "md"))
                    .collect()
            })
            .unwrap_or_default();
        files.sort_by(|a, b| b.cmp(a));
        self.files = files;
        self.loaded = true;
        let first = self.files.first().cloned();
        let keep = self
            .selected
            .clone()
            .filter(|path| self.files.contains(path));
        self.select(keep.or(first));
    }

    fn select(&mut self, path: Option<PathBuf>) {
        self.text = path
            .as_ref()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .map(SharedString::from);
        self.selected = path;
    }

    fn row(&self, path: &PathBuf, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let name = day_name(path);
        let file = file_name(path);
        let target = path.clone();
        shell::list_row(
            SharedString::from(format!("summary-{name}")),
            self.selected.as_ref() == Some(path),
        )
        .a11y_button(format!("{name} — {file}"))
        .on_click(cx.listener(move |this, _, _, cx| {
            this.select(Some(target.clone()));
            cx.notify();
        }))
        .child(
            div()
                .text_size(px(theme::font_name()))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::text())
                .child(name),
        )
        .child(
            div()
                .mt_0p5()
                .font_family(theme::code_font())
                .text_size(px(theme::font_mono_meta()))
                .text_color(theme::text_muted())
                .child(file),
        )
    }

    fn summarize_button(&self, busy: bool, enabled: bool) -> impl IntoElement + use<> {
        let store = self.store.clone();
        let live = enabled && !busy;
        div()
            .id("summary-button")
            .debug_selector(|| "summary-button".into())
            .a11y_button(if busy {
                "Summarizing…"
            } else {
                "Summarize yesterday"
            })
            .flex_shrink_0()
            .h(px(shell::CONTROL_HEIGHT))
            .px_3()
            .rounded(px(6.))
            .flex()
            .items_center()
            .gap_2()
            .text_size(px(theme::font_control()))
            .font_weight(FontWeight::SEMIBOLD)
            .when(live, |d| {
                d.cursor_pointer()
                    .bg(theme::accent())
                    .text_color(theme::on_accent())
                    .hover(|s| s.bg(theme::accent_hover()))
                    .on_click(move |_, _, cx| {
                        store.update(cx, |s, cx| s.run_summary(cx));
                    })
            })
            .when(!live, |d| {
                d.bg(theme::raised()).text_color(theme::text_muted())
            })
            .child(if busy {
                warpui_kit_core::motion::spinner(
                    "summary-spinner",
                    "icons/icon-spinner.svg",
                    px(theme::icon_small()),
                    theme::text_muted(),
                )
                .into_any_element()
            } else {
                icon(
                    "icons/icon-agent-sparkle.svg",
                    px(theme::icon_small()),
                    if live {
                        theme::on_accent()
                    } else {
                        theme::text_muted()
                    },
                )
                .into_any_element()
            })
            .child(if busy {
                "Summarizing…"
            } else {
                "Summarize yesterday"
            })
    }
}

/// The summary's day, as the prototype writes it ("Sunday, 1 Nov"); a file
/// whose name is not a date keeps its stem.
fn day_name(path: &std::path::Path) -> String {
    let stem = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_string())
        .unwrap_or_default();
    chrono::NaiveDate::parse_from_str(&stem, "%Y-%m-%d")
        .map(|day| day.format("%A, %-d %b").to_string())
        .unwrap_or(stem)
}

/// The file the day lives in: `Summaries/2026-11-01.md` is what the notes app
/// shows, so the list names it as it is.
fn file_name(path: &std::path::Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default()
}

impl Render for SummaryPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        let (busy, available) = {
            let store = self.store.read(cx);
            (store.summary_busy, store.wand_available())
        };
        if !self.loaded || (self.was_busy && !busy) {
            self.reload(cx);
        }
        self.was_busy = busy;

        let files = self.files.clone();
        let mut rows = div()
            .id("summary-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll();
        if files.is_empty() {
            rows = rows.child(
                div()
                    .px_4()
                    .py_6()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_size(px(theme::font_body()))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme::text_dim())
                            .child("No summaries yet"),
                    )
                    .child(
                        div()
                            .text_size(px(theme::font_meta()))
                            .text_color(theme::text_muted())
                            .child("Summarize yesterday writes the first one to Summaries/."),
                    ),
            );
        }
        for path in &files {
            let row = self.row(path, cx);
            rows = rows.child(row);
        }
        let list = shell::list_pane(self.store.read(cx).list_width())
            .child(
                div()
                    .flex_shrink_0()
                    .px_4()
                    .pt_3p5()
                    .pb_3()
                    .border_b_1()
                    .border_color(theme::hairline())
                    .child(
                        div()
                            .text_size(px(theme::font_list_title()))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme::text())
                            .child("Summary"),
                    )
                    .child(
                        div()
                            .mt_1()
                            .text_size(px(theme::font_meta()))
                            .text_color(theme::text_dim())
                            .child("Yesterday's work across every tracked repository"),
                    ),
            )
            .child(rows);

        // with nothing selected the list head already names the section: the
        // detail head carries only the action, not a second title
        let notes_path = self.selected.clone();
        let heading = self.selected.as_deref().map(|path| {
            let mut heading = div().flex_1().min_w_0().flex().flex_col().child(
                div()
                    .text_size(px(theme::font_screen_title()))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme::text())
                    .child(day_name(path)),
            );
            heading = heading.child(
                div()
                    .mt_0p5()
                    .flex()
                    .items_center()
                    .gap_2()
                    .font_family(theme::code_font())
                    .text_size(px(theme::font_mono_meta()))
                    .text_color(theme::text_muted())
                    .child(format!("Summaries/{}", file_name(path))),
            );
            heading
        });
        let head = div()
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap_3()
            .min_h(px(36.))
            .px(px(shell::DETAIL_PADDING))
            .py_4()
            .border_b_1()
            .border_color(theme::hairline())
            .child(heading.unwrap_or_else(|| div().flex_1()))
            .when_some(notes_path, |row, path| {
                let store = self.store.clone();
                row.child(
                    div()
                        .id("summary-open-notes")
                        .debug_selector(|| "summary-open-notes".into())
                        .a11y_button("Open in Notes")
                        .cursor_pointer()
                        .flex_shrink_0()
                        .h(px(shell::CONTROL_HEIGHT))
                        .px_3()
                        .rounded(px(6.))
                        .border_1()
                        .border_color(theme::line_strong())
                        .bg(theme::panel())
                        .flex()
                        .items_center()
                        .gap_2()
                        .text_size(px(theme::font_control()))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme::text())
                        .hover(|s| s.bg(theme::raised()))
                        .on_click(move |_, _, cx| {
                            store.update(cx, |s, cx| {
                                s.open_note_in_notes(path.clone());
                                cx.notify();
                            });
                        })
                        .child(icon(
                            "icons/icon-note.svg",
                            px(theme::icon_small()),
                            theme::text_dim(),
                        ))
                        .child("Open in Notes"),
                )
            })
            .child(self.summarize_button(busy, available));

        let body = div()
            .id("summary-body")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p(px(shell::DETAIL_PADDING))
            .child(match self.text.clone() {
                Some(text) => {
                    // feed the read-only editor on content change (a newly
                    // selected or regenerated summary)
                    if self.fed_text.as_ref() != Some(&text) {
                        match &self.editor {
                            Some(editor) => md::feed(editor, text.clone(), window, cx),
                            None => {
                                self.editor = Some(md::attach(text.clone(), window, cx));
                            }
                        }
                        self.fed_text = Some(text);
                    }
                    div()
                        .max_w(px(shell::READING_WIDTH))
                        .child(md::render_read_only(
                            self.editor.as_ref().expect("editor fed above"),
                            cx,
                        ))
                        .into_any_element()
                }
                None => div()
                    .text_size(px(theme::font_body()))
                    .text_color(theme::text_dim())
                    .child(if !available {
                        "Summaries run through warp-agent. Turn it on in Settings."
                    } else if files.is_empty() {
                        "Summarize yesterday to write the first one."
                    } else {
                        "Pick a summary from the list."
                    })
                    .into_any_element(),
            });

        shell::panes()
            .child(list)
            .child(shell::detail_pane().child(head).child(body))
    }
}
