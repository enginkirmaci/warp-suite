//! The Clipboard Password page: stores a password (Base64 over UTF-8 in the
//! settings file — the exact legacy format) and copies it back via the
//! Copy button or the system-wide Ctrl+Shift+V (net.rs registers the grab).

use crate::store::Store;
use crate::theme;
use crate::ui::A11y;
use gpui::prelude::*;
use gpui::{App, Context, Entity, FontWeight, Window, div, px};
use gpui_kit::component::Sizable as _;
use gpui_kit::component::input::{Input, InputEvent, InputState};

pub struct ClipboardPasswordDrawer {
    pub store: Entity<Store>,
    password: Entity<InputState>,
    status: String,
    /// First click on "Clear Stored Password" arms the action; the next click
    /// within the window below actually clears the stored secret.
    clear_armed: Option<std::time::Instant>,
    /// A finished save asks the next render pass to clear the input
    /// (set_value needs a Window, which command paths don't carry).
    pending_clear: bool,
}

/// The stored password decoded, ready for the clipboard.
pub fn stored_password(settings: &warp_station_core::settings::AppSettings) -> Option<String> {
    settings
        .clipboard_password
        .encrypted_password
        .as_deref()
        .filter(|stored| !stored.is_empty())
        .and_then(warp_station_core::clipboard_password::decode)
}

/// Copies the stored password and returns the toast text.
pub fn copy_stored_password(store: &Entity<Store>, cx: &mut App) -> &'static str {
    match stored_password(&store.read(cx).settings()) {
        Some(password) => {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(password));
            "Password copied to clipboard"
        }
        None => "No password stored",
    }
}

fn status_line(has_stored: bool) -> &'static str {
    if has_stored {
        "Password is stored."
    } else {
        "No password stored yet"
    }
}

impl ClipboardPasswordDrawer {
    pub fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let has_stored = stored_password(&store.read(cx).settings()).is_some();
        let password = cx.new(|cx| {
            let state = InputState::new(window, cx).masked(true);
            if has_stored {
                state.placeholder("Stored — type a new password")
            } else {
                state.placeholder("Type your password here…")
            }
        });
        cx.subscribe(&password, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                this.save(cx);
            }
            // the Save button enables as soon as there is something to save
            cx.notify();
        })
        .detach();
        Self {
            store,
            password,
            status: status_line(has_stored).to_string(),
            clear_armed: None,
            pending_clear: false,
        }
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        let value = self.password.read(cx).value().trim().to_string();
        if value.is_empty() {
            self.status = "Please enter a password".into();
            return;
        }
        let encoded = warp_station_core::clipboard_password::encode(&value);
        self.store.update(cx, |store, _| {
            store.update_settings(|s| s.clipboard_password.encrypted_password = Some(encoded));
        });
        self.pending_clear = true;
        self.clear_armed = None;
        self.status = "Password saved successfully! Use Ctrl+Shift+V to paste it.".into();
        self.toast("Password saved", cx);
    }

    fn clear(&mut self, cx: &mut Context<Self>) {
        self.store.update(cx, |store, _| {
            store.update_settings(|s| s.clipboard_password.encrypted_password = None);
        });
        self.pending_clear = true;
        self.clear_armed = None;
        self.status = "Password cleared".into();
        self.toast("Password cleared", cx);
    }

    fn copy(&mut self, cx: &mut Context<Self>) {
        self.clear_armed = None;
        let message = copy_stored_password(&self.store.clone(), cx);
        self.toast(message, cx);
    }

    fn toast(&mut self, message: &'static str, cx: &mut Context<Self>) {
        let kind = crate::store::NoticeKind::for_copy_message(message);
        self.store.update(cx, |store, cx| {
            store.notify(kind, message.to_string());
            cx.notify();
        });
        cx.notify();
    }
}

impl Render for ClipboardPasswordDrawer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        if self.pending_clear {
            self.pending_clear = false;
            self.password
                .update(cx, |state, cx| state.set_value(String::new(), window, cx));
        }
        let stored = stored_password(&self.store.read(cx).settings()).is_some();
        // the confirm state auto-expires so the page never sits in a
        // destructive "armed" look after the user has moved on
        let clear_armed = self
            .clear_armed
            .is_some_and(|at| at.elapsed() < std::time::Duration::from_secs(4));
        if self.clear_armed.is_some() && !clear_armed {
            self.clear_armed = None;
            self.status = status_line(stored).to_string();
        }

        div()
            .id("clipboard-password-drawer")
            .size_full()
            .max_w(px(warpui_kit_core::shell::READING_WIDTH + 2. * warpui_kit_core::shell::DETAIL_PADDING))
            .p(px(warpui_kit_core::shell::DETAIL_PADDING))
            .flex()
            .flex_col()
            .gap_3()
            .occlude()
            .child(crate::ui::tool_header(
                "Clipboard Password",
                "Copy the stored password from anywhere with Ctrl+Shift+V.",
            ))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_size(px(theme::font_meta()))
                            .text_color(theme::TEXT_MUTED)
                            .child("Password"),
                    )
                    .child(Input::new(&self.password).with_size(theme::field_size()).mask_toggle()),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .justify_end()
                    .gap_2()
                    .child(ghost_button(
                        "clipboard-clear",
                        if clear_armed { "Confirm clear?" } else { "Clear Stored Password" },
                        theme::DANGER,
                        stored,
                        clear_armed,
                        cx.listener(move |this, _, _, cx| {
                            if clear_armed {
                                this.clear(cx);
                            } else {
                                let armed_at = std::time::Instant::now();
                                this.clear_armed = Some(armed_at);
                                this.status =
                                    "Click Clear again to remove the stored password.".into();
                                cx.notify();
                                // the armed look expires on its own — without
                                // this the solid danger button would sit armed
                                // until some unrelated render happened
                                cx.spawn(async move |this, cx| {
                                    cx.background_executor()
                                        .timer(std::time::Duration::from_secs(4))
                                        .await;
                                    let _ = this.update(cx, |this, cx| {
                                        if this.clear_armed == Some(armed_at) {
                                            this.clear_armed = None;
                                            let stored = stored_password(
                                                &this.store.read(cx).settings(),
                                            )
                                            .is_some();
                                            this.status = status_line(stored).to_string();
                                            cx.notify();
                                        }
                                    });
                                })
                                .detach();
                            }
                        }),
                    ))
                    .child(ghost_button(
                        "clipboard-copy",
                        "Copy",
                        theme::BLUE_BRIGHT,
                        stored,
                        false,
                        cx.listener(|this, _, _, cx| this.copy(cx)),
                    ))
                    .child(accent_button(
                        "clipboard-save",
                        "Save Password",
                        !self.password.read(cx).value().trim().is_empty(),
                        cx.listener(|this, _, _, cx| this.save(cx)),
                    )),
            )
            .child(
                div()
                    .text_size(px(theme::font_body()))
                    .text_color(theme::TEXT_DIM)
                    .child(self.status.clone()),
            )
            .child(
                div()
                    .text_size(px(theme::font_meta()))
                    .text_color(theme::TEXT_FAINT)
                    .child("Ctrl+Shift+V copies the stored password system-wide while Warp Station runs (focused-only where the OS refuses a global grab)."),
            )
    }
}

fn ghost_button(
    id: &'static str,
    label: &str,
    color: gpui::Rgba,
    enabled: bool,
    emphasized: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> gpui::AnyElement {
    div()
        .id(id)
        .a11y_button(label.to_string())
        .cursor_pointer()
        .rounded_md()
        .px_2p5()
        .py_1()
        .text_size(px(theme::font_body()))
        .font_weight(FontWeight::SEMIBOLD)
        // emphasized = the destructive confirm state: solid danger fill
        .bg(if emphasized {
            color.into()
        } else {
            theme::tint(color)
        })
        .text_color(if emphasized { theme::ON_DANGER } else { color })
        .map(|d| {
            if enabled {
                d.hover(|s| s.opacity(0.85))
            } else {
                d.opacity(0.4)
            }
        })
        .when(enabled, |d| d.on_click(on_click))
        .child(label.to_string())
        .into_any_element()
}

fn accent_button(
    id: &'static str,
    label: &str,
    enabled: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> gpui::AnyElement {
    div()
        .id(id)
        .a11y_button(label.to_string())
        .cursor_pointer()
        .rounded_md()
        .px_3()
        .py_1()
        .text_size(px(theme::font_body()))
        .font_weight(FontWeight::SEMIBOLD)
        .bg(theme::ACCENT)
        .text_color(theme::ON_ACCENT)
        .map(|d| {
            if enabled {
                d.hover(|s| s.opacity(0.9))
            } else {
                d.opacity(0.4)
            }
        })
        .when(enabled, |d| d.on_click(on_click))
        .child(label.to_string())
        .into_any_element()
}
