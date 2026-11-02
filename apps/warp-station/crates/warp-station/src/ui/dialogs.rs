//! The toolbar dialogs as centered overlay cards: Add Repositories (deep scan
//! with a pre-checked checklist; already-tracked repos locked) and Clone from
//! URL (auto-derived name, default destination from the first scan root).
//! Dialog-bound net results land on the store and are drained in the page's
//! render, the same pass that draws them.
//!
//! Both cards use the redesign's modal chrome: an 8px card on the ramp's card
//! step with a ruled 44px header and 50px footer, ghost/primary buttons at the
//! toolbar's 26px metric, and 30px fields. They are layers over whichever
//! layout is underneath — the split screen stays behind Add repo and Clone.

use crate::store::Store;
use crate::theme;
use crate::ui::A11y;
use crate::ui::icon;
use gpui::prelude::*;
use gpui::{Context, Entity, FontWeight, SharedString, Window, div, px};
use gpui_kit::component::Sizable as _;
use gpui_kit::component::input::{Input, InputEvent, InputState};

use crate::ui::repos::ReposPage;

pub enum Dialog {
    AddRepo(AddRepoDialog),
    Clone(CloneDialog),
}

impl Dialog {
    pub fn render(&mut self, window: &mut Window, cx: &mut Context<ReposPage>) -> gpui::AnyElement {
        match self {
            Dialog::AddRepo(dialog) => dialog.render(window, cx),
            Dialog::Clone(dialog) => dialog.render(window, cx),
        }
    }
}

/// The last URL path segment minus .git — the clone's folder name.
pub fn derive_repo_name(url: &str) -> String {
    let trimmed = url.trim().trim_end_matches('/');
    let last = trimmed.rsplit(['/']).next().unwrap_or("");
    let last = last.strip_suffix(".git").unwrap_or(last);
    if last.is_empty() {
        "repo".to_string()
    } else {
        last.to_string()
    }
}

/// The card width: one comfortable column of fields, a step narrower than the
/// 540px rail so the dialog never reads as a second pane.
const CARD_W: f32 = 520.;

/// Scrim plus the centered card. The scrim is the family's
/// (`00000073`), so every overlay in the app dims the page by the same amount,
/// and a click on it dismisses regardless of where the pointer landed.
fn overlay_shell(
    id: &'static str,
    cx: &mut Context<ReposPage>,
    body: gpui::Div,
) -> gpui::AnyElement {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .a11y_button("Close dialog")
        .absolute()
        .inset_0()
        .occlude()
        .bg(theme::SCRIM)
        .flex()
        .items_center()
        .justify_center()
        .p_4()
        .on_click(cx.listener(|this: &mut ReposPage, _, _, cx| {
            this.dialog = None;
            cx.notify();
        }))
        .child(warpui_kit_core::motion::enter(
            SharedString::from(format!("{id}-enter")),
            body.id("dialog-card-body")
                .role(gpui::Role::Dialog)
                .occlude()
                .on_click(|_, _, _| {}),
        ))
        .into_any_element()
}

/// The dialog card: 8px corners, 1px `LINE_STRONG` edge, one step of the ramp
/// above the page, and a ruled header — the mockup's panel language rather
/// than the 12px popup the pre-redesign app used.
///
/// `close_id` is the debug selector the probes click; each dialog passes its
/// own so both headers stay addressable.
fn dialog_card(
    title: &str,
    icon_path: &'static str,
    close_id: &'static str,
    on_close: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> gpui::Div {
    div()
        .w(px(CARD_W))
        .rounded(px(8.))
        .border_1()
        .border_color(theme::LINE_STRONG)
        .bg(theme::CARD)
        .shadow_lg()
        .overflow_hidden()
        .flex()
        .flex_col()
        .child(
            div()
                .h(px(44.))
                .px(px(16.))
                .flex()
                .flex_row()
                .items_center()
                .gap(px(8.))
                .border_b_1()
                .border_color(theme::ROW_LINE)
                .child(icon(icon_path, px(theme::icon_small()), theme::TEXT_DIM))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(px(theme::font_brand()))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::TEXT)
                        .child(title.to_string()),
                )
                .child(
                    div()
                        .id(close_id)
                        .debug_selector(move || close_id.into())
                        .a11y_button("Close")
                        .cursor_pointer()
                        .size(px(24.))
                        .rounded(px(4.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .hover(|s| s.bg(theme::HOVER_BG))
                        .on_click(on_close)
                        .child(icon("icons/icon-close.svg", px(12.), theme::TEXT_MUTED)),
                ),
        )
}

/// The card's content band: the fields sit between the two header/footer
/// rules with their own padding, so the rules run edge to edge.
fn dialog_body() -> gpui::Div {
    div().flex().flex_col().gap(px(12.)).px(px(16.)).py(px(14.))
}

/// The card's action band: one ruled row, actions right-aligned — Cancel
/// left of the single filled action, exactly like the toolbar's pairing.
fn dialog_footer() -> gpui::Div {
    div()
        .h(px(50.))
        .px(px(16.))
        .flex()
        .flex_row()
        .items_center()
        .justify_end()
        .gap(px(8.))
        .border_t_1()
        .border_color(theme::ROW_LINE)
}

/// A field label on the row meta step.
fn field_label(label: &str) -> gpui::Div {
    div()
        .text_size(px(theme::font_row_meta()))
        .text_color(theme::TEXT_MUTED)
        .child(label.to_string())
}

/// The dialogs' text field: the kit input pulled down to the design's field
/// metric — 30px high, 10.5px type, the recessed search fill and its border —
/// instead of the kit's default 38px/16px, which was the loudest leftover of
/// the old look inside the new chrome.
fn dialog_field(id: &'static str, input: &Entity<InputState>) -> gpui::Stateful<gpui::Div> {
    div().id(id).debug_selector(move || id.into()).child(
        Input::new(input)
            .with_size(theme::field_size())
            .h(px(30.))
            .px(px(8.))
            .rounded(px(4.))
            .bg(theme::SEARCH_BG)
            .border_color(theme::SEARCH_BORDER)
            .text_size(px(theme::font_sub())),
    )
}

/// A ghost action at the pane-head metric (26px, 4px corners, the
/// `#0f1318`/`#2a313b` pair) so a dialog button and a toolbar button read as
/// the same control.
fn ghost_button(
    label: &str,
    enabled: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> gpui::AnyElement {
    let id = format!("btn-{label}");
    div()
        .id(SharedString::from(id.clone()))
        .debug_selector(move || id.clone())
        .a11y_button(label.to_string())
        .rounded(px(4.))
        .h(px(26.))
        .px(px(9.))
        .flex()
        .items_center()
        .text_size(px(theme::font_control()))
        .font_weight(FontWeight::MEDIUM)
        .border_1()
        .border_color(theme::GHOST_BORDER)
        .bg(theme::GHOST_BG)
        .map(|d| {
            if enabled {
                d.cursor_pointer().text_color(theme::TEXT_DIM).hover(|s| {
                    s.bg(theme::HOVER_BG)
                        .border_color(theme::GHOST_BORDER_HOVER)
                        .text_color(theme::TEXT)
                })
            } else {
                d.text_color(theme::TEXT_MUTED).opacity(0.45)
            }
        })
        .when(enabled, |d| d.on_click(on_click))
        .child(label.to_string())
        .into_any_element()
}

/// A ghost button in its working state: same chrome, a spinner in place of the
/// press, and the verb's present participle.
fn ghost_busy(label: &'static str) -> gpui::AnyElement {
    let id = format!("btn-{label}");
    div()
        .id(SharedString::from(id.clone()))
        .debug_selector(move || id.clone())
        .role(gpui::Role::Status)
        .aria_label(label)
        .rounded(px(4.))
        .h(px(26.))
        .px(px(9.))
        .flex()
        .items_center()
        .gap(px(6.))
        .text_size(px(theme::font_control()))
        .font_weight(FontWeight::MEDIUM)
        .border_1()
        .border_color(theme::GHOST_BORDER)
        .bg(theme::GHOST_BG)
        .text_color(theme::TEXT_DIM)
        .child(warpui_kit_core::motion::spinner(
            "dialog-scan-spinner",
            "icons/icon-spinner.svg",
            px(theme::icon_tiny()),
            theme::TEXT_DIM,
        ))
        .child(label)
        .into_any_element()
}

/// The one filled action: the mockup's blue at 26px, darkening to
/// `ACCENT_HOVER` on hover like every primary.
fn primary_button(
    label: &str,
    enabled: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> gpui::AnyElement {
    let id = format!("btn-accent-{label}");
    div()
        .id(SharedString::from(id.clone()))
        .debug_selector(move || id.clone())
        .a11y_button(label.to_string())
        .rounded(px(4.))
        .h(px(26.))
        .px(px(12.))
        .flex()
        .items_center()
        .text_size(px(theme::font_control()))
        .font_weight(FontWeight::MEDIUM)
        .bg(theme::ACCENT)
        .text_color(theme::ON_ACCENT)
        .map(|d| {
            if enabled {
                d.cursor_pointer().hover(|s| s.bg(theme::ACCENT_HOVER))
            } else {
                d.opacity(0.4)
            }
        })
        .when(enabled, |d| d.on_click(on_click))
        .child(label.to_string())
        .into_any_element()
}

pub struct AddRepoDialog {
    pub root: Entity<InputState>,
    /// Scan depth for this dialog, seeded from the settings; changing it here
    /// does not persist (Settings owns the stored value).
    pub depth: i32,
    pub scanning: bool,
    /// true once a scan has settled — distinguishes "never scanned" from
    /// "scanned and found nothing"
    pub scanned: bool,
    /// (repo, checked, already-tracked)
    pub found: Vec<(warp_station_core::scan::ScannedRepo, bool, bool)>,
}

impl AddRepoDialog {
    pub fn new(window: &mut Window, depth: i32, cx: &mut Context<ReposPage>) -> Self {
        let root =
            cx.new(|cx| InputState::new(window, cx).placeholder("Pick or type a folder path…"));
        cx.subscribe(&root, |this: &mut ReposPage, _, event: &InputEvent, cx| {
            if let InputEvent::PressEnter { .. } = event {
                this.kick_deep_scan(cx);
            }
            // the Scan button enables as soon as a path is typed
            cx.notify();
        })
        .detach();
        Self {
            root,
            depth,
            scanning: false,
            scanned: false,
            found: Vec::new(),
        }
    }

    fn render(&mut self, window: &mut Window, cx: &mut Context<ReposPage>) -> gpui::AnyElement {
        let selectable = self
            .found
            .iter()
            .filter(|(_, _, tracked)| !*tracked)
            .count();
        let selected = self
            .found
            .iter()
            .filter(|(_, checked, tracked)| *checked && !*tracked)
            .count();
        let all_checked = selectable > 0
            && self
                .found
                .iter()
                .all(|(_, checked, tracked)| *tracked || *checked);

        // ---- results well --------------------------------------------------
        // leave room for the card chrome (44 header + 14/14 body padding +
        // label/field/scan rows + 50 footer) and the scrim's 16px inset, so a
        // long scan result list scrolls inside the well instead of pushing
        // Cancel/Add out of the dialog on short windows
        let results_h = (window.viewport_size().height.as_f32() - 280.).clamp(140., 360.);
        let card_center_text = if self.scanning {
            "Scanning…"
        } else if self.found.is_empty() && self.scanned {
            "No repositories found under that folder — try another folder, or raise the depth"
        } else {
            "Scan a folder to find git repositories."
        };

        let mut rows = div().flex().flex_col().gap(px(1.)).py(px(4.));
        if selectable > 0 {
            rows = rows.child(
                div()
                    .id("select-all-row")
                    .a11y_toggle(format!("Select all available ({selectable})"), all_checked)
                    .cursor_pointer()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(8.))
                    .h(px(28.))
                    .px(px(8.))
                    .border_b_1()
                    .border_color(theme::ROW_LINE)
                    .text_size(px(theme::font_row_meta()))
                    .text_color(theme::TEXT_MUTED)
                    .hover(|s| s.bg(theme::HOVER_BG))
                    .on_click(cx.listener(move |this: &mut ReposPage, _, _, cx| {
                        if let Some(Dialog::AddRepo(dialog)) = &mut this.dialog {
                            // three-state header checkbox touches only the
                            // selectable rows
                            let target = !all_checked;
                            for (_, checked, tracked) in dialog.found.iter_mut() {
                                if !*tracked {
                                    *checked = target;
                                }
                            }
                        }
                        cx.notify();
                    }))
                    .child(
                        div()
                            .rounded(px(2.))
                            .size(px(12.))
                            .border_1()
                            .flex()
                            .items_center()
                            .justify_center()
                            .border_color(theme::GHOST_BORDER)
                            .when(all_checked, |d| {
                                d.bg(theme::ACCENT).border_color(theme::ACCENT).child(icon(
                                    "icons/icon-check.svg",
                                    px(8.),
                                    theme::ON_ACCENT,
                                ))
                            }),
                    )
                    .child(format!("Select all available ({selectable})")),
            );
        }
        for (index, (repo, checked, tracked)) in self.found.iter().enumerate() {
            let label: SharedString = if *tracked {
                format!("{}   (already tracked)", repo.name).into()
            } else {
                repo.name.clone().into()
            };
            rows = rows.child(
                div()
                    .id(("found-repo", index as u64))
                    .debug_selector(move || format!("found-repo-{index}"))
                    .a11y_toggle(label.to_string(), *checked && !*tracked)
                    .cursor_pointer()
                    .rounded(px(4.))
                    .h(px(26.))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .text_size(px(theme::font_sub()))
                    .text_color(theme::TEXT_DIM)
                    .when(*checked && !*tracked, |d| {
                        d.bg(theme::tint(theme::ACCENT)).text_color(theme::TEXT)
                    })
                    .when(*tracked, |d| d.opacity(0.45))
                    .hover(|s| s.bg(theme::HOVER_BG))
                    .on_click(cx.listener(move |this: &mut ReposPage, _, _, cx| {
                        if let Some(Dialog::AddRepo(dialog)) = &mut this.dialog
                            && !dialog.found[index].2
                        {
                            dialog.found[index].1 = !dialog.found[index].1;
                        }
                        cx.notify();
                    }))
                    .child(
                        // checkbox square
                        div()
                            .rounded(px(2.))
                            .size(px(12.))
                            .border_1()
                            .flex()
                            .items_center()
                            .justify_center()
                            .border_color(theme::GHOST_BORDER)
                            .when(*checked && !*tracked, |d| {
                                d.bg(theme::ACCENT).border_color(theme::ACCENT).child(icon(
                                    "icons/icon-check.svg",
                                    px(8.),
                                    theme::ON_ACCENT,
                                ))
                            }),
                    )
                    .child(label),
            );
        }

        let results_card = div()
            .id("add-results")
            .debug_selector(|| "add-results".into())
            .h(px(results_h))
            .rounded(px(6.))
            .border_1()
            .border_color(theme::ROWS_RULE)
            .bg(theme::SEARCH_BG)
            .overflow_hidden()
            .when(self.found.is_empty(), |card| {
                card.child(
                    div()
                        .size_full()
                        .p(px(24.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(theme::font_sub()))
                        .text_color(theme::TEXT_FAINT)
                        .child(card_center_text),
                )
            })
            .when(!self.found.is_empty(), |card| {
                card.child(
                    div()
                        .id("found-repo-list")
                        .h_full()
                        .overflow_y_scroll()
                        .flex()
                        .flex_col()
                        .px(px(4.))
                        .child(rows)
                        .when(selectable == 0, |column| {
                            column.child(
                                div()
                                    .px(px(8.))
                                    .py(px(6.))
                                    .text_size(px(theme::font_sub()))
                                    .text_color(theme::TEXT_FAINT)
                                    .child("All found repositories are already in your list."),
                            )
                        }),
                )
            });

        // ---- folder row + scan controls -----------------------------------
        let can_scan = !self.scanning && !self.root.read(cx).value().trim().is_empty();
        let depth_stepper = div()
            .h(px(24.))
            .flex()
            .flex_row()
            .items_center()
            .rounded(px(4.))
            .border_1()
            .border_color(theme::GHOST_BORDER)
            .bg(theme::GHOST_BG)
            .child(
                div()
                    .w(px(22.))
                    .flex()
                    .justify_center()
                    .text_size(px(theme::font_row_meta()))
                    .text_color(theme::TEXT)
                    .child(format!("{}", self.depth)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .border_l_1()
                    .border_color(theme::GHOST_BORDER)
                    .child(
                        div()
                            .id("depth-up")
                            .a11y_button("Increase scan depth")
                            .cursor_pointer()
                            .w(px(16.))
                            .h(px(11.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .hover(|s| s.bg(theme::HOVER_BG))
                            .on_click(cx.listener(|this: &mut ReposPage, _, _, cx| {
                                if let Some(Dialog::AddRepo(dialog)) = &mut this.dialog {
                                    dialog.depth = (dialog.depth + 1).min(8);
                                    cx.notify();
                                }
                            }))
                            .child(icon("icons/icon-arrow-up.svg", px(7.), theme::TEXT_MUTED)),
                    )
                    .child(
                        div()
                            .id("depth-down")
                            .a11y_button("Decrease scan depth")
                            .cursor_pointer()
                            .w(px(16.))
                            .h(px(11.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .hover(|s| s.bg(theme::HOVER_BG))
                            .on_click(cx.listener(|this: &mut ReposPage, _, _, cx| {
                                if let Some(Dialog::AddRepo(dialog)) = &mut this.dialog {
                                    dialog.depth = (dialog.depth - 1).max(1);
                                    cx.notify();
                                }
                            }))
                            .child(icon("icons/icon-arrow-down.svg", px(7.), theme::TEXT_MUTED)),
                    ),
            );

        let body = dialog_card(
            "Add Repositories",
            "icons/icon-plus.svg",
            "add-dialog-close",
            cx.listener(|this: &mut ReposPage, _, _, cx| {
                this.dialog = None;
                cx.notify();
            }),
        )
        .child(
            dialog_body()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(6.))
                        .child(field_label("Folder to scan"))
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap(px(6.))
                                .child(dialog_field("add-root-input", &self.root).flex_1())
                                .child(ghost_button(
                                    "Browse…",
                                    !self.scanning,
                                    cx.listener(|this: &mut ReposPage, _, _, cx| {
                                        this.store.update(cx, |s, _| {
                                            let _ = s.net.send_blocking(
                                                crate::net::NetCommand::BrowseFolder,
                                            );
                                        });
                                    }),
                                )),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(8.))
                        .child(if self.scanning {
                            ghost_busy("Scanning…")
                        } else {
                            ghost_button(
                                "Scan",
                                can_scan,
                                cx.listener(|this: &mut ReposPage, _, _, cx| {
                                    this.kick_deep_scan(cx)
                                }),
                            )
                        })
                        .child(field_label("Depth"))
                        .child(depth_stepper)
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_size(px(theme::font_row_meta()))
                                .text_color(theme::TEXT_FAINT)
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .child("Pick or type a folder, then scan for git repositories"),
                        ),
                )
                .child(results_card),
        )
        .child(
            dialog_footer()
                .child(ghost_button(
                    "Cancel",
                    true,
                    cx.listener(|this: &mut ReposPage, _, _, cx| {
                        this.dialog = None;
                        cx.notify();
                    }),
                ))
                .child(primary_button(
                    &if selected > 0 {
                        format!("Add {selected}")
                    } else {
                        "Add".to_string()
                    },
                    !self.scanning && selected > 0,
                    cx.listener(|this: &mut ReposPage, _, _, cx| this.commit_add_dialog(cx)),
                )),
        );
        overlay_shell("add-repo-overlay", cx, body)
    }
}

pub struct CloneDialog {
    pub url: Entity<InputState>,
    /// The clone's folder name; left empty it follows the URL's derivation
    /// (the field's "repo" placeholder shows that fallback).
    pub name: Entity<InputState>,
    pub destination: Entity<InputState>,
    pub busy: bool,
    pub error: Option<String>,
}

impl CloneDialog {
    pub fn new(store: &Entity<Store>, window: &mut Window, cx: &mut Context<ReposPage>) -> Self {
        let url = cx
            .new(|cx| InputState::new(window, cx).placeholder("https://github.com/owner/repo.git"));
        cx.subscribe(&url, |this: &mut ReposPage, _, event: &InputEvent, cx| {
            if let InputEvent::PressEnter { .. } = event {
                this.commit_clone(cx);
            }
            // the Clone button enables as soon as the URL has a usable shape
            cx.notify();
        })
        .detach();
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("repo"));
        // default destination: the PARENT of the first existing scan
        // root — roots are repo folders themselves, and cloning into one would
        // nest a repo inside another repo's working tree
        let default_dest = store
            .read(cx)
            .settings()
            .repos
            .repo_scan_folders
            .iter()
            .flatten()
            .find(|f| std::path::Path::new(f).is_dir())
            .and_then(|f| {
                std::path::Path::new(f)
                    .parent()
                    .map(|p| p.to_string_lossy().to_string())
            })
            .unwrap_or_default();
        let destination = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("path to clone into")
                .default_value(default_dest)
        });
        Self {
            url,
            name,
            destination,
            busy: false,
            error: None,
        }
    }

    /// The effective clone folder: the typed Folder Name wins; otherwise the
    /// name derives from the URL, so pasting a link is enough.
    pub fn folder_name(&self, cx: &Context<ReposPage>) -> String {
        let typed = self.name.read(cx).value().trim().to_string();
        if typed.is_empty() {
            derive_repo_name(self.url.read(cx).value().as_ref())
        } else {
            typed
        }
    }

    fn render(&mut self, _window: &mut Window, cx: &mut Context<ReposPage>) -> gpui::AnyElement {
        let name = self.folder_name(cx);
        let error = self.error.clone();
        let busy = self.busy;
        let raw_url = self.url.read(cx).value().to_string();
        let dest = self.destination.read(cx).value().to_string();
        // Each field checks itself, beside itself; only the backend's failure
        // belongs to the whole dialog.
        let url_error = if raw_url.trim().is_empty() {
            Some("Enter a repository URL")
        } else if !raw_url.contains('/') && !raw_url.contains(':') {
            Some("That does not look like a repository URL")
        } else {
            None
        };
        let dest_error = if dest.trim().is_empty() {
            Some("Enter the destination folder")
        } else if !std::path::Path::new(&dest).is_dir() {
            Some("That folder does not exist")
        } else {
            None
        };
        let name_error = if name.is_empty() || name == "." || name == ".." {
            Some("Enter a folder name")
        } else if name.contains('/') || name.contains('\\') {
            Some("A folder name cannot contain a path")
        } else if dest_error.is_none() && std::path::Path::new(&dest).join(&name).exists() {
            Some("That folder already exists")
        } else {
            None
        };
        let valid = url_error.is_none() && dest_error.is_none() && name_error.is_none();
        let target_line = if dest_error.is_some() {
            String::new()
        } else {
            format!("Clones into {}/{}", dest.trim_end_matches('/'), name)
        };
        let body = dialog_card(
            "Clone from URL",
            "icons/icon-cloud-download.svg",
            "clone-dialog-close",
            cx.listener(|this: &mut ReposPage, _, _, cx| {
                this.dialog = None;
                cx.notify();
            }),
        )
        .child(
            dialog_body()
                .child(form_field(
                    "Repository URL",
                    dialog_field("clone-url-input", &self.url),
                    url_error.map(str::to_string),
                ))
                .child(form_field(
                    "Destination Folder",
                    dialog_field("clone-dest-input", &self.destination),
                    dest_error.map(str::to_string),
                ))
                .child(form_field(
                    "Folder Name",
                    dialog_field("clone-name-input", &self.name),
                    name_error.map(str::to_string),
                ))
                .when(!target_line.is_empty(), |body| {
                    body.child(
                        div()
                            .font_family(theme::code_font())
                            .text_size(px(theme::font_mono_meta()))
                            .text_color(theme::TEXT_MUTED)
                            .child(target_line),
                    )
                })
                .when_some(error, |body, error| {
                    body.child(
                        div()
                            .text_size(px(theme::font_sub()))
                            .text_color(theme::DANGER)
                            .child(error),
                    )
                }),
        )
        .child(
            dialog_footer()
                .child(ghost_button(
                    "Cancel",
                    true,
                    cx.listener(|this: &mut ReposPage, _, _, cx| {
                        this.dialog = None;
                        cx.notify();
                    }),
                ))
                .child(primary_button(
                    if busy { "Cloning…" } else { "Clone" },
                    !busy && valid,
                    cx.listener(|this: &mut ReposPage, _, _, cx| this.commit_clone(cx)),
                )),
        );
        overlay_shell("clone-overlay", cx, body)
    }
}

/// A labeled form field: 9.5px label over the shared 30px field, then the
/// field's own complaint when it has one. The selector stays on
/// [`dialog_field`]'s wrapper, which holds the input alone — putting it on this
/// label-plus-input column would aim a probe's click at the gap between the two.
fn form_field(
    label: &str,
    field: gpui::Stateful<gpui::Div>,
    error: Option<String>,
) -> gpui::AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(6.))
        .child(field_label(label))
        .child(field)
        .when_some(error, |column, error| {
            column.child(
                div()
                    .text_size(px(theme::font_row_meta()))
                    .text_color(theme::DANGER)
                    .child(error),
            )
        })
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::derive_repo_name;

    #[test]
    fn derives_names_from_url_shapes() {
        assert_eq!(
            derive_repo_name("https://github.com/owner/repo.git"),
            "repo"
        );
        assert_eq!(derive_repo_name("https://github.com/owner/repo"), "repo");
        assert_eq!(
            derive_repo_name("git@github.com:owner/my-repo.git"),
            "my-repo"
        );
        assert_eq!(
            derive_repo_name("https://dev.azure.com/org/proj/_git/Thing"),
            "Thing"
        );
        assert_eq!(
            derive_repo_name("https://github.com/owner/repo.git/"),
            "repo"
        );
        assert_eq!(derive_repo_name(""), "repo");
    }
}
