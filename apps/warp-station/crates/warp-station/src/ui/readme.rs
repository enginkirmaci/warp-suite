//! The README view: the repo's README rendered through the suite's live
//! markdown engine in read-only mode (ui::markdown). The wand generates a
//! draft that renders in place with Save/Discard; Save writes README.md.

use crate::net::WandKind;
use crate::store::Store;
use crate::theme;
use crate::ui::A11y;
use crate::ui::icon;
use crate::ui::overview::{card, section_title};
use gpui::prelude::*;
use gpui::{Context, Entity, FontWeight, SharedString, Window, div, px};
use warpui_kit_markdown::LiveEditorState;

use crate::ui::markdown as md;

pub struct ReadmeView {
    pub store: Entity<Store>,
    /// SharedString: every render hands the engine a refcount, never a full
    /// copy of the README.
    readme: Option<SharedString>,
    /// The read-only kit editor rendering `fed_text` (README or wand draft).
    editor: Option<Entity<LiveEditorState>>,
    fed_text: Option<SharedString>,
    loaded_for: Option<String>,
}

impl ReadmeView {
    pub fn new(store: Entity<Store>, cx: &mut Context<Self>) -> Self {
        cx.observe(&store, |this, _, cx| this.follow_selection(cx))
            .detach();
        let mut view = Self {
            store,
            readme: None,
            editor: None,
            fed_text: None,
            loaded_for: None,
        };
        view.follow_selection(cx);
        view
    }

    /// Reads the README once per selected repo.
    fn follow_selection(&mut self, cx: &mut Context<Self>) {
        let selected = self.store.read(cx).selected_repo.clone();
        if selected == self.loaded_for {
            return;
        }
        self.readme = selected
            .as_deref()
            .and_then(read_readme)
            .map(SharedString::from);
        self.loaded_for = selected;
        cx.notify();
    }
}

impl Render for ReadmeView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        let folder = self.store.read(cx).selected_repo.clone();
        let draft = self.store.read(cx).readme_draft.clone();
        let (wand_active, wand_queued_here) = {
            let state = self.store.read(cx);
            (
                state.wand_busy == Some(WandKind::Readme)
                    && state.wand_repo.as_deref() == state.selected_repo.as_deref(),
                state
                    .selected_repo
                    .as_deref()
                    .is_some_and(|folder| state.wand_queued(WandKind::Readme, folder)),
            )
        };
        let ai_available = self.store.read(cx).wand_available();
        let has_draft = draft.is_some();
        let shown: Option<SharedString> = draft
            .as_ref()
            .map(|d| SharedString::from(d.as_str()))
            .or_else(|| self.readme.clone());

        // feed the read-only editor: created on first content, re-fed when
        // the README or the draft changed (set_value forgets undo and resets
        // scroll — right for a whole-document swap)
        match &shown {
            Some(text) if self.fed_text.as_ref() == Some(text) => {}
            Some(text) => match &self.editor {
                Some(editor) => md::feed(editor, text.clone(), window, cx),
                None => {
                    self.editor = Some(md::attach(text.clone(), window, cx));
                    self.fed_text = shown.clone();
                }
            },
            None => {
                self.editor = None;
                self.fed_text = None;
            }
        }

        let actions = div()
            .id("readme-actions")
            .flex()
            .flex_row()
            .items_center()
            .flex_shrink_0()
            .gap_2()
            .child(
                div()
                    .text_size(px(theme::font_meta()))
                    .text_color(theme::text_muted())
                    .child(if has_draft {
                        "README.md (unsaved)"
                    } else {
                        "README.md"
                    }),
            )
            .when(has_draft, |row| {
                let folder_save = folder.clone();
                row.child(
                    div()
                        .id("readme-discard")
                        .debug_selector(|| "readme-discard".into())
                        .a11y_button("Discard README draft")
                        .cursor_pointer()
                        .rounded_md()
                        .px_2()
                        .py_0p5()
                        .flex()
                        .items_center()
                        .gap_1()
                        .text_size(px(theme::font_body()))
                        .text_color(theme::text_dim())
                        .border_1()
                        .border_color(theme::hairline())
                        .hover(|s| s.bg(theme::hover_bg()))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.store.update(cx, |s: &mut Store, cx| {
                                s.readme_draft = None;
                                cx.notify();
                            });
                        }))
                        .child(icon(
                            "icons/icon-undo.svg",
                            px(theme::icon_small() - 1.),
                            theme::text_dim(),
                        ))
                        .child("Discard"),
                )
                .child(
                    div()
                        .id("readme-save")
                        .debug_selector(|| "readme-save".into())
                        .a11y_button("Save README")
                        .cursor_pointer()
                        .rounded_md()
                        .px_2()
                        .py_0p5()
                        .flex()
                        .items_center()
                        .gap_1()
                        .text_size(px(theme::font_body()))
                        .font_weight(FontWeight::SEMIBOLD)
                        .bg(theme::accent())
                        .text_color(theme::on_accent())
                        .hover(|s| s.opacity(0.9))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let Some(folder) = folder_save.clone() else {
                                return;
                            };
                            let draft = this
                                .store
                                .update(cx, |s: &mut Store, _| s.readme_draft.take());
                            if let Some(draft) = draft {
                                let path = std::path::Path::new(&folder).join("README.md");
                                match std::fs::write(&path, format!("{draft}\n")) {
                                    Ok(()) => {
                                        this.readme = Some(draft.into());
                                        tracing::info!("saved {}", path.display());
                                    }
                                    Err(err) => {
                                        tracing::warn!("README save failed: {err}");
                                        this.store.update(cx, |s: &mut Store, cx| {
                                            s.notify_error("Could not save README.md");
                                            cx.notify();
                                        });
                                    }
                                }
                            }
                            cx.notify();
                        }))
                        .child(icon(
                            "icons/icon-save.svg",
                            px(theme::icon_small() - 1.),
                            theme::on_accent(),
                        ))
                        .child("Save README"),
                )
            })
            .when(
                !wand_active && !wand_queued_here && shown.is_some() && ai_available,
                |row| {
                    row.child({
                        let store = self.store.clone();
                        div()
                            .id("readme-wand")
                            .a11y_button("Draft README with OpenCode")
                            .cursor_pointer()
                            .rounded_md()
                            .p_1()
                            .flex()
                            .items_center()
                            .hover(|s| s.bg(theme::hover_bg()))
                            .on_click(move |_, _, cx| {
                                store.update(cx, |s, cx| s.run_wand(WandKind::Readme, false, cx));
                            })
                            .child(icon(
                                "icons/icon-wand.svg",
                                px(theme::icon_small()),
                                theme::text_muted(),
                            ))
                    })
                },
            )
            .when(wand_active, |row| {
                row.child(div().rounded_md().p_1().flex().items_center().child(icon(
                    "icons/icon-wand.svg",
                    px(theme::icon_small()),
                    theme::blue_bright(),
                )))
            })
            .when(wand_queued_here, |row| {
                let store = self.store.clone();
                let folder = folder.clone().unwrap_or_default();
                row.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .text_size(px(theme::font_meta()))
                        .text_color(theme::text_dim())
                        .child(icon(
                            "icons/icon-wand.svg",
                            px(theme::icon_small() - 1.),
                            theme::text_dim(),
                        ))
                        .child("queued")
                        // one request leaves the queue; the live run and the
                        // rest of the queue stay
                        .child(
                            div()
                                .id("readme-dequeue")
                                .debug_selector(|| "readme-dequeue".into())
                                .a11y_button("Cancel queued README draft")
                                .cursor_pointer()
                                .ml_1p5()
                                .px_1p5()
                                .rounded(px(4.))
                                .hover(|s| s.bg(theme::hover_bg()).text_color(theme::text()))
                                .on_click(move |_, _, cx| {
                                    store.update(cx, |s, cx| {
                                        s.dequeue_wand(WandKind::Readme, &folder, cx)
                                    })
                                })
                                .child("Cancel"),
                        ),
                )
            });

        let store_for_empty = self.store.clone();
        let pane = div()
            .id("readme-pane")
            .debug_selector(|| "readme-pane".into())
            .min_w_0()
            // the markdown engine owns its own reading margins; the card is
            // already the measure, so no second inset or width cap here
            .pb_2()
            .when(shown.is_some(), |pane| {
                pane.child(md::render_read_only_card(
                    self.editor.as_ref().expect("editor fed above"),
                    cx,
                ))
            })
            .when(shown.is_none(), |pane| {
                pane.child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .gap_2()
                        .px_4()
                        .py_8()
                        .child(icon(
                            "icons/icon-note.svg",
                            px(theme::icon_large() + 10.),
                            theme::text_faint(),
                        ))
                        .child(
                            div()
                                .text_size(px(theme::font_heading()))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(theme::text_dim())
                                .child("No README yet"),
                        )
                        .child(
                            div()
                                .text_size(px(theme::font_body()))
                                .text_color(theme::text_faint())
                                .child("Create a README.md in the repository root to see it here."),
                        )
                        .when(
                            ai_available && !wand_active && !wand_queued_here,
                            |column| {
                                column.child(
                                    div()
                                        .id("readme-empty-wand")
                                        .a11y_button("Draft README with OpenCode")
                                        .cursor_pointer()
                                        .rounded_md()
                                        .border_1()
                                        .border_color(theme::accent())
                                        .px_3()
                                        .py_1p5()
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .gap_1p5()
                                        .text_size(px(theme::font_body()))
                                        .text_color(theme::blue_bright())
                                        .hover(|s| s.bg(theme::tint(theme::accent())))
                                        .on_click(move |_, _, cx| {
                                            store_for_empty.update(cx, |s, cx| {
                                                s.run_wand(WandKind::Readme, false, cx)
                                            });
                                        })
                                        .child(icon(
                                            "icons/icon-wand.svg",
                                            px(theme::icon_small()),
                                            theme::blue_bright(),
                                        ))
                                        .child("Draft with OpenCode"),
                                )
                            },
                        ),
                )
            });

        card(section_title("README".to_string()).child(actions))
            .debug_selector(|| "readme-card".into())
            .w_full()
            .child(pane)
    }
}

fn read_readme(folder: &str) -> Option<String> {
    for name in [
        "README.md",
        "readme.md",
        "Readme.md",
        "README.markdown",
        "README",
    ] {
        if let Ok(text) = std::fs::read_to_string(std::path::Path::new(folder).join(name)) {
            return Some(text);
        }
    }
    None
}
