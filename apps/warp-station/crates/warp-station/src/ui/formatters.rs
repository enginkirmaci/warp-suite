//! The Formatters page: one input box, four operations (Base64
//! encode/decode, snake_case, PascalCase) and a recent-results history you
//! can click back into the input.

use crate::store::Store;
use crate::theme;
use crate::ui::A11y;
use gpui::prelude::*;
use gpui::{Context, Entity, FontWeight, SharedString, Window, div, px};
use gpui_kit::component::input::{InputEvent, TextareaState};

#[derive(Clone, Copy, PartialEq)]
enum FormatterMode {
    Base64Encode,
    Base64Decode,
    SnakeCase,
    PascalCase,
}

impl FormatterMode {
    fn label(&self) -> &'static str {
        match self {
            FormatterMode::Base64Encode => "Base64 Encode",
            FormatterMode::Base64Decode => "Base64 Decode",
            FormatterMode::SnakeCase => "Snake Case",
            FormatterMode::PascalCase => "Pascal Case",
        }
    }

    /// Runs one line (each non-empty line converts separately; invalid
    /// Base64 names the line).
    fn run_line(&self, input: &str) -> Result<String, String> {
        match self {
            FormatterMode::Base64Encode => Ok(b64_encode(input.as_bytes())),
            FormatterMode::Base64Decode => b64_decode(input.trim())
                .map(|bytes| String::from_utf8_lossy(&bytes).to_string())
                .map_err(|_| format!("[Invalid Base64: {input}]")),
            FormatterMode::SnakeCase => Ok(to_snake_case(input)),
            FormatterMode::PascalCase => Ok(to_pascal_case(input)),
        }
    }

    fn run(&self, input: &str) -> Result<String, String> {
        let lines: Vec<Result<String, String>> = input
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| self.run_line(line))
            .collect();
        if lines.is_empty() {
            return Err(String::new());
        }
        let mut out = Vec::with_capacity(lines.len());
        for line in lines {
            out.push(line?);
        }
        Ok(out.join("\n"))
    }
}

pub struct FormattersDrawer {
    pub store: Entity<Store>,
    mode: FormatterMode,
    input: Entity<TextareaState>,
    output: String,
    history: Vec<String>,
    /// A finished convert asks the next render pass to clear the input
    /// (set_value needs a Window, which command paths don't carry).
    pending_clear: bool,
}

impl FormattersDrawer {
    pub fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        // the design's `textarea.area` takes a pasted block, so one line per
        // result; Enter still runs it (Shift+Enter is the newline)
        let input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Paste text…")
                .submit_on_enter(true)
        });
        cx.subscribe(&input, |this, field, event: &InputEvent, cx| {
            if let InputEvent::PressEnter { .. } = event {
                this.run(field.read(cx).value().to_string());
            }
            // keeps the Run button's enabled state in step with the field
            cx.notify();
        })
        .detach();
        Self {
            store,
            mode: FormatterMode::Base64Encode,
            input,
            output: String::new(),
            history: Vec::new(),
            pending_clear: false,
        }
    }

    fn run(&mut self, text: String) {
        if text.trim().is_empty() {
            return;
        }
        match self.mode.run(&text) {
            Ok(result) => {
                self.output = result.clone();
                // newest-first, batch order preserved, cap 100; the input
                // clears after a convert
                for line in result.lines().rev() {
                    self.history.insert(0, line.to_string());
                }
                self.history.truncate(100);
                self.pending_clear = true;
            }
            Err(err) => self.output = err,
        }
    }
}

impl Render for FormattersDrawer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        if self.pending_clear {
            self.pending_clear = false;
            self.input
                .update(cx, |state, cx| state.set_value(String::new(), window, cx));
        }
        // the design's segmented control: a recessed well trough, the active
        // mode on the raised step — no accent on a plain mode switch
        let mut modes = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_0p5()
            .p(px(3.))
            .rounded(px(8.))
            .bg(theme::well())
            .self_start();
        for mode in [
            FormatterMode::Base64Encode,
            FormatterMode::Base64Decode,
            FormatterMode::SnakeCase,
            FormatterMode::PascalCase,
        ] {
            let active = self.mode == mode;
            modes = modes.child(
                div()
                    .id(SharedString::from(format!("fmt-mode-{}", mode.label())))
                    .debug_selector({
                        let sel = format!("fmt-mode-{}", mode.label());
                        move || sel.clone()
                    })
                    .a11y_toggle(mode.label(), active)
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .justify_center()
                    .h(px(28.))
                    .px(px(12.))
                    .rounded(px(6.))
                    .text_size(px(theme::font_meta()))
                    .when(active, |d| {
                        d.bg(theme::raised())
                            .text_color(theme::text())
                            .font_weight(FontWeight::SEMIBOLD)
                    })
                    .when(!active, |d| {
                        d.text_color(theme::text_dim())
                            .hover(|s| s.text_color(theme::text()))
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.mode = mode;
                        cx.notify();
                    }))
                    .child(mode.label()),
            );
        }

        let mut history = div()
            .id("fmt-history-list")
            .flex()
            .flex_col()
            .max_h(px(240.))
            .overflow_y_scroll();
        for (index, item) in self.history.iter().enumerate() {
            history = history.child(
                div()
                    .id(("fmt-history", index as u64))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .rounded_md()
                    .px_1p5()
                    .py_0p5()
                    .hover(|s| s.bg(theme::hover_bg()))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(theme::font_meta()))
                            .font_family(gpui::SharedString::from(theme::code_font()))
                            .text_color(theme::text_faint())
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .child(truncate_middle(item, 48)),
                    )
                    .child(
                        div()
                            .id(("fmt-copy", index as u64))
                            .a11y_button(format!("Copy {item}"))
                            .cursor_pointer()
                            .rounded_md()
                            .px_1()
                            .text_size(px(theme::font_control()))
                            .text_color(theme::text_muted())
                            .hover(|s| s.text_color(theme::blue_bright()))
                            .on_click({
                                let item = item.clone();
                                let store = self.store.clone();
                                move |_, _, cx| {
                                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                        item.clone(),
                                    ));
                                    store.update(cx, |s, cx| {
                                        s.notify_success("Copied to clipboard");
                                        cx.notify();
                                    });
                                }
                            })
                            .child("copy"),
                    ),
            );
        }

        div()
            .id("formatters-drawer")
            .size_full()
            .max_w(px(
                warpui_kit_core::shell::READING_WIDTH + 2. * warpui_kit_core::shell::DETAIL_PADDING
            ))
            .p(px(warpui_kit_core::shell::DETAIL_PADDING))
            .flex()
            .flex_col()
            .gap_3()
            .occlude()
            .child(crate::ui::tool_header_actions(
                "Formatters",
                "Base64 and case conversion, one line at a time.",
                // Run is the page's one primary, and the design keeps it in
                // the head rather than beside the input
                crate::ui::primary_button(
                    "fmt-run",
                    "Run",
                    !self.input.read(cx).value().trim().is_empty(),
                    cx.listener(|this, _, _, cx| {
                        let text = this.input.read(cx).value().to_string();
                        this.run(text);
                        cx.notify();
                    }),
                ),
            ))
            .child(modes)
            .child(
                div().flex().flex_row().items_center().gap_2().child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .debug_selector(|| "fmt-input".into())
                        .child(
                            // the prototype's input is a six-row mono
                            // textarea (`.area.t-mono`)
                            warpui_kit_core::field::mono_area(
                                "fmt-input-field",
                                &self.input,
                                window,
                                cx,
                            )
                            .min_h(px(126.))
                            .debug_selector(|| "fmt-input-box".into()),
                        ),
                ),
            )
            .child(
                div()
                    .id("fmt-output")
                    .rounded_md()
                    .bg(theme::card())
                    .border_1()
                    .border_color(theme::border_light())
                    .p_2()
                    .min_h(px(72.))
                    .max_h(px(180.))
                    .overflow_y_scroll()
                    .text_size(px(theme::font_body()))
                    .font_family(gpui::SharedString::from(theme::code_font()))
                    .text_color(theme::text_dim())
                    .when(self.output.is_empty(), |d| {
                        d.text_color(theme::text_faint())
                    })
                    .child(if self.output.is_empty() {
                        "Result appears here.".to_string()
                    } else {
                        self.output.clone()
                    }),
            )
            .when(!self.history.is_empty(), |drawer| {
                drawer
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_size(px(theme::font_meta()))
                                    .text_color(theme::text_faint())
                                    .child("Recent"),
                            )
                            .child(div().flex_1())
                            .child(
                                div()
                                    .id("fmt-clear-history")
                                    .a11y_button("Clear history")
                                    .cursor_pointer()
                                    .rounded_md()
                                    .px_1p5()
                                    .py_0p5()
                                    .text_size(px(theme::font_meta()))
                                    .text_color(theme::text_muted())
                                    .hover(|s| s.bg(theme::hover_bg()).text_color(theme::text()))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.history.clear();
                                        cx.notify();
                                    }))
                                    .child("Clear"),
                            ),
                    )
                    .child(history)
            })
    }
}

fn truncate_middle(text: &str, max: usize) -> String {
    let count = text.chars().count();
    if count <= max {
        return text.to_string();
    }
    let half = (max - 1) / 2;
    let start: String = text.chars().take(half).collect();
    let end: String = text.chars().skip(count - half).collect();
    format!("{start}…{end}")
}

// ---- the operations ------------------------------------------------------------

fn to_snake_case(input: &str) -> String {
    let mut out = String::with_capacity(input.len() + 4);
    for (index, ch) in input.chars().enumerate() {
        if ch.is_uppercase() {
            if index > 0 {
                out.push('_');
            }
            out.extend(ch.to_lowercase());
        } else if ch == ' ' || ch == '-' {
            out.push('_');
        } else {
            out.push(ch);
        }
    }
    out
}

fn to_pascal_case(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut capitalize = true;
    for ch in input.chars() {
        if ch == ' ' || ch == '_' || ch == '-' {
            capitalize = true;
        } else if capitalize {
            out.extend(ch.to_uppercase());
            capitalize = false;
        } else {
            out.extend(ch.to_lowercase());
        }
    }
    out
}

fn b64_encode(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(TABLE[(n >> 18) as usize & 63] as char);
        out.push(TABLE[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            TABLE[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

fn b64_decode(input: &str) -> Result<Vec<u8>, String> {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let value_of =
        |c: u8| -> Option<u32> { TABLE.iter().position(|&t| t == c).map(|ix| ix as u32) };
    let mut out = Vec::new();
    let mut buffer: u32 = 0;
    let mut bits = 0u32;
    for &c in input.as_bytes() {
        if c == b'=' || c == b'\n' || c == b'\r' || c == b' ' {
            continue;
        }
        let Some(value) = value_of(c) else {
            return Err(format!("'{}' is not a base64 character", c as char));
        };
        buffer = (buffer << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn casing_conversions() {
        assert_eq!(to_snake_case("DeviceIdentifier"), "device_identifier");
        assert_eq!(to_snake_case("already snake"), "already_snake");
        assert_eq!(to_pascal_case("device_identifier"), "DeviceIdentifier");
        assert_eq!(to_pascal_case("hello world"), "HelloWorld");
    }

    #[test]
    fn base64_round_trip() {
        let encoded = b64_encode(b"hello warp-station");
        assert_eq!(encoded, "aGVsbG8gd2FycC1zdGF0aW9u");
        assert_eq!(b64_decode(&encoded).unwrap(), b"hello warp-station");
        assert!(b64_decode("not@base64!").is_err());
    }

    #[test]
    fn formatter_modes_run() {
        assert_eq!(FormatterMode::SnakeCase.run("FooBar").unwrap(), "foo_bar");
        assert_eq!(FormatterMode::PascalCase.run("foo_bar").unwrap(), "FooBar");
        assert_eq!(FormatterMode::Base64Encode.run("a").unwrap(), "YQ==");
        assert_eq!(FormatterMode::Base64Decode.run("YQ==").unwrap(), "a");
    }
}
