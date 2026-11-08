//! The main window: the title bar over the rail, and the rail's section beside
//! it. Every section is the shell's list and detail pair: Repos, the written
//! Summaries, Tools and Settings. Notes and warp-agent are their own apps, so
//! the title bar launches them.

use crate::store::{Store, WorkspaceTab};
use crate::theme;
use crate::ui::A11y;
use crate::ui::clipboard_password::{ClipboardPasswordDrawer, copy_stored_password};
use crate::ui::formatters::FormattersDrawer;
use crate::ui::icon;
use crate::ui::nuget::NugetDrawer;
use crate::ui::summary::SummaryPage;
use crate::ui::windows_tools::{KeepAwakePage, SnapItPage};
use gpui::prelude::*;
use gpui::{
    App, Context, Entity, Focusable as _, FontWeight, MouseButton, Render, SharedString, Window,
    WindowControlArea, div, px,
};
use gpui_kit::component::input::{InputEvent, InputState};
use warpui_kit_core::{motion, shell};

/// The rail's sections.
#[derive(Clone, Copy, PartialEq)]
enum Section {
    Repos,
    Summary,
    Tools,
    Settings,
}

/// The Tools section's pages.
#[derive(Clone, Copy, PartialEq)]
enum Tool {
    Formatters,
    Nuget,
    ClipboardPassword,
    SnapIt,
    KeepAwake,
}

/// How wide the title bar's search field is.
const SEARCH_W: f32 = 460.;

pub struct MainWindow {
    pub store: Entity<Store>,
    section: Section,
    tool: Tool,
    repos_page: Entity<crate::ui::repos::ReposPage>,
    settings_page: Entity<crate::ui::settings::SettingsPage>,
    summary_page: Entity<SummaryPage>,
    global_search: Entity<InputState>,
    /// The tools' pages, built the first time each opens.
    formatters: Option<Entity<FormattersDrawer>>,
    nuget_drawer: Option<Entity<NugetDrawer>>,
    clipboard_drawer: Option<Entity<ClipboardPasswordDrawer>>,
    snapit_page: Option<Entity<SnapItPage>>,
    keep_awake_page: Option<Entity<KeepAwakePage>>,
    /// A left press landed on the titlebar background; the first move turns
    /// it into a window drag (Linux/macOS — Windows uses the HTCAPTION path).
    titlebar_should_move: bool,
    /// The title-bar queue panel is open: the ordered list of wand requests
    /// waiting for the one slot.
    wand_queue_open: bool,
    /// Window-level focus for the app shortcuts. The window body is not made
    /// of text fields, so without this root handle Escape/Ctrl+K only fire
    /// after an input has been clicked.
    focus_handle: gpui::FocusHandle,
    /// This window, for the single-instance door's activation request:
    /// `App::activate` is a no-op on Linux, `Window::activate_window` is not.
    window_handle: gpui::AnyWindowHandle,
}

impl MainWindow {
    pub fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let window_handle = window.window_handle();
        let repos_page = cx.new(|cx| crate::ui::repos::ReposPage::new(store.clone(), window, cx));
        let settings_page =
            cx.new(|cx| crate::ui::settings::SettingsPage::new(store.clone(), window, cx));
        let summary_page = cx.new(|cx| SummaryPage::new(store.clone(), cx));
        let global_search =
            cx.new(|cx| InputState::new(window, cx).placeholder("Search repos, notes, code…"));
        let search_store = store.clone();
        cx.subscribe(&global_search, move |_, field, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                let term = field.read(cx).value().to_string();
                search_store.update(cx, |store, cx| store.run_global_search(&term, cx));
            }
        })
        .detach();
        cx.observe(&store, |this, _, cx| {
            // a second launch asked the single-instance door to bring this
            // window forward; App::activate is a Linux no-op, so reach the
            // window itself (the compositor still decides whether to grant it)
            if this.store.update(cx, |s, _| s.take_focus_request()) {
                // defer: this observer already holds MainWindow's lease, and
                // update_window re-enters the same root view. The effect runs
                // after the current update, where activating is safe.
                let handle = this.window_handle;
                cx.defer(move |cx| {
                    if let Err(err) = cx.update_window(handle, |_, window, _| {
                        window.activate_window();
                    }) {
                        tracing::warn!("focus request could not activate the window: {err}");
                    }
                });
            }
            cx.notify();
        })
        .detach();
        let focus_handle = cx.focus_handle();
        // E2E hook: WARP_STATION_TOOLS_OPEN=formatters|nuget|clipboard opens
        // that tool on launch
        let tool = match std::env::var("WARP_STATION_TOOLS_OPEN").as_deref() {
            Ok("formatters") => Some(Tool::Formatters),
            Ok("nuget") => Some(Tool::Nuget),
            Ok("clipboard") => Some(Tool::ClipboardPassword),
            _ => None,
        };
        let this = Self {
            store,
            // E2E hook: WARP_STATION_PAGE=settings|summary opens that section
            section: match std::env::var("WARP_STATION_PAGE").as_deref() {
                _ if tool.is_some() => Section::Tools,
                Ok("settings") => Section::Settings,
                Ok("summary") => Section::Summary,
                Ok("tools") => Section::Tools,
                _ => Section::Repos,
            },
            tool: tool.unwrap_or(Tool::Formatters),
            repos_page,
            settings_page,
            summary_page,
            global_search,
            formatters: None,
            nuget_drawer: None,
            clipboard_drawer: None,
            snapit_page: None,
            keep_awake_page: None,
            titlebar_should_move: false,
            wand_queue_open: false,
            focus_handle,
            window_handle,
        };
        // keep the root focus handle live so Escape / Ctrl+K work before the
        // first input click; inputs take focus back when the user clicks them
        window.focus(&this.focus_handle, cx);
        // a focused field can leave the tree with its popup or page, and the
        // window's keys would die with it until the next root click
        cx.on_focus_lost(window, |this, window, cx| {
            window.focus(&this.focus_handle, cx);
        })
        .detach();
        this
    }
}

impl Render for MainWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        if self.store.read(cx).notes_requested {
            self.store.update(cx, |s, _| s.notes_requested = false);
            self.launch_notes(None, cx);
        }
        if let Some(note) = self.store.read(cx).notes_open.clone() {
            self.store.update(cx, |s, _| s.notes_open = None);
            self.launch_notes(Some(note), cx);
        }
        let mut root = div()
            .id("app-root")
            .role(gpui::Role::Application)
            .aria_label("Warp Station")
            .size_full()
            .flex()
            .flex_col()
            .bg(theme::background())
            .text_color(theme::text())
            .font_family(theme::ui_font())
            .track_focus(&self.focus_handle)
            // any press outside the page's menus dismisses them; the menus
            // themselves and their triggers keep the press from reaching this
            // handler
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    gpui_kit::base::GlobalState::suppress_text_selection(cx);
                    this.repos_page.update(cx, |page, cx| page.close_popups(cx));
                    if this.wand_queue_open {
                        this.wand_queue_open = false;
                        cx.notify();
                    }
                }),
            )
            // Ctrl+K and Ctrl+F focus the global-search field; Ctrl+Shift+V
            // copies the stored Clipboard Password (fallback for when the
            // system-wide grab in net.rs did not land); Escape dismisses the
            // topmost layer (page menu or dialog, then the section, then search
            // text). Plain letters belong to the repo list's keymap.
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                let modifiers = event.keystroke.modifiers;
                let key = event.keystroke.key.as_str();
                if modifiers.control && modifiers.shift && key == "v" {
                    let message = copy_stored_password(&this.store, cx);
                    let kind = crate::store::NoticeKind::for_copy_message(message);
                    this.store.update(cx, |s, cx| {
                        s.notify(kind, message.to_string());
                        cx.notify();
                    });
                    cx.stop_propagation();
                    return;
                }
                if modifiers.control && !modifiers.shift && matches!(key, "f" | "k") {
                    this.global_search.update(cx, |state, cx| {
                        state.focus(window, cx);
                        // typing after the shortcut replaces the previous term
                        state.select_all(window, cx);
                    });
                    cx.stop_propagation();
                    return;
                }
                if modifiers.control
                    && this.section == Section::Repos
                    && this.tab_key(key, modifiers.shift, window, cx)
                {
                    cx.stop_propagation();
                    return;
                }
                if key == "escape" {
                    if this.wand_queue_open {
                        this.wand_queue_open = false;
                        cx.notify();
                        cx.stop_propagation();
                        return;
                    }
                    if this.section == Section::Repos
                        && this
                            .repos_page
                            .update(cx, |page, cx| page.dismiss_overlay(cx))
                    {
                        cx.stop_propagation();
                        return;
                    }
                    if this.section != Section::Repos {
                        this.section = Section::Repos;
                        cx.notify();
                        cx.stop_propagation();
                        return;
                    }
                    if !this.global_search.read(cx).value().trim().is_empty() {
                        this.global_search
                            .update(cx, |state, cx| state.set_value("", window, cx));
                        this.store.update(cx, |s, cx| s.run_global_search("", cx));
                        cx.stop_propagation();
                    }
                }

                // The list's own keymap: ↑↓ move the row selection, S favorites
                // the focused repo, C opens Changes, ↵ and the letters run its
                // launchers. Text inputs keep their keys (the Input key context
                // owns the focus path) and so does anything open on top of the
                // list: these all act on the rows under it.
                if this.section == Section::Repos
                    && !modifiers.modified()
                    && !this.layer_open(cx)
                    && !window
                        .context_stack()
                        .iter()
                        .any(|context| context.contains("Input"))
                {
                    match key {
                        "down" | "up" => {
                            let delta = if key == "down" { 1 } else { -1 };
                            this.store
                                .update(cx, |store, cx| store.step_selection(delta, cx));
                            cx.stop_propagation();
                        }
                        // a held key repeats: only stepping through rows wants that
                        _ if event.is_held => {}
                        "s" => {
                            let folder = this.store.read(cx).selected_repo.clone();
                            if let Some(folder) = folder {
                                this.store
                                    .update(cx, |s, cx| s.toggle_favorite(&folder, cx));
                            }
                            cx.stop_propagation();
                        }
                        "c" => {
                            this.store.update(cx, |s, cx| {
                                s.open_detail_tab(WorkspaceTab::Changes, cx);
                            });
                            cx.stop_propagation();
                        }
                        _ => {
                            if crate::ui::launchers::run_key(key, &this.store, cx) {
                                cx.stop_propagation();
                            }
                        }
                    }
                }
            }))
            // mounts the window's selection machinery: without this layer the
            // kit's global mouse handlers never register and selectable
            // TextViews (README, notes preview) ignore drags entirely
            .child(gpui_kit::base::TextSelectionLayer)
            .child(self.render_titlebar(window, cx))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .min_h_0()
                    .child(self.render_rail(cx))
                    .child(self.render_body(window, cx)),
            )
            .relative();
        if let Some(popup) = self.render_search_popup(window, cx) {
            root = root.child(popup);
        }
        if let Some(panel) = self.render_wand_queue(cx) {
            root = root.child(panel);
        }
        if let Some(toasts) = self.render_toasts(cx) {
            root = root.child(toasts);
        }
        root
    }
}

impl MainWindow {
    /// The wand queue panel, hanging under the title bar's activity chip: the
    /// requests waiting for the one slot, in the order they will run, each
    /// with its own remove control. Closed by a press outside, Escape, or the
    /// queue emptying.
    fn render_wand_queue(&mut self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        if !self.wand_queue_open {
            return None;
        }
        let queue = self.store.read(cx).wand_queue.clone();
        if queue.is_empty() {
            // the last request left while the panel was open: close instead
            // of flashing an empty card
            self.wand_queue_open = false;
            return None;
        }
        let mut panel = div()
            .id("wand-queue-panel")
            .debug_selector(|| "wand-queue-panel".into())
            .absolute()
            .top(px(theme::TITLEBAR_H))
            .right(px(12.))
            .w(px(360.))
            .occlude()
            .rounded_lg()
            .border_1()
            .border_color(theme::hairline())
            .bg(theme::popup())
            .shadow_lg()
            .p_2()
            .flex()
            .flex_col()
            .gap_1()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(theme::font_body()))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme::text())
                            .child("Generation queue"),
                    )
                    .child(
                        div()
                            .text_size(px(theme::font_meta()))
                            .text_color(theme::text_faint())
                            .child(format!("{} waiting", queue.len())),
                    ),
            )
            .child(
                div()
                    .text_size(px(theme::font_meta()))
                    .text_color(theme::text_faint())
                    .child("Runs one at a time, in this order."),
            );
        for (ix, request) in queue.iter().enumerate() {
            let store = self.store.clone();
            let name = self.store.read(cx).repo_name(&request.folder);
            let kind = request.kind;
            let auto_commit = request.auto_commit;
            let folder = request.folder.clone();
            let what = match kind {
                crate::net::WandKind::Commit => "commit message",
                crate::net::WandKind::Readme => "README",
            };
            panel = panel.child(
                div()
                    .id(("wand-queue-row", ix))
                    .debug_selector(move || format!("wand-queue-row-{ix}"))
                    .flex()
                    .items_center()
                    .gap_2()
                    .rounded(px(4.))
                    .px_1()
                    .py_1()
                    .hover(|s| s.bg(theme::hover_bg()))
                    .child(
                        div()
                            .w(px(16.))
                            .flex_shrink_0()
                            .text_size(px(theme::font_meta()))
                            .text_color(theme::text_faint())
                            .child(format!("{}", ix + 1)),
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
                                    .min_w_0()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_size(px(theme::font_body()))
                                    .text_color(theme::text())
                                    .child(name),
                            )
                            .child(
                                div()
                                    .text_size(px(theme::font_meta()))
                                    .text_color(theme::text_faint())
                                    .child(if auto_commit {
                                        format!("{what} · then commit")
                                    } else {
                                        what.to_string()
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .id(("wand-queue-cancel", ix))
                            .debug_selector(move || format!("wand-queue-cancel-{ix}"))
                            .a11y_button("Remove from the generation queue")
                            .cursor_pointer()
                            .flex_shrink_0()
                            .rounded(px(4.))
                            .p_1()
                            .hover(|s| s.bg(theme::raised()))
                            .on_click(move |_, _, cx| {
                                store.update(cx, |s, cx| s.dequeue_wand(kind, &folder, cx))
                            })
                            .child(icon(
                                "icons/icon-close.svg",
                                px(theme::icon_small() - 4.),
                                theme::text_faint(),
                            )),
                    ),
            );
        }
        Some(panel.into_any_element())
    }

    /// The toast stack, bottom-right: severity glyph + full text + a close
    /// button. Painted last so it rides above pages and menus; each card
    /// occludes so a click dismisses instead of falling through to whatever
    /// sits underneath. The per-second tick expires them (errors live
    /// longer than confirmations).
    fn render_toasts(&mut self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        let notices = self.store.read(cx).notices.list().to_vec();
        if notices.is_empty() {
            return None;
        }
        let mut stack = div()
            .id("toast-stack")
            .debug_selector(|| "toast-stack".into())
            .absolute()
            .bottom_3()
            .right_3()
            .flex()
            .flex_col()
            .items_end()
            .gap_2();
        for notice in notices {
            let (glyph, color) = match notice.kind {
                crate::store::NoticeKind::Success => {
                    ("icons/icon-check.svg", theme::SUCCESS_BRIGHT)
                }
                crate::store::NoticeKind::Info => ("icons/icon-info.svg", theme::blue_bright()),
                crate::store::NoticeKind::Error => ("icons/icon-alert.svg", theme::DANGER),
            };
            let id = notice.id;
            let store = self.store.clone();
            stack = stack.child(motion::enter(
                ("toast-enter", id),
                div()
                    .id(("toast", id))
                    .debug_selector(move || format!("toast-{id}"))
                    .role(gpui::Role::Alert)
                    .aria_label(notice.text.clone())
                    .occlude()
                    .max_w(px(420.))
                    .flex()
                    .flex_row()
                    .items_start()
                    .gap_2()
                    .rounded_lg()
                    .border_1()
                    .border_color(if notice.kind == crate::store::NoticeKind::Error {
                        theme::tint(theme::DANGER)
                    } else {
                        gpui::Hsla::from(theme::hairline())
                    })
                    .bg(theme::popup())
                    .shadow_lg()
                    .p_2p5()
                    .child(icon(glyph, px(theme::icon_small()), color))
                    .child(
                        div()
                            .min_w_0()
                            .text_size(px(theme::font_body()))
                            .text_color(if notice.kind == crate::store::NoticeKind::Error {
                                theme::text()
                            } else {
                                theme::text_dim()
                            })
                            .child(notice.text.clone()),
                    )
                    .child(
                        div()
                            .id(("toast-close", id))
                            .debug_selector(move || format!("toast-close-{id}"))
                            .a11y_button("Dismiss notification")
                            .cursor_pointer()
                            .flex_shrink_0()
                            .rounded_md()
                            .p_0p5()
                            .hover(|s| s.bg(theme::hover_bg()))
                            .on_click(move |_, _, cx| {
                                cx.stop_propagation();
                                store.update(cx, |s, cx| s.dismiss_notice(id, cx));
                            })
                            .child(icon(
                                "icons/icon-close.svg",
                                px(theme::icon_small() - 4.),
                                theme::text_faint(),
                            )),
                    ),
            ));
        }
        Some(stack.into_any_element())
    }
}

impl MainWindow {
    /// Whether something is open over the repo list.
    fn layer_open(&self, cx: &gpui::App) -> bool {
        self.repos_page.read(cx).layer_open()
    }

    /// The repo tabs' keys, Ctrl held: T or ↵ opens the selected repo in a tab
    /// (fields keep ↵), W closes the active tab, \ toggles the list, Tab steps
    /// along the strip and the digits jump to a place on it. Returns whether
    /// the key was one of those.
    fn tab_key(&mut self, key: &str, shift: bool, window: &Window, cx: &mut Context<Self>) -> bool {
        let in_field = window
            .context_stack()
            .iter()
            .any(|context| context.contains("Input"));
        match key {
            "\\" => {
                self.repos_page.update(cx, |page, cx| page.toggle_list(cx));
                true
            }
            "t" => self.open_selected_in_tab(cx),
            "enter" if !in_field => self.open_selected_in_tab(cx),
            "w" => {
                let active = self.store.read(cx).active_repo_tab.clone();
                if let Some(folder) = active {
                    self.store.update(cx, |s, cx| s.close_repo_tab(&folder, cx));
                }
                true
            }
            "tab" => {
                let delta = if shift { -1 } else { 1 };
                self.store.update(cx, |s, cx| s.cycle_repo_tab(delta, cx));
                true
            }
            _ => match key.parse::<usize>() {
                Ok(digit @ 1..=9) => {
                    self.store
                        .update(cx, |s, cx| s.show_repo_tab_at(digit - 1, cx));
                    true
                }
                _ => false,
            },
        }
    }

    fn open_selected_in_tab(&mut self, cx: &mut Context<Self>) -> bool {
        let selected = self.store.read(cx).selected_repo.clone();
        if let Some(folder) = selected {
            self.store.update(cx, |s, cx| s.open_repo_tab(&folder, cx));
        }
        true
    }

    /// The global-search results popup under the titlebar field.
    fn render_search_popup(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let term = self.global_search.read(cx).value().to_string();
        if term.trim().is_empty() {
            return None;
        }
        // the popup hangs from the centered search field's left edge
        let left = ((window.viewport_size().width.as_f32() - SEARCH_W) / 2.).max(0.);
        let (hits, pending, code_hits, code_searching) = {
            let store = self.store.read(cx);
            (
                store.global_search.clone(),
                store.global_search_pending,
                store.code_search_hits.clone(),
                store.code_searching,
            )
        };
        if hits.is_empty() && code_hits.is_empty() {
            // the popup shows a footer while focused; a hint beats a stale
            // "No matches." during the debounced background search — either
            // half of it (repo/note or the ripgrep sweep)
            return Some(
                div()
                    .id("search-popup")
                    .debug_selector(|| "search-popup".into())
                    .absolute()
                    .top(px(theme::TITLEBAR_H))
                    .left(px(left))
                    .w(px(SEARCH_W))
                    .occlude()
                    .rounded_lg()
                    .border_1()
                    .border_color(theme::hairline())
                    .bg(theme::popup())
                    .shadow_lg()
                    .p_2()
                    .text_size(px(theme::font_body()))
                    .text_color(theme::text_faint())
                    .child(if pending || code_searching {
                        "Searching…"
                    } else {
                        "No matches."
                    })
                    .into_any_element(),
            );
        }
        // One flat row budget shared by every kind, in the design's group
        // order: repositories, then notes, then the code sweep. Each group
        // names itself, so a row no longer carries its kind.
        const POPUP_ROWS: usize = 12;
        let query = self.global_search.read(cx).value().trim().to_string();
        let mut rows = div().flex().flex_col().gap_0p5();
        let mut shown = 0usize;

        let repos: Vec<&crate::store::GlobalSearchHit> = hits
            .iter()
            .filter(|hit| matches!(hit, crate::store::GlobalSearchHit::Repo { .. }))
            .collect();
        if !repos.is_empty() {
            rows = rows.child(search_group("Repositories"));
            for (index, hit) in repos.into_iter().enumerate() {
                if shown >= POPUP_ROWS {
                    break;
                }
                shown += 1;
                let crate::store::GlobalSearchHit::Repo { name, folder } = hit else {
                    continue;
                };
                let hit_for_click = hit.clone();
                rows = rows.child(
                    search_hit_row(
                        index,
                        "search-hit",
                        "icons/icon-folder.svg",
                        marked_text(name, &query, theme::text_dim(), false),
                        marked_text(folder, &query, theme::text_faint(), true),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.activate_search_hit(&hit_for_click, window, cx);
                    })),
                );
            }
        }

        let notes: Vec<&crate::store::GlobalSearchHit> = hits
            .iter()
            .filter(|hit| matches!(hit, crate::store::GlobalSearchHit::Note { .. }))
            .collect();
        if !notes.is_empty() {
            rows = rows.child(search_group("Notes"));
            for (index, hit) in notes.into_iter().enumerate() {
                if shown >= POPUP_ROWS {
                    break;
                }
                shown += 1;
                let crate::store::GlobalSearchHit::Note { title, excerpt, .. } = hit else {
                    continue;
                };
                let hit_for_click = hit.clone();
                rows = rows.child(
                    search_hit_row(
                        index,
                        "search-note",
                        "icons/icon-note.svg",
                        marked_text(title, &query, theme::text_dim(), false),
                        marked_text(excerpt, &query, theme::text_faint(), false),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.activate_search_hit(&hit_for_click, window, cx);
                    })),
                );
            }
        }

        if !code_hits.is_empty() && shown < POPUP_ROWS {
            rows = rows.child(search_group("Code"));
        }
        for (index, hit) in code_hits.iter().enumerate() {
            if shown >= POPUP_ROWS {
                break;
            }
            shown += 1;
            let hit_for_click = hit.clone();
            let file_name = hit
                .abs_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| hit.rel_path.clone());
            let dir = std::path::Path::new(&hit.rel_path)
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();
            // the excerpt splits around the match so the hit itself rides the
            // accent; the offsets are byte offsets into the excerpt
            let safe_end = hit.match_end.min(hit.excerpt.len());
            let safe_start = hit.match_start.min(safe_end);
            let (before, matched, after) = (
                hit.excerpt[..safe_start].to_string(),
                hit.excerpt[safe_start..safe_end].to_string(),
                hit.excerpt[safe_end..].to_string(),
            );
            rows = rows.child(
                div()
                    .id(("search-code-hit", index as u64))
                    .debug_selector(move || format!("search-code-hit-{index}"))
                    .a11y_button(format!("{}:{} — {}", file_name, hit.line, hit.rel_path))
                    .cursor_pointer()
                    .rounded_md()
                    .px_2()
                    .py_1()
                    .flex()
                    .flex_row()
                    .items_start()
                    .gap_2()
                    .overflow_hidden()
                    .hover(|s| s.bg(theme::hover_bg()))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.activate_code_hit(hit_for_click.clone(), window, cx);
                    }))
                    .child(icon(
                        "icons/icon-code.svg",
                        px(theme::icon_small()),
                        theme::text_muted(),
                    ))
                    .child(
                        div()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .overflow_hidden()
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .gap_1()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .child(
                                        div()
                                            .text_size(px(theme::font_body()))
                                            .text_color(theme::text_dim())
                                            .child(file_name),
                                    )
                                    .when(!dir.is_empty(), |title| {
                                        title.child(
                                            div()
                                                .text_size(px(theme::font_control()))
                                                .text_color(theme::text_faint())
                                                .child(dir),
                                        )
                                    }),
                            )
                            // inline styling = one row of separately styled
                            // text segments; nowrap keeps the three pieces on
                            // the excerpt's single line
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_size(px(theme::font_control()))
                                    .child(div().text_color(theme::text_faint()).child(before))
                                    .child(
                                        div()
                                            .rounded(px(2.))
                                            .bg(theme::accent_soft())
                                            .text_color(theme::accent_ink())
                                            .child(matched),
                                    )
                                    .child(div().text_color(theme::text_faint()).child(after)),
                            ),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .ml_auto()
                            .text_size(px(theme::font_control()))
                            .text_color(theme::text_faint())
                            .child(format!(":{}", hit.line)),
                    ),
            );
        }
        let mut popup = div()
            .id("search-popup")
            .debug_selector(|| "search-popup".into())
            .absolute()
            .top(px(theme::TITLEBAR_H))
            .left(px(left))
            .w(px(SEARCH_W))
            .occlude()
            .rounded_lg()
            .border_1()
            .border_color(theme::hairline())
            .bg(theme::popup())
            .shadow_lg()
            .p_1()
            .child(rows);
        if code_searching {
            // the sweep trails the repo/note half; one quiet line, no spinner
            popup = popup.child(
                div()
                    .px_2()
                    .py_1()
                    .text_size(px(theme::font_control()))
                    .text_color(theme::text_faint())
                    .child("searching code…"),
            );
        }
        Some(popup.into_any_element())
    }

    fn activate_search_hit(
        &mut self,
        hit: &crate::store::GlobalSearchHit,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match hit {
            crate::store::GlobalSearchHit::Repo { folder, .. } => {
                self.section = Section::Repos;
                self.store.update(cx, |s, cx| s.pick_repo(folder, cx));
            }
            crate::store::GlobalSearchHit::Note { path, .. } => {
                self.launch_notes(Some(path.clone()), cx);
            }
        }
        self.global_search
            .update(cx, |state, cx| state.set_value("", window, cx));
        self.store.update(cx, |s, cx| s.run_global_search("", cx));
        cx.notify();
    }

    /// A code hit opens the file at its line in VS Code (the configured
    /// profile; fallback: the system default editor), then the popup clears
    /// like every other activation. The launch is fire-and-forget — spawn
    /// only, so a slow editor never holds the click.
    fn activate_code_hit(
        &mut self,
        hit: warp_station_core::code_search::CodeHit,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let settings = self.store.read(cx).settings();
        let plan = warp_station_core::terminal::plan_code_open(
            &hit.abs_path.to_string_lossy(),
            hit.line,
            &settings.repos.vscode_executable,
            &settings.repos.vscode_profile,
        );
        if let Err(err) = warp_station_core::terminal::open_code_plan(&plan) {
            self.store
                .update(cx, |s, _| s.notify_error(format!("open failed: {err}")));
        }
        self.global_search
            .update(cx, |state, cx| state.set_value("", window, cx));
        self.store.update(cx, |s, cx| s.run_global_search("", cx));
        cx.notify();
    }

    /// Launches the external warp-notes app on the shared store: the notes
    /// toggle, the summary's "Open in Notes" and the global-search note hits
    /// (with one) all land here. They ask for the whole store, so the titlebar
    /// always opens the notes root whatever repo is selected; only the repo
    /// launchers pin a folder. Fire-and-forget — the window outlives Station.
    fn launch_notes(&mut self, note: Option<std::path::PathBuf>, cx: &mut Context<Self>) {
        let settings = self.store.read(cx).settings();
        if !settings.warp_notes.enable_warp_notes {
            return;
        }
        let executable = settings.warp_notes.executable.clone();
        let store_root =
            warp_station_core::notes::resolve_store_root(&settings.general.notes_store_path);
        if let Err(err) = warp_station_core::warp_notes::launch(
            &executable,
            &store_root,
            warp_station_core::warp_notes::NotesScope::WholeStore,
            note.as_deref(),
        ) {
            self.store.update(cx, |s, _| s.notify_error(err));
        }
        cx.notify();
    }
}

/// A group heading inside the search results.
fn search_group(title: &'static str) -> gpui::AnyElement {
    div()
        .px_2()
        .pt_2()
        .pb_1()
        .text_size(px(theme::font_control()))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme::text_dim())
        .child(title)
        .into_any_element()
}

/// One search hit's shell: the kind's glyph over a title and a detail line.
fn search_hit_row(
    index: usize,
    id: &'static str,
    icon_path: &'static str,
    title: gpui::AnyElement,
    detail: gpui::AnyElement,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id((id, index as u64))
        .debug_selector(move || format!("{id}-{index}"))
        .cursor_pointer()
        .rounded_md()
        .px_2()
        .py_1()
        .flex()
        .flex_row()
        .items_start()
        .gap_2()
        .overflow_hidden()
        .hover(|s| s.bg(theme::hover_bg()))
        .child(icon(
            icon_path,
            px(theme::icon_small()),
            theme::text_muted(),
        ))
        .child(
            div()
                .min_w_0()
                .flex()
                .flex_col()
                .overflow_hidden()
                .child(title)
                .child(detail),
        )
}

/// `text` with the query's match on the accent-soft fill the design gives a
/// search hit. Case folding can change byte lengths, so a length mismatch
/// (or a non-boundary) leaves the text whole rather than slicing mid-glyph.
fn marked_text(text: &str, query: &str, ink: gpui::Rgba, mono: bool) -> gpui::AnyElement {
    let size = if mono {
        px(theme::font_mono_meta())
    } else {
        px(theme::font_body())
    };
    let family = if mono {
        theme::code_font()
    } else {
        theme::ui_font()
    };
    let folded = text.to_lowercase();
    let needle = query.trim().to_lowercase();
    let hit = (!needle.is_empty() && folded.len() == text.len())
        .then(|| folded.find(&needle))
        .flatten()
        .filter(|at| {
            let end = at + needle.len();
            end <= text.len() && text.is_char_boundary(*at) && text.is_char_boundary(end)
        });
    let mut row = div()
        .flex()
        .flex_row()
        .overflow_hidden()
        .whitespace_nowrap()
        .font_family(family)
        .text_size(size);
    match hit {
        Some(at) => {
            row = row
                .child(div().text_color(ink).child(text[..at].to_string()))
                .child(
                    div()
                        .rounded(px(2.))
                        .bg(theme::accent_soft())
                        .text_color(theme::accent_ink())
                        .child(text[at..at + needle.len()].to_string()),
                )
                .child(
                    div()
                        .text_color(ink)
                        .child(text[at + needle.len()..].to_string()),
                );
        }
        None => {
            row = row.child(div().text_color(ink).child(text.to_string()));
        }
    }
    row.into_any_element()
}

pub(crate) fn window_button(
    id: &'static str,
    icon_path: &'static str,
    hover_bg: gpui::Hsla,
    tooltip: &'static str,
    show_tooltip: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> impl gpui::IntoElement {
    window_button_glyph(
        id,
        hover_bg,
        tooltip,
        show_tooltip,
        on_click,
        icon(icon_path, px(theme::icon_medium()), theme::text_muted()).into_any_element(),
    )
}

/// [`window_button`] carrying a prepared glyph: the zoom button passes the
/// kit's window-state glyph so the family's maximize/restore reads alike.
pub(crate) fn window_button_glyph(
    id: &'static str,
    hover_bg: gpui::Hsla,
    tooltip: &'static str,
    show_tooltip: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
    glyph: gpui::AnyElement,
) -> impl gpui::IntoElement {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .a11y_button(tooltip)
        .cursor_pointer()
        // the titlebar underneath is one big drag region; a control both
        // occludes it from the Windows hit test and stops the press from
        // reaching the drag handler, so a click stays a click
        .occlude()
        .flex_shrink_0()
        .w(px(36.))
        .h(px(theme::TITLEBAR_H))
        .flex()
        .items_center()
        .justify_center()
        .hover(move |s| s.bg(hover_bg))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(on_click)
        .when(show_tooltip, |d| {
            d.tooltip(crate::ui::tooltip_text(tooltip))
        })
        .child(glyph)
}

/// A title bar icon button: a control island inside the drag region.
fn title_icon(
    id: &'static str,
    label: &'static str,
    icon_path: &'static str,
    show_tooltip: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> impl gpui::IntoElement {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .a11y_button(label)
        .cursor_pointer()
        .occlude()
        .flex_shrink_0()
        .w(px(34.))
        .h(px(32.))
        .rounded(px(6.))
        .flex()
        .items_center()
        .justify_center()
        .hover(|d| d.bg(theme::raised()))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(on_click)
        .when(show_tooltip, |d| d.tooltip(crate::ui::tooltip_text(label)))
        .child(icon(icon_path, px(theme::icon_medium()), theme::text_dim()))
}

impl MainWindow {
    fn render_titlebar(&mut self, window: &Window, cx: &mut Context<Self>) -> gpui::AnyElement {
        let settings = self.store.read(cx).settings();
        let show_tooltips = settings.general.show_tooltips;
        // the wand's Cancel rides the title bar: a run keeps going when the
        // repo it was kicked for leaves the screen, so its stop control must
        // be reachable from every repo and every section
        let queue_open = self.wand_queue_open;
        let (activity, wand_live, queued) = {
            let store = self.store.read(cx);
            (
                store.activity(),
                store.wand_busy.is_some(),
                store.wand_queue.len(),
            )
        };

        let right = div()
            .debug_selector(|| "titlebar-right".into())
            .flex_1()
            .min_w_0()
            .flex()
            .items_center()
            .justify_end()
            .gap_1()
            .when_some(activity, |row, label| {
                row.child(
                    div()
                        .id("activity-chip")
                        .debug_selector(|| "activity-chip".into())
                        .role(gpui::Role::Status)
                        .aria_label(label.clone())
                        .min_w_0()
                        .h(px(28.))
                        .px_2p5()
                        .rounded(px(14.))
                        .bg(theme::panel())
                        .flex()
                        .items_center()
                        .gap(px(7.))
                        .text_size(px(theme::font_meta()))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme::BEHIND)
                        .whitespace_nowrap()
                        .child(motion::spinner(
                            "activity-spinner",
                            "icons/icon-spinner.svg",
                            px(theme::icon_tiny()),
                            theme::BEHIND,
                        ))
                        .child(label)
                        .when(queued > 0, |chip| {
                            chip.child(
                                div()
                                    .id("wand-queue-toggle")
                                    .debug_selector(|| "wand-queue-toggle".into())
                                    .a11y_button("Show the generation queue")
                                    .cursor_pointer()
                                    .occlude()
                                    .flex_shrink_0()
                                    .ml_1p5()
                                    .px_1p5()
                                    .rounded(px(4.))
                                    .text_color(if queue_open {
                                        theme::text()
                                    } else {
                                        theme::text_dim()
                                    })
                                    .hover(|s| s.bg(theme::hover_bg()).text_color(theme::text()))
                                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                        cx.stop_propagation()
                                    })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.wand_queue_open = !this.wand_queue_open;
                                        cx.notify();
                                    }))
                                    .child(format!("{queued} queued")),
                            )
                        })
                        .when(wand_live, |chip| {
                            chip.child(
                                div()
                                    .id("activity-cancel")
                                    .debug_selector(|| "activity-cancel".into())
                                    .a11y_button("Stop generation and clear the queue")
                                    .cursor_pointer()
                                    .occlude()
                                    .flex_shrink_0()
                                    .ml_1p5()
                                    .pl_2()
                                    .border_l_1()
                                    .border_color(theme::hairline())
                                    .text_color(theme::text_dim())
                                    .hover(|s| s.text_color(theme::text()).bg(theme::hover_bg()))
                                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                        cx.stop_propagation()
                                    })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.store.update(cx, |s, cx| s.cancel_all_wands(cx));
                                    }))
                                    .child("Cancel"),
                            )
                        }),
                )
            })
            .when(self.section == Section::Repos, |row| {
                row.child(title_icon(
                    "list-toggle",
                    "Toggle list",
                    "icons/icon-panel-left.svg",
                    show_tooltips,
                    cx.listener(|this, _, _, cx| {
                        this.repos_page.update(cx, |page, cx| page.toggle_list(cx));
                    }),
                ))
            })
            .child(div().mx_1().h(px(20.)).w(px(1.)).bg(theme::hairline()))
            .child(
                // box to box: the row's gap belongs to the buttons before them
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .child(window_button(
                        "minimize-window",
                        "icons/icon-window-minimize.svg",
                        theme::hover_bg().into(),
                        "Minimize",
                        show_tooltips,
                        |_, window, _| window.minimize_window(),
                    ))
                    .child(window_button_glyph(
                        "zoom",
                        theme::hover_bg().into(),
                        if window.is_maximized() {
                            "Restore"
                        } else {
                            "Maximize"
                        },
                        show_tooltips,
                        cx.listener(|_, _, window, cx| {
                            window.zoom_window();
                            // the compositor toggles asynchronously — nudge a
                            // re-render so the maximize/restore glyph keeps up
                            cx.notify();
                        }),
                        shell::zoom_glyph(window.is_maximized(), theme::text_muted()),
                    ))
                    .child(window_button(
                        "close-window",
                        "icons/icon-window-close.svg",
                        theme::tint(theme::DANGER),
                        "Close",
                        show_tooltips,
                        |_, window, _| window.remove_window(),
                    )),
            );
        div()
            .h(px(theme::TITLEBAR_H))
            .flex()
            .flex_none()
            .items_center()
            .gap_2()
            .px(px(14.))
            .bg(theme::topbar_bg())
            .border_b_1()
            .border_color(theme::hairline())
            .debug_selector(|| "titlebar-drag".into())
            // the whole bar drags, not just the strips beside the search:
            // Windows turns this hitbox into HTCAPTION, and the buttons and
            // search field above it occlude/stop the press so they stay
            // clickable. Linux/macOS start the move on the first motion so a
            // plain click (and the headless probes) never enters a drag.
            .window_control_area(WindowControlArea::Drag)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    gpui_kit::base::GlobalState::suppress_text_selection(cx);
                    this.titlebar_should_move = true;
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| {
                    this.titlebar_should_move = false;
                }),
            )
            .on_mouse_move(
                cx.listener(|this, event: &gpui::MouseMoveEvent, window, _| {
                    if this.titlebar_should_move {
                        this.titlebar_should_move = false;
                        // a missed mouse-up (the OS move loop swallowed it)
                        // must not turn a later hover into a drag
                        if event.pressed_button == Some(MouseButton::Left) {
                            window.start_window_move();
                        }
                    }
                }),
            )
            .on_mouse_down_out(cx.listener(|this, _, _, _| {
                this.titlebar_should_move = false;
            }))
            // the left column: the app's own mark and name, then the family's
            // links. It is ONE flexible column so the search field between it
            // and the right cluster lands on the window's centre — the links
            // used to be a fixed-width sibling outside it, which pushed the
            // field ~46px right of centre.
            .child(
                div()
                    .debug_selector(|| "titlebar-left".into())
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .debug_selector(|| "titlebar-brand".into())
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .gap_2p5()
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .size(px(28.))
                                    .rounded(px(6.))
                                    .bg(theme::accent_soft())
                                    .child(icon(
                                        "icons/logo-glyph.svg",
                                        px(theme::icon_small()),
                                        theme::accent_ink(),
                                    )),
                            )
                            .child(
                                div()
                                    .text_size(px(theme::font_brand()))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme::text())
                                    .whitespace_nowrap()
                                    .child("Warp Station"),
                            ),
                    )
                    // the family: the sibling apps this window can open, right
                    // after its own mark and name, behind one separator
                    .child(
                        shell::app_links()
                            .when(settings.warp_notes.enable_warp_notes, |row| {
                                row.child(
                                    shell::app_link(
                                        "notes-toggle",
                                        "icons/icon-warp-notes.svg",
                                        "Open warp-notes",
                                    )
                                    .when(show_tooltips, |d| {
                                        d.tooltip(crate::ui::tooltip_text("Open warp-notes"))
                                    })
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.launch_notes(None, cx)),
                                    ),
                                )
                            })
                            .when(settings.warp_agent.enable_warp_agent, |row| {
                                row.child(
                                    shell::app_link(
                                        "agent-toggle",
                                        "icons/icon-warp-agent.svg",
                                        "Open warp-agent",
                                    )
                                    .when(show_tooltips, |d| {
                                        d.tooltip(crate::ui::tooltip_text("Open warp-agent"))
                                    })
                                    .on_click(cx.listener(
                                        |this, _, _, cx| {
                                            this.store.update(cx, |s, cx| {
                                                s.open_in_warp("general".into(), cx)
                                            });
                                        },
                                    )),
                                )
                            }),
                    ),
            )
            .child(
                div()
                    // the input is a control island inside the drag region:
                    // occlude it for the Windows hit test and keep its press
                    // from starting a window move on Linux
                    .occlude()
                    .w(px(SEARCH_W))
                    .min_w(px(160.))
                    .flex_shrink_1()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        warpui_kit_core::field::boxed(
                            "global-search-field",
                            &self.global_search.read(cx).focus_handle(cx),
                            window,
                            warpui_kit_core::field::title_height(),
                        )
                        .child(icon(
                            "icons/icon-search.svg",
                            px(theme::icon_small()),
                            theme::text_muted(),
                        ))
                        .child(warpui_kit_core::field::input(&self.global_search).cleanable(true))
                        .child(warpui_kit_core::field::kbd("Ctrl K")),
                    ),
            )
            .child(right)
            .into_any_element()
    }

    /// One section of the rail.
    fn rail_item(
        &self,
        section: Section,
        id: &'static str,
        icon_path: &str,
        label: &'static str,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        shell::rail_item(id, icon_path, label, self.section == section)
            .debug_selector(move || id.into())
            .on_click(cx.listener(move |this, _, _, cx| {
                this.section = section;
                cx.notify();
            }))
    }

    fn render_rail(&mut self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let attention = self
            .store
            .read(cx)
            .repos
            .iter()
            .filter(|repo| !repo.is_archived() && crate::store::projection::needs_attention(repo))
            .count();
        shell::rail()
            .debug_selector(|| "app-rail".into())
            .child(
                self.rail_item(
                    Section::Repos,
                    "rail-repos",
                    "icons/icon-folder.svg",
                    "Repos",
                    cx,
                )
                .relative()
                .when(attention > 0, |item| {
                    item.child(
                        div()
                            .absolute()
                            .top(px(4.))
                            .right(px(10.))
                            .min_w(px(16.))
                            .h(px(16.))
                            .px_1()
                            .rounded(px(8.))
                            .bg(theme::accent())
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(theme::font_micro()))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme::on_accent())
                            .child(format!("{attention}")),
                    )
                }),
            )
            .child(self.rail_item(
                Section::Summary,
                "rail-summary",
                "icons/icon-agent-sparkle.svg",
                "Summary",
                cx,
            ))
            .child(self.rail_item(
                Section::Tools,
                "rail-tools",
                "icons/icon-tools.svg",
                "Tools",
                cx,
            ))
            .child(div().flex_1())
            .child(self.rail_item(
                Section::Settings,
                "rail-settings",
                "icons/icon-cog.svg",
                "Settings",
                cx,
            ))
            .into_any_element()
    }

    fn render_body(&mut self, window: &mut Window, cx: &mut Context<Self>) -> gpui::AnyElement {
        let body = div().flex().flex_1().min_w_0().min_h_0();
        match self.section {
            Section::Repos => body.child(self.repos_page.clone()).into_any_element(),
            Section::Summary => body.child(self.summary_page.clone()).into_any_element(),
            Section::Tools => body.child(self.render_tools(window, cx)).into_any_element(),
            Section::Settings => body.child(self.settings_page.clone()).into_any_element(),
        }
    }

    /// The Tools section: a row per tool the settings allow, its page beside
    /// the list. Pages are built the first time they open and keep their state.
    fn render_tools(&mut self, window: &mut Window, cx: &mut Context<Self>) -> gpui::AnyElement {
        let settings = self.store.read(cx).settings();
        let watching = self.store.read(cx).nuget.watching;
        let mut tools = vec![(
            Tool::Formatters,
            "tool-formatters",
            "Formatters",
            "Base64 and case conversion",
        )];
        if settings.clipboard_password.enable_clipboard_password {
            tools.push((
                Tool::ClipboardPassword,
                "tool-clipboard",
                "Clipboard Password",
                "One stored password",
            ));
        }
        if settings.nuget_local.enable_nuget {
            tools.push((
                Tool::Nuget,
                "tool-nuget",
                "NuGet Package Manager",
                if watching {
                    "Watching for builds"
                } else {
                    "Watch is idle"
                },
            ));
        }
        if cfg!(windows) {
            tools.push((
                Tool::SnapIt,
                "tool-snapit",
                "SnapIt",
                if warp_station_core::snapit::is_running() {
                    "Running"
                } else {
                    "Stopped"
                },
            ));
            tools.push((
                Tool::KeepAwake,
                "tool-keep-awake",
                "Keep Awake",
                if warp_station_core::keepawake::is_enabled() {
                    "On"
                } else {
                    "Off"
                },
            ));
        }
        // a tool switched off in Settings falls back to the first
        let active = if tools.iter().any(|tool| tool.0 == self.tool) {
            self.tool
        } else {
            Tool::Formatters
        };

        let mut rows = div().flex().flex_col().pt_1();
        for (tool, id, label, hint) in tools {
            rows = rows.child(
                shell::list_row(id, tool == active)
                    .debug_selector(move || id.into())
                    .a11y_tab(label, tool == active)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.tool = tool;
                        cx.notify();
                    }))
                    .child(
                        div()
                            .text_size(px(theme::font_name()))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme::text())
                            .child(label),
                    )
                    .child(
                        div()
                            .mt_0p5()
                            .text_size(px(theme::font_row_meta()))
                            .text_color(theme::text_dim())
                            .child(hint),
                    ),
            );
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
                    .text_size(px(theme::font_list_title()))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme::text())
                    .child(SharedString::from("Tools")),
            )
            .child(rows);

        let store = self.store.clone();
        let page = match active {
            Tool::Formatters => self
                .formatters
                .get_or_insert_with(|| cx.new(|cx| FormattersDrawer::new(store, window, cx)))
                .clone()
                .into_any_element(),
            Tool::Nuget => self
                .nuget_drawer
                .get_or_insert_with(|| cx.new(|cx| NugetDrawer::new(store, window, cx)))
                .clone()
                .into_any_element(),
            Tool::ClipboardPassword => self
                .clipboard_drawer
                .get_or_insert_with(|| cx.new(|cx| ClipboardPasswordDrawer::new(store, window, cx)))
                .clone()
                .into_any_element(),
            Tool::SnapIt => self
                .snapit_page
                .get_or_insert_with(|| cx.new(|_| SnapItPage::new(store)))
                .clone()
                .into_any_element(),
            Tool::KeepAwake => self
                .keep_awake_page
                .get_or_insert_with(|| cx.new(|_| KeepAwakePage::new(store)))
                .clone()
                .into_any_element(),
        };
        shell::panes()
            .child(list)
            .child(
                shell::detail_pane()
                    .debug_selector(|| "tool-page".into())
                    .child(page),
            )
            .into_any_element()
    }
}
