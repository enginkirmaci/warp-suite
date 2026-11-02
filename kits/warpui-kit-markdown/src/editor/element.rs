//! The live editor's element: one text surface painted line by line, block
//! washes first, then selection, decorations and text, the caret last. There
//! are no child elements per block: a line's widgets are painted from its
//! view, so a caret entering a callout swaps it for source in the same frame.
//!
//! Layout and design language follow Obsidian's default theme: a centered
//! readable column, quiet faint syntax, accent bars and checkboxes; every
//! color comes from [`LiveColors`], so the app's preset owns the look.

use super::display::{Band, Decor};
use super::frame::{Frame, LaidBox, LaidLine, Rect, TableStrip};
use super::icons::{self, Icon};
use super::layout::{Piece, link_icon_width, remove_gap};
use super::state::{self, KEY_CONTEXT, LiveEditorState, PasteHandler};
use super::theme::LiveColors;
use gpui::prelude::*;
use gpui::{
    App, Bounds, ClipboardItem, ContentMask, Corners, CursorStyle, Element, ElementId, Entity,
    GlobalElementId, Hitbox, HitboxBehavior, Hsla, InspectorElementId, IntoElement, LayoutId,
    MouseButton, Pixels, Point, RenderOnce, SharedString, Style, TextAlign, TextRun,
    TransformationMatrix, Window, div, fill, font, point, px, quad, relative, size,
};
use gpui_kit::base::input::{Copy, Cut, Paste, SelectAll};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::input::Input;
use gpui_kit::component::menu::ContextMenuExt as _;
use std::ops::Range;
use std::panic::Location;
use std::rc::Rc;
use warpui_kit_core::A11y as _;

/// Quote bars and rules, in logical px.
const BAR_WIDTH: f32 = 2.;
const RULE_HEIGHT: f32 = 2.;
const CARET_WIDTH: f32 = 1.5;
const CORNER_RADIUS: f32 = 4.;
/// A bullet's diameter, in em.
const BULLET_SIZE: f32 = 0.3;
/// A code block's language label, in em.
const LABEL_SIZE: f32 = 0.75;
const LABEL_INSET_RIGHT: f32 = 0.875;
const LABEL_INSET_TOP: f32 = 0.6;
const IMAGE_RADIUS: f32 = 4.;
/// A task box's border, in logical px.
const CHECKBOX_BORDER: f32 = 1.;
/// A checkmark inside its box, as a share of the box.
const CHECK_SCALE: f32 = 0.75;
/// A callout title icon, in em.
const ICON_SIZE: f32 = 1.;
/// A table strip's plus and a diagram's edit icon, in em.
const STRIP_ICON_SIZE: f32 = 0.75;
const BUTTON_ICON_SIZE: f32 = 1.;
/// The widest gap between strikes of emboldened text, in logical px: below
/// the quarter-pixel glyph positions, so the strokes merge.
const STRIKE_STEP: f32 = 0.35;

/// The Obsidian-style live markdown editor; give it an
/// `Entity<`[`LiveEditorState`]`>`.
#[derive(IntoElement)]
pub struct LiveEditor {
    state: Entity<LiveEditorState>,
    aria_label: Option<SharedString>,
    text_size: Option<Pixels>,
    surface: Option<Hsla>,
    paste_handler: Option<PasteHandler>,
}

impl LiveEditor {
    pub fn new(state: &Entity<LiveEditorState>) -> Self {
        Self {
            state: state.clone(),
            aria_label: None,
            text_size: None,
            surface: None,
            paste_handler: None,
        }
    }

    pub fn aria_label(mut self, label: impl Into<SharedString>) -> Self {
        self.aria_label = Some(label.into());
        self
    }

    /// The base font size; headings and everything else scale from it.
    pub fn text_size(mut self, size: Pixels) -> Self {
        self.text_size = Some(size);
        self
    }

    /// The color behind the editor, when it is not the theme's background:
    /// a diagram's labels mask its lines with this.
    pub fn surface(mut self, color: impl Into<Hsla>) -> Self {
        self.surface = Some(color.into());
        self
    }

    /// Intercept paste payloads before the default text insertion: `true`
    /// consumes the paste, `false` falls through to the clipboard's text.
    pub fn on_paste(
        mut self,
        handler: impl Fn(&ClipboardItem, &mut Window, &mut App) -> bool + 'static,
    ) -> Self {
        self.paste_handler = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for LiveEditor {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = self.state;
        let handler = self.paste_handler;
        let text_size = self.text_size;
        let surface = self.surface;
        state.update(cx, |editor, cx| {
            editor.set_paste_handler(handler);
            editor.set_surface(surface, cx);
            if let Some(text_size) = text_size {
                editor.set_text_size(text_size, cx);
            }
        });
        let editor = state.read(cx);
        let focus = editor.focus_handle().clone();
        let scroll = editor.scroll_handle();
        let label = self.aria_label.unwrap_or_else(|| "Note".into());
        let menu_focus = focus.clone();
        let menu_state = state.clone();
        let hover_state = state.clone();
        let find_bar = editor.find_bar().open.then(|| find_bar(&state, cx));
        let surface = div()
            .id(ElementId::View(state.entity_id()))
            .key_context(KEY_CONTEXT)
            .track_focus(&focus)
            .role(gpui::Role::MultilineTextInput)
            .aria_label(label)
            .relative()
            .size_full()
            .overflow_hidden()
            .cursor(CursorStyle::IBeam)
            .on_action(window.listener_for(&state, LiveEditorState::move_left))
            .on_action(window.listener_for(&state, LiveEditorState::move_right))
            .on_action(window.listener_for(&state, LiveEditorState::move_up))
            .on_action(window.listener_for(&state, LiveEditorState::move_down))
            .on_action(window.listener_for(&state, LiveEditorState::select_left))
            .on_action(window.listener_for(&state, LiveEditorState::select_right))
            .on_action(window.listener_for(&state, LiveEditorState::select_up))
            .on_action(window.listener_for(&state, LiveEditorState::select_down))
            .on_action(window.listener_for(&state, LiveEditorState::move_page_up))
            .on_action(window.listener_for(&state, LiveEditorState::move_page_down))
            .on_action(window.listener_for(&state, LiveEditorState::move_home))
            .on_action(window.listener_for(&state, LiveEditorState::move_end))
            .on_action(window.listener_for(&state, LiveEditorState::move_to_start_of_line))
            .on_action(window.listener_for(&state, LiveEditorState::move_to_end_of_line))
            .on_action(window.listener_for(&state, LiveEditorState::select_to_start_of_line))
            .on_action(window.listener_for(&state, LiveEditorState::select_to_end_of_line))
            .on_action(window.listener_for(&state, LiveEditorState::move_to_start))
            .on_action(window.listener_for(&state, LiveEditorState::move_to_end))
            .on_action(window.listener_for(&state, LiveEditorState::select_to_start))
            .on_action(window.listener_for(&state, LiveEditorState::select_to_end))
            .on_action(window.listener_for(&state, LiveEditorState::move_to_previous_word))
            .on_action(window.listener_for(&state, LiveEditorState::move_to_next_word))
            .on_action(window.listener_for(&state, LiveEditorState::select_to_previous_word))
            .on_action(window.listener_for(&state, LiveEditorState::select_to_next_word))
            .on_action(window.listener_for(&state, LiveEditorState::select_all))
            .on_action(window.listener_for(&state, LiveEditorState::escape))
            .on_action(window.listener_for(&state, LiveEditorState::backspace))
            .on_action(window.listener_for(&state, LiveEditorState::delete))
            .on_action(window.listener_for(&state, LiveEditorState::delete_word_left))
            .on_action(window.listener_for(&state, LiveEditorState::delete_word_right))
            .on_action(window.listener_for(&state, LiveEditorState::delete_to_line_start))
            .on_action(window.listener_for(&state, LiveEditorState::delete_to_line_end))
            .on_action(window.listener_for(&state, LiveEditorState::enter))
            .on_action(window.listener_for(&state, LiveEditorState::indent_inline))
            .on_action(window.listener_for(&state, LiveEditorState::outdent_inline))
            .on_action(window.listener_for(&state, LiveEditorState::indent_lines))
            .on_action(window.listener_for(&state, LiveEditorState::outdent_lines))
            .on_action(window.listener_for(&state, LiveEditorState::toggle_bold))
            .on_action(window.listener_for(&state, LiveEditorState::toggle_italic))
            .on_action(window.listener_for(&state, LiveEditorState::insert_link))
            .on_action(window.listener_for(&state, LiveEditorState::toggle_task))
            .on_action(window.listener_for(&state, LiveEditorState::delete_line))
            .on_action(window.listener_for(&state, LiveEditorState::remove_property))
            .on_action(window.listener_for(&state, LiveEditorState::undo))
            .on_action(window.listener_for(&state, LiveEditorState::redo))
            .on_action(window.listener_for(&state, LiveEditorState::copy))
            .on_action(window.listener_for(&state, LiveEditorState::cut))
            .on_action(window.listener_for(&state, LiveEditorState::paste))
            .on_action(window.listener_for(&state, LiveEditorState::search))
            .on_action(window.listener_for(&state, LiveEditorState::replace))
            .on_mouse_down(
                MouseButton::Left,
                window.listener_for(&state, LiveEditorState::mouse_down),
            )
            .on_mouse_up(
                MouseButton::Left,
                window.listener_for(&state, LiveEditorState::mouse_up),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                window.listener_for(&state, LiveEditorState::mouse_up),
            )
            .on_mouse_down(
                MouseButton::Right,
                window.listener_for(&state, LiveEditorState::right_mouse_down),
            )
            .on_mouse_move(window.listener_for(&state, LiveEditorState::mouse_move))
            .on_scroll_wheel(window.listener_for(&state, LiveEditorState::scroll_wheel))
            .on_hover(move |hovered, _, cx| {
                if !hovered {
                    hover_state.update(cx, LiveEditorState::clear_hover);
                }
            })
            .child(LiveSurface {
                state: state.clone(),
            })
            .child(state::scrollbar(&scroll))
            .context_menu(move |menu, _, cx| {
                let menu = menu
                    .action_context(menu_focus.clone())
                    .menu("Cut", Box::new(Cut))
                    .menu("Copy", Box::new(Copy))
                    .menu("Paste", Box::new(Paste))
                    .separator()
                    .menu("Select all", Box::new(SelectAll));
                if menu_state.read(cx).menu_on_property() {
                    menu.separator()
                        .menu("Remove property", Box::new(state::RemoveProperty))
                } else {
                    menu
                }
            });
        // the bar sits beside the text, not inside it: its fields' Enter must
        // never bubble into the note
        div()
            .relative()
            .size_full()
            .child(surface)
            .children(find_bar)
    }
}

/// One glyph button of the find bar.
fn bar_button(
    id: &'static str,
    label: &'static str,
    name: &'static str,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    div()
        .id(id)
        .a11y_button(name)
        .cursor_pointer()
        .rounded_md()
        .h(px(24.))
        .min_w(px(24.))
        .px_1()
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(12.))
        .text_color(theme.muted_foreground)
        .hover(|style| style.bg(warpui_kit_core::palette::HOVER))
        .on_click(move |_, window, cx| on_click(window, cx))
        .child(label)
}

/// The find bar: floats over the top of the text, like Obsidian's.
fn find_bar(state: &Entity<LiveEditorState>, cx: &App) -> impl IntoElement {
    let editor = state.read(cx);
    let find = editor.find_bar();
    let count = if find.matches.is_empty() {
        "No results".to_string()
    } else {
        match find.current {
            Some(current) => format!("{} / {}", current + 1, find.matches.len()),
            None => format!("{} found", find.matches.len()),
        }
    };
    let on = |f: fn(&mut LiveEditorState, &mut Window, &mut gpui::Context<LiveEditorState>)| {
        let state = state.clone();
        move |window: &mut Window, cx: &mut App| {
            state.update(cx, |state, cx| f(state, window, cx));
        }
    };
    let query = Input::new(&find.query).small();
    let replacement = find
        .replacing
        .then(|| Input::new(&find.replacement).small());
    let replacing = find.replacing;
    let theme = cx.theme();
    let dismiss = state.clone();
    div()
        .id("live-editor-find")
        .absolute()
        .top(px(8.))
        .right(px(16.))
        .w(px(380.))
        .flex()
        .flex_col()
        .gap_1()
        .p_1p5()
        .rounded_lg()
        .border_1()
        .border_color(theme.border)
        .bg(theme.popover)
        .text_color(theme.popover_foreground)
        .on_key_down(move |event: &gpui::KeyDownEvent, window, cx| {
            if event.keystroke.key == "escape" {
                dismiss.update(cx, |state, cx| state.close_find(window, cx));
                cx.stop_propagation();
            }
        })
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_1()
                .child(div().flex_1().min_w_0().child(query))
                .child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(11.))
                        .text_color(theme.muted_foreground)
                        .child(count),
                )
                .child(bar_button(
                    "live-find-previous",
                    "↑",
                    "Previous match",
                    on(|state, window, cx| state.find_step(false, window, cx)),
                    cx,
                ))
                .child(bar_button(
                    "live-find-next",
                    "↓",
                    "Next match",
                    on(|state, window, cx| state.find_step(true, window, cx)),
                    cx,
                ))
                .child(bar_button(
                    "live-find-toggle-replace",
                    "⇄",
                    "Toggle replace",
                    on(|state, _, cx| state.toggle_replace(cx)),
                    cx,
                ))
                .child(bar_button(
                    "live-find-close",
                    "×",
                    "Close find",
                    on(|state, window, cx| state.close_find(window, cx)),
                    cx,
                )),
        )
        .when(replacing, |bar| {
            bar.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .child(div().flex_1().min_w_0().children(replacement))
                    .child(bar_button(
                        "live-find-replace",
                        "Replace",
                        "Replace match",
                        on(|state, window, cx| state.replace_current(window, cx)),
                        cx,
                    ))
                    .child(bar_button(
                        "live-find-replace-all",
                        "All",
                        "Replace all matches",
                        on(|state, _, cx| state.replace_all_matches(cx)),
                        cx,
                    )),
            )
        })
}

/// The painted text surface.
struct LiveSurface {
    state: Entity<LiveEditorState>,
}

impl IntoElement for LiveSurface {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

/// What paint needs from prepaint.
struct Prepared {
    frame: Rc<Frame>,
    hitbox: Hitbox,
    colors: LiveColors,
}

impl Element for LiveSurface {
    type RequestLayoutState = ();
    type PrepaintState = Prepared;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let style = Style {
            size: size(relative(1.).into(), relative(1.).into()),
            ..Style::default()
        };
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Prepared {
        let frame = self
            .state
            .update(cx, |state, cx| state.layout(bounds, window, cx));
        Prepared {
            frame,
            hitbox: window.insert_hitbox(bounds, HitboxBehavior::Normal),
            colors: LiveColors::from_theme(cx),
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        prepared: &mut Prepared,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus = self.state.read(cx).focus_handle().clone();
        window.handle_input(&focus, state::input_handler(bounds, self.state.clone()), cx);
        if let Some(pointer) = self.state.read(cx).pointer() {
            window.set_cursor_style(pointer, &prepared.hitbox);
        }
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            let painter = Painter {
                frame: prepared.frame.clone(),
                colors: prepared.colors,
                state: self.state.clone(),
                bounds,
            };
            painter.paint(window, cx);
        });
    }
}

struct Painter {
    frame: Rc<Frame>,
    colors: LiveColors,
    state: Entity<LiveEditorState>,
    bounds: Bounds<Pixels>,
}

impl Painter {
    fn paint(&self, window: &mut Window, cx: &mut App) {
        let editor = self.state.read(cx);
        let doc = editor.document();
        let em = editor.em();
        let selection = editor.selection();
        let marked = editor.marked_range();
        let find_matches = editor.find_bar().matches.clone();
        let caret_visible = editor.caret_visible(window);
        let placeholder = editor.placeholder_text();
        let empty = editor.value().is_empty();
        let scroll = editor.scroll_handle().top();
        let hover = editor.hover();
        let viewport = self.bounds.size.height.as_f32();
        let visible = scroll..scroll + viewport;
        let first = self.frame.first_visible(visible.start);
        let lines: Vec<&LaidLine> = self.frame.lines[first..]
            .iter()
            .take_while(|line| line.top <= visible.end)
            .collect();
        let origin = point(
            self.bounds.origin.x + px(self.frame.column_x),
            self.bounds.origin.y - px(scroll),
        );

        for line in &lines {
            self.paint_band(line, origin, window);
        }
        if let (Some(first), Some(last)) = (lines.first(), lines.last()) {
            let shown = first.source.start..last.source.end;
            for found in &find_matches {
                if found.end < shown.start || found.start > shown.end {
                    continue;
                }
                for rect in self.frame.selection_rects(&doc, found, visible.clone()) {
                    let at = origin + point(px(rect.x), px(rect.y));
                    window.paint_quad(fill(
                        Bounds::new(at, size(px(rect.width), px(rect.height))),
                        self.colors.highlight,
                    ));
                }
            }
        }
        let selection_range = selection.range();
        for rect in self
            .frame
            .selection_rects(&doc, &selection_range, visible.clone())
        {
            let at = origin + point(px(rect.x), px(rect.y));
            window.paint_quad(fill(
                Bounds::new(at, size(px(rect.width), px(rect.height))),
                self.colors.selection,
            ));
        }
        for line in &lines {
            self.paint_decor(line, origin, em, window, cx);
            for laid in &line.boxes {
                self.paint_box(line, laid, origin, window, cx);
            }
        }
        if let Some((block, strip)) = hover.table {
            self.paint_table_strips(block, strip, origin, em, window, cx);
        }
        if let Some((block, on_button)) = hover.diagram {
            self.paint_diagram_hover(block, on_button, origin, em, window, cx);
        }
        if let Some(range) = &marked {
            self.paint_marked(&doc, range, origin, window);
        }
        if empty && !placeholder.is_empty() {
            self.paint_placeholder(placeholder, origin, em, window, cx);
        }
        if caret_visible
            && selection.is_caret()
            && let Some(spot) = self.frame.caret(&doc, selection.head)
        {
            let at = origin + point(px(spot.x), px(spot.y));
            window.paint_quad(fill(
                Bounds::new(at, size(px(CARET_WIDTH), px(spot.height))),
                self.colors.caret,
            ));
        }
    }

    /// A code or callout wash, the full column wide.
    fn paint_band(&self, line: &LaidLine, origin: Point<Pixels>, window: &mut Window) {
        for decor in &line.view.decor {
            let Decor::Band { band, first, last } = decor else {
                continue;
            };
            let color = match band {
                Band::Code => self.colors.code_well,
                Band::Callout(tone) => self.colors.tone_wash(*tone),
            };
            let radius = |on: bool| if on { px(CORNER_RADIUS) } else { px(0.) };
            let corners = Corners {
                top_left: radius(*first),
                top_right: radius(*first),
                bottom_right: radius(*last),
                bottom_left: radius(*last),
            };
            let at = origin + point(px(0.), px(line.top));
            let extent = size(px(self.frame.column_width), px(line.height));
            window.paint_quad(quad(
                Bounds::new(at, extent),
                corners,
                color,
                px(0.),
                gpui::transparent_black(),
                gpui::BorderStyle::default(),
            ));
        }
    }

    fn paint_icon(
        &self,
        icon: Icon,
        bounds: Bounds<Pixels>,
        color: Hsla,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Err(err) = window.paint_svg(
            bounds,
            SharedString::new_static(icon.key),
            Some(icon.svg.as_bytes()),
            TransformationMatrix::unit(),
            color,
            cx,
        ) {
            paint_failed(&err);
        }
    }

    fn paint_decor(
        &self,
        line: &LaidLine,
        origin: Point<Pixels>,
        em: f32,
        window: &mut Window,
        cx: &mut App,
    ) {
        let top = line.top;
        let text_top = top + line.view.height.top.0;
        let row = line.view.height.line.0;
        for decor in &line.view.decor {
            match decor {
                Decor::Bullet { center } => {
                    let diameter = em * BULLET_SIZE;
                    let at = origin
                        + point(
                            px(center.0 - diameter / 2.),
                            px(text_top + (row - diameter) / 2.),
                        );
                    window.paint_quad(quad(
                        Bounds::new(at, size(px(diameter), px(diameter))),
                        px(diameter / 2.),
                        self.colors.faint,
                        px(0.),
                        gpui::transparent_black(),
                        gpui::BorderStyle::default(),
                    ));
                }
                Decor::Checkbox { x, checked, .. } => {
                    let side = em;
                    let at = origin + point(px(x.0), px(text_top + (row - side) / 2.));
                    let bounds = Bounds::new(at, size(px(side), px(side)));
                    let (background, border) = if *checked {
                        (self.colors.accent, self.colors.accent)
                    } else {
                        (gpui::transparent_black(), self.colors.faint)
                    };
                    window.paint_quad(quad(
                        bounds,
                        px(CORNER_RADIUS),
                        background,
                        px(CHECKBOX_BORDER),
                        border,
                        gpui::BorderStyle::default(),
                    ));
                    if *checked {
                        let inner = side * CHECK_SCALE;
                        let inset = (side - inner) / 2.;
                        let at = at + point(px(inset), px(inset));
                        self.paint_icon(
                            icons::CHECKMARK,
                            Bounds::new(at, size(px(inner), px(inner))),
                            self.colors.on_accent,
                            window,
                            cx,
                        );
                    }
                }
                Decor::QuoteBar { offset } => {
                    let Some(laid) = line.boxes.first() else {
                        continue;
                    };
                    let x = laid.shaped.rows.first().map_or(0., |r| r.x_at(*offset));
                    let at = origin + point(px(laid.x + x), px(top));
                    window.paint_quad(fill(
                        Bounds::new(at, size(px(BAR_WIDTH), px(line.height))),
                        self.colors.accent,
                    ));
                }
                Decor::Guide { x } => {
                    let at = origin + point(px(x.0), px(top));
                    window.paint_quad(fill(
                        Bounds::new(at, size(px(1.), px(line.height))),
                        self.colors.border,
                    ));
                }
                Decor::Rule => {
                    let at = origin + point(px(0.), px(top + (line.height - RULE_HEIGHT) / 2.));
                    window.paint_quad(fill(
                        Bounds::new(at, size(px(self.frame.column_width), px(RULE_HEIGHT))),
                        self.colors.border,
                    ));
                }
                Decor::Label(label) => {
                    let label_size = em * LABEL_SIZE;
                    let run = plain_run(label.len(), self.colors.muted, cx);
                    let shaped = window.text_system().shape_line(
                        SharedString::from(label.clone()),
                        px(label_size),
                        &[run],
                        None,
                    );
                    let at = origin
                        + point(
                            px(self.frame.column_width
                                - em * LABEL_INSET_RIGHT
                                - shaped.width().as_f32()),
                            px(top + em * LABEL_INSET_TOP),
                        );
                    if let Err(err) =
                        shaped.paint(at, px(label_size * 1.5), TextAlign::Left, None, window, cx)
                    {
                        paint_failed(&err);
                    }
                }
                Decor::Icon { glyph, tone, x } => {
                    let side = em * ICON_SIZE;
                    let at = origin + point(px(x.0), px(text_top + (row - side) / 2.));
                    self.paint_icon(
                        icons::glyph(*glyph),
                        Bounds::new(at, size(px(side), px(side))),
                        self.colors.tone(*tone),
                        window,
                        cx,
                    );
                }
                Decor::Band { .. } => {}
            }
        }
        for cell in &line.cells {
            let at = origin + point(px(cell.x), px(top + line.view.height.top.0));
            let body = line.height - line.view.height.top.0 - line.view.height.bottom.0;
            window.paint_quad(gpui::outline(
                // one px past the cell, so neighbors share their border
                Bounds::new(at, size(px(cell.width + 2.), px(body + 1.))),
                self.colors.border,
                gpui::BorderStyle::default(),
            ));
        }
        if let Some(image) = &line.image {
            let at = origin + point(px(image.x), px(top + image.y));
            let bounds = Bounds::new(at, size(px(image.width), px(image.height)));
            self.paint_image(&image.key, bounds, window, cx);
        }
    }

    fn paint_image(&self, key: &str, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
        let Some(data) = self.state.read(cx).image_data(key) else {
            return;
        };
        let corners = Corners::all(px(IMAGE_RADIUS));
        if let Err(err) = window.paint_image(bounds, bounds, corners, data, 0, false) {
            paint_failed(&err);
        }
    }

    fn bounds_of(rect: Rect, origin: Point<Pixels>) -> Bounds<Pixels> {
        Bounds::new(
            origin + point(px(rect.x), px(rect.y)),
            size(px(rect.width), px(rect.height)),
        )
    }

    /// `icon`, `side` square, in the middle of `bounds`.
    fn paint_centered_icon(
        &self,
        icon: Icon,
        bounds: Bounds<Pixels>,
        side: f32,
        color: Hsla,
        window: &mut Window,
        cx: &mut App,
    ) {
        let inset = point(
            (bounds.size.width - px(side)) / 2.,
            (bounds.size.height - px(side)) / 2.,
        );
        let at = Bounds::new(bounds.origin + inset, size(px(side), px(side)));
        self.paint_icon(icon, at, color, window, cx);
    }

    /// A hovered table's add strips, the one under the pointer lit.
    fn paint_table_strips(
        &self,
        block: usize,
        hot: Option<TableStrip>,
        origin: Point<Pixels>,
        em: f32,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(table) = self.frame.tables.iter().find(|table| table.block == block) else {
            return;
        };
        for strip in [TableStrip::Row, TableStrip::Column] {
            let bounds = Self::bounds_of(self.frame.table_strip(table, strip), origin);
            let lit = hot == Some(strip);
            let background = if lit {
                warpui_kit_core::palette::HOVER
            } else {
                gpui::transparent_black()
            };
            window.paint_quad(quad(
                bounds,
                px(CORNER_RADIUS),
                background,
                px(1.),
                self.colors.border,
                gpui::BorderStyle::default(),
            ));
            let ink = if lit {
                self.colors.muted
            } else {
                self.colors.faint
            };
            self.paint_centered_icon(icons::ADD, bounds, em * STRIP_ICON_SIZE, ink, window, cx);
        }
    }

    /// A hovered diagram's outline and edit button.
    fn paint_diagram_hover(
        &self,
        block: usize,
        on_button: bool,
        origin: Point<Pixels>,
        em: f32,
        window: &mut Window,
        cx: &mut App,
    ) {
        let found = self
            .frame
            .lines
            .iter()
            .find(|line| line.view.diagram == Some(block));
        let Some(line) = found else {
            return;
        };
        let at = origin + point(px(0.), px(line.top));
        let extent = size(px(self.frame.column_width), px(line.height));
        window.paint_quad(quad(
            Bounds::new(at, extent),
            px(CORNER_RADIUS),
            gpui::transparent_black(),
            px(1.),
            self.colors.border,
            gpui::BorderStyle::default(),
        ));
        let button = Self::bounds_of(self.frame.diagram_button(line), origin);
        if on_button {
            window.paint_quad(quad(
                button,
                px(CORNER_RADIUS),
                warpui_kit_core::palette::HOVER,
                px(0.),
                gpui::transparent_black(),
                gpui::BorderStyle::default(),
            ));
        }
        self.paint_centered_icon(
            icons::EDIT_SOURCE,
            button,
            em * BUTTON_ICON_SIZE,
            self.colors.text,
            window,
            cx,
        );
    }

    fn paint_box(
        &self,
        line: &LaidLine,
        laid: &LaidBox,
        origin: Point<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let shaped = &laid.shaped;
        let height = shaped.line_height;
        for row in &shaped.rows {
            let baseline = line.top + laid.y + shaped.baseline(row);
            for piece in &row.pieces {
                let x = laid.x + row.x + piece.x;
                if let Some(image) = &piece.image {
                    let bottom = line.top + laid.y + row.top + row.height;
                    let at = origin + point(px(x), px(bottom - image.height));
                    let extent = size(px(piece.width), px(image.height));
                    self.paint_image(&image.key, Bounds::new(at, extent), window, cx);
                    continue;
                }
                let piece_padding =
                    (height - piece.line.ascent.as_f32() - piece.line.descent.as_f32()) / 2.;
                let piece_top = baseline - piece.raise - piece_padding - piece.line.ascent.as_f32();
                if piece.pill.is_some() {
                    self.paint_pill(piece, origin, x, baseline - piece.raise, window);
                }
                let at = origin + point(px(x), px(piece_top));
                if let Err(err) =
                    piece
                        .line
                        .paint_background(at, px(height), TextAlign::Left, None, window, cx)
                {
                    paint_failed(&err);
                }
                // no real bold in this family: strike the glyphs a few times
                // across the added stem width
                let strikes = (piece.embolden / STRIKE_STEP).ceil() as usize;
                for strike in 0..=strikes {
                    let shift = if strikes == 0 {
                        0.
                    } else {
                        piece.embolden * (strike as f32 / strikes as f32 - 0.5)
                    };
                    let at = at + point(px(shift), px(0.));
                    if let Err(err) =
                        piece
                            .line
                            .paint(at, px(height), TextAlign::Left, None, window, cx)
                    {
                        paint_failed(&err);
                    }
                }
                if piece.remove {
                    let side = piece.size;
                    let ascent = piece.line.ascent.as_f32();
                    let middle = baseline - ascent + (ascent + piece.line.descent.as_f32()) / 2.;
                    let icon_at = origin
                        + point(
                            px(x + piece.width + remove_gap(side)),
                            px(middle - side / 2.),
                        );
                    self.paint_icon(
                        icons::CROSS,
                        Bounds::new(icon_at, size(px(side), px(side))),
                        self.colors.faint,
                        window,
                        cx,
                    );
                }
                if piece.link_icon {
                    let side = link_icon_width(piece.size) * 0.85;
                    let icon_at = origin
                        + point(
                            px(x + piece.width + (link_icon_width(piece.size) - side) / 2.),
                            px(baseline - piece.line.ascent.as_f32() * 0.8),
                        );
                    self.paint_icon(
                        icons::LINK_OUT,
                        Bounds::new(icon_at, size(px(side), px(side))),
                        self.colors.faint,
                        window,
                        cx,
                    );
                }
            }
        }
    }

    fn paint_pill(
        &self,
        piece: &Piece,
        origin: Point<Pixels>,
        x: f32,
        baseline: f32,
        window: &mut Window,
    ) {
        let Some(pill) = piece.pill else {
            return;
        };
        let (_, pad_y) = super::display::pill_padding(pill);
        let pad_y = pad_y * piece.size;
        let ascent = piece.line.ascent.as_f32();
        let descent = piece.line.descent.as_f32();
        let left = x - if piece.opens { piece.pad_left } else { 0. };
        let right = x + piece.width + if piece.closes { piece.pad_right } else { 0. };
        let top = baseline - ascent - pad_y;
        let height = ascent + descent + pad_y * 2.;
        let (color, radius) = match pill {
            super::display::Pill::Code => (self.colors.code_pill, px(CORNER_RADIUS)),
            super::display::Pill::Tag | super::display::Pill::Item => {
                (self.colors.tag_wash, px(height / 2.))
            }
        };
        let edge = |on: bool| if on { radius } else { px(0.) };
        let corners = Corners {
            top_left: edge(piece.opens),
            top_right: edge(piece.closes),
            bottom_right: edge(piece.closes),
            bottom_left: edge(piece.opens),
        };
        let at = origin + point(px(left), px(top));
        window.paint_quad(quad(
            Bounds::new(at, size(px(right - left), px(height))),
            corners,
            color,
            px(0.),
            gpui::transparent_black(),
            gpui::BorderStyle::default(),
        ));
    }

    /// IME composition reads as underlined text.
    fn paint_marked(
        &self,
        doc: &crate::live::LiveDocument,
        range: &Range<usize>,
        origin: Point<Pixels>,
        window: &mut Window,
    ) {
        let rects = self.frame.selection_rects(doc, range, 0.0..f32::MAX);
        for rect in rects {
            let at = origin + point(px(rect.x), px(rect.y + rect.height - 1.));
            window.paint_quad(fill(
                Bounds::new(at, size(px(rect.width), px(1.))),
                self.colors.text,
            ));
        }
    }

    fn paint_placeholder(
        &self,
        text: SharedString,
        origin: Point<Pixels>,
        em: f32,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(line) = self.frame.lines.first() else {
            return;
        };
        let run = plain_run(text.len(), self.colors.faint, cx);
        let shaped = window.text_system().shape_line(text, px(em), &[run], None);
        let at = origin + point(px(0.), px(line.top));
        if let Err(err) = shaped.paint(at, px(line.height), TextAlign::Left, None, window, cx) {
            paint_failed(&err);
        }
    }
}

fn plain_run(len: usize, color: Hsla, cx: &App) -> TextRun {
    TextRun {
        len,
        font: font(cx.theme().font_family.clone()),
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    }
}

/// A paint call that failed cost one glyph or icon this frame; log and go on.
fn paint_failed(err: &anyhow::Error) {
    log::warn!("live editor paint: {err}");
}
