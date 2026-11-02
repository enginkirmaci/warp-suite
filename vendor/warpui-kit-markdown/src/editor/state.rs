//! The live editor's entity: the text, selection and undo history, every
//! editing command, and the input handler the platform talks to. Key bindings
//! are the kit's own (`Input` context), so movement and clipboard keys match
//! every other field on each platform; the markdown shortcuts sit on top.

use super::code::CodeTokens;
use super::diagram::{Diagrams, Look};
use super::display::{LineView, Reveal, ViewContext, build_view};
use super::frame::{self, Frame, Hit, HitKind, Hover, ShapeCache, TableStrip};
use super::images::ImageStore;
use super::layout::Fonts;
use super::ops::{self, Change, Selection};
use super::theme::LiveColors;
use crate::live::{
    InlineObjectKind, LiveBlock, LiveBlockKind, LiveDocument, LiveLineKind, Property,
    parse_document,
};
use gpui::prelude::*;
use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, Entity, EntityInputHandler, EventEmitter,
    FocusHandle, Focusable, Hsla, KeyBinding, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, Point, ScrollWheelEvent, SharedString, Subscription, UTF16Selection,
    Window, font, point, px, size,
};
use gpui_kit::base::actions::{SelectDown, SelectLeft, SelectRight, SelectUp};
use gpui_kit::base::input::{
    Backspace, Copy, Cut, Delete, DeleteToBeginningOfLine, DeleteToEndOfLine, DeleteToNextWordEnd,
    DeleteToPreviousWordStart, Enter, Escape, Indent, IndentInline, MoveDown, MoveEnd, MoveHome,
    MoveLeft, MovePageDown, MovePageUp, MoveRight, MoveToEnd, MoveToEndOfLine, MoveToNextWord,
    MoveToPreviousWord, MoveToStart, MoveToStartOfLine, MoveUp, Outdent, OutdentInline, Paste,
    Redo, Replace, Search, SelectAll, SelectToEnd, SelectToEndOfLine, SelectToNextWordEnd,
    SelectToPreviousWordStart, SelectToStart, SelectToStartOfLine, Undo,
};
use gpui_kit::base::{Scrollbar, ScrollbarHandle};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::input::{InputEvent, InputState};
use std::borrow::Cow;
use std::cell::Cell;
use std::collections::HashSet;
use std::ops::Range;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

gpui::actions!(
    live_editor,
    [
        /// Wraps the selection in `**`.
        ToggleBold,
        /// Wraps the selection in `*`.
        ToggleItalic,
        /// Turns the selection into a link.
        InsertLink,
        /// Flips the checkbox on the caret's line.
        ToggleTask,
        /// Removes the caret's line.
        DeleteLine,
        /// Takes the property under the context menu out of the note.
        RemoveProperty,
    ]
);

/// The key context of the editor's focused element: `Input` picks up the kit's
/// editing bindings, `LiveEditor` the markdown ones below.
pub(super) const KEY_CONTEXT: &str = "Input LiveEditor";

/// How long the caret stays on, then off.
const BLINK_INTERVAL: Duration = Duration::from_millis(500);
/// Keystrokes this close together undo as one step.
const UNDO_GROUP_WINDOW: Duration = Duration::from_millis(1000);
/// Lines kept between the caret and the viewport edge when scrolling to it.
const CARET_MARGIN_LINES: f32 = 1.5;
/// What one wheel notch of a line-based wheel moves, in lines.
const WHEEL_LINE_EM: f32 = 1.5;

/// What the editor tells its host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LiveEditorEvent {
    /// The text changed.
    Change,
    /// A rendered external link was clicked.
    OpenUrl(String),
    /// A rendered `[[wikilink]]` or relative link was clicked.
    OpenNote(String),
}

impl EventEmitter<LiveEditorEvent> for LiveEditorState {}

pub(super) type PasteHandler = Rc<dyn Fn(&ClipboardItem, &mut Window, &mut App) -> bool>;

/// One undoable edit: `removed` at `at` became `inserted`.
#[derive(Clone, Debug)]
struct Step {
    at: usize,
    removed: String,
    inserted: String,
    before: Selection,
    after: Selection,
    stamp: Instant,
    group: Group,
}

/// Which steps may merge into one undo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Group {
    Typing,
    Deleting,
    /// Never merges: structure changes, pastes, formatting.
    Single,
}

#[derive(Default)]
struct History {
    undo: Vec<Step>,
    redo: Vec<Step>,
}

impl History {
    fn record(&mut self, step: Step) {
        self.redo.clear();
        if let Some(last) = self.undo.last_mut()
            && step.group != Group::Single
            && step.group == last.group
            && step.stamp.duration_since(last.stamp) < UNDO_GROUP_WINDOW
        {
            match step.group {
                // typing runs on from where the last keystroke ended
                Group::Typing
                    if last.at + last.inserted.len() == step.at
                        && last.removed.is_empty()
                        && step.removed.is_empty()
                        && !step.inserted.contains('\n') =>
                {
                    last.inserted.push_str(&step.inserted);
                    last.after = step.after;
                    last.stamp = step.stamp;
                    return;
                }
                // backspacing eats backwards from where it stood
                Group::Deleting
                    if step.at + step.removed.len() == last.at
                        && last.inserted.is_empty()
                        && step.inserted.is_empty() =>
                {
                    last.removed.insert_str(0, &step.removed);
                    last.at = step.at;
                    last.after = step.after;
                    last.stamp = step.stamp;
                    return;
                }
                _ => {}
            }
        }
        self.undo.push(step);
    }
}

/// The scroll position, shared with the kit's scrollbar.
#[derive(Clone, Default)]
pub struct LiveScroll {
    inner: Rc<ScrollCells>,
}

#[derive(Default)]
struct ScrollCells {
    /// Pixels scrolled from the top, positive.
    top: Cell<f32>,
    max: Cell<f32>,
    bounds: Cell<Option<Bounds<Pixels>>>,
    content: Cell<f32>,
}

impl LiveScroll {
    pub(super) fn top(&self) -> f32 {
        self.inner.top.get()
    }

    fn set_top(&self, top: f32) {
        self.inner.top.set(top.clamp(0., self.inner.max.get()));
    }

    fn update_extent(&self, bounds: Bounds<Pixels>, content: f32, max: f32) {
        self.inner.bounds.set(Some(bounds));
        self.inner.content.set(content);
        self.inner.max.set(max);
        self.set_top(self.top());
    }
}

impl ScrollbarHandle for LiveScroll {
    fn viewport_bounds(&self) -> Bounds<Pixels> {
        self.inner.bounds.get().unwrap_or_default()
    }

    fn offset(&self) -> Point<Pixels> {
        point(px(0.), px(-self.top()))
    }

    fn set_offset(&self, offset: Point<Pixels>) {
        self.set_top(-offset.y.as_f32());
    }

    fn content_size(&self) -> gpui::Size<Pixels> {
        let bounds = self.viewport_bounds();
        size(bounds.size.width, px(self.inner.content.get()))
    }
}

/// The inputs a frame was laid out from; equal keys reuse the frame.
#[derive(Clone, Debug, PartialEq)]
struct FrameKey {
    revision: u64,
    selection: Option<Range<usize>>,
    size: gpui::Size<Pixels>,
    em: f32,
    colors: LiveColors,
}

/// What a press in the text is dragging out.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Press {
    /// Extending the selection.
    Selecting,
    /// Holding a link: released in place, it opens.
    Link {
        kind: InlineObjectKind,
        origin: Point<Pixels>,
    },
}

/// Where Tab goes from a property.
enum PropertyStep {
    Caret(usize),
    /// A new item in the list property on this line.
    AddItem(usize),
    AddProperty,
}

/// The find bar's state.
pub(super) struct FindBar {
    pub open: bool,
    pub replacing: bool,
    pub query: Entity<InputState>,
    pub replacement: Entity<InputState>,
    /// Every match of the query, ascending.
    pub matches: Vec<Range<usize>>,
    /// The match the selection sits on.
    pub current: Option<usize>,
}

/// The live markdown editor's state; render it with
/// [`LiveEditor`](super::LiveEditor).
pub struct LiveEditorState {
    focus_handle: FocusHandle,
    text: SharedString,
    doc: Rc<LiveDocument>,
    revision: u64,
    selection: Selection,
    /// IME composition range.
    marked: Option<Range<usize>>,
    history: History,
    /// Closers the editor typed itself, offsets into `text`.
    closed: Vec<usize>,
    /// The x a vertical move aims for, kept across lines of different shapes.
    goal_x: Option<f32>,
    scroll: LiveScroll,
    /// Set by anything that moved the caret: the next layout brings it on
    /// screen.
    reveal_caret: bool,
    frame: Option<Rc<Frame>>,
    frame_key: Option<FrameKey>,
    bounds: Bounds<Pixels>,
    views: Vec<Rc<LineView>>,
    /// The selection the views were last built for.
    view_selection: Option<Range<usize>>,
    views_revision: u64,
    shapes: ShapeCache,
    images: ImageStore,
    /// The revision the images were last refreshed for.
    images_revision: u64,
    diagrams: Diagrams,
    tokens: CodeTokens,
    tokens_revision: u64,
    press: Option<Press>,
    caret_on: bool,
    blink_epoch: usize,
    /// The pointer's shape where the text's I-beam does not apply.
    pointer: Option<CursorStyle>,
    hover: Hover,
    /// The line of a properties row added or emptied here: blank when the
    /// caret leaves it, it goes.
    draft: Option<usize>,
    /// The property row the context menu was opened on.
    menu_property: Option<usize>,
    em: f32,
    placeholder: SharedString,
    base_dir: Option<PathBuf>,
    paste_handler: Option<PasteHandler>,
    /// The notes wikilinks resolve to; `None` treats every link as resolved.
    notes: Option<HashSet<String>>,
    /// What the host paints behind the text, when not the theme's background.
    surface: Option<Hsla>,
    find: FindBar,
    _subscriptions: Vec<Subscription>,
}

impl LiveEditorState {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        ensure_keys_bound(cx);
        let focus_handle = cx.focus_handle();
        let query = cx.new(|cx| InputState::new(window, cx).placeholder("Find"));
        let replacement = cx.new(|cx| InputState::new(window, cx).placeholder("Replace with"));
        let subscriptions =
            vec![
                cx.subscribe_in(&query, window, |this, _, event: &InputEvent, window, cx| {
                    match event {
                        InputEvent::Change => this.find_query_changed(cx),
                        InputEvent::PressEnter { shift, .. } => this.find_step(!*shift, window, cx),
                        _ => {}
                    }
                }),
                cx.subscribe_in(
                    &replacement,
                    window,
                    |this, _, event: &InputEvent, window, cx| {
                        if matches!(event, InputEvent::PressEnter { .. }) {
                            this.replace_current(window, cx);
                        }
                    },
                ),
                cx.on_focus(&focus_handle, window, |this, _, cx| {
                    this.restart_blink(cx);
                    cx.notify();
                }),
                cx.on_blur(&focus_handle, window, |this, _, cx| {
                    this.blink_epoch += 1;
                    this.press = None;
                    this.settle_draft(None, cx);
                    cx.notify();
                }),
            ];
        Self {
            focus_handle,
            text: SharedString::default(),
            doc: Rc::new(parse_document("")),
            revision: 0,
            selection: Selection::default(),
            marked: None,
            history: History::default(),
            closed: Vec::new(),
            goal_x: None,
            scroll: LiveScroll::default(),
            reveal_caret: false,
            frame: None,
            frame_key: None,
            bounds: Bounds::default(),
            views: Vec::new(),
            view_selection: None,
            views_revision: u64::MAX,
            shapes: ShapeCache::default(),
            images: ImageStore::default(),
            images_revision: u64::MAX,
            diagrams: Diagrams::default(),
            tokens: CodeTokens::default(),
            tokens_revision: u64::MAX,
            press: None,
            caret_on: true,
            blink_epoch: 0,
            pointer: None,
            hover: Hover::default(),
            draft: None,
            menu_property: None,
            em: 16.,
            placeholder: SharedString::default(),
            base_dir: None,
            paste_handler: None,
            notes: None,
            surface: None,
            find: FindBar {
                open: false,
                replacing: false,
                query,
                replacement,
                matches: Vec::new(),
                current: None,
            },
            _subscriptions: subscriptions,
        }
    }

    /// The text shown while the document is empty.
    pub fn placeholder(mut self, text: impl Into<SharedString>) -> Self {
        self.placeholder = text.into();
        self
    }

    /// The base font size every metric scales from.
    pub fn set_text_size(&mut self, size: Pixels, cx: &mut Context<Self>) {
        if self.em != size.as_f32() {
            self.em = size.as_f32();
            self.views_revision = u64::MAX;
            cx.notify();
        }
    }

    /// The folder relative image paths resolve against: the note's own.
    pub fn set_base_dir(&mut self, dir: Option<PathBuf>, cx: &mut Context<Self>) {
        if self.base_dir != dir {
            self.base_dir = dir;
            self.images.clear();
            self.images_revision = u64::MAX;
            self.frame_key = None;
            cx.notify();
        }
    }

    /// The note names `[[wikilinks]]` can reach, file stems in any case.
    /// Anything else renders as an unresolved link.
    pub fn set_note_names(
        &mut self,
        names: impl IntoIterator<Item = String>,
        cx: &mut Context<Self>,
    ) {
        let notes: HashSet<String> = names
            .into_iter()
            .map(|name| super::display::note_key(&name))
            .collect();
        if self.notes.as_ref() != Some(&notes) {
            self.notes = Some(notes);
            self.views_revision = u64::MAX;
            self.frame_key = None;
            cx.notify();
        }
    }

    /// What the host paints behind the text: what a diagram's labels mask
    /// its lines with.
    pub(super) fn set_surface(&mut self, surface: Option<Hsla>, cx: &mut Context<Self>) {
        if self.surface != surface {
            self.surface = surface;
            cx.notify();
        }
    }

    pub(super) fn set_paste_handler(&mut self, handler: Option<PasteHandler>) {
        self.paste_handler = handler;
    }

    /// The document text.
    pub fn value(&self) -> SharedString {
        self.text.clone()
    }

    /// The selection as an ordered byte range.
    pub fn selected_range(&self) -> Range<usize> {
        self.selection.range()
    }

    /// The focus handle of the editor.
    pub fn focus_handle(&self) -> &FocusHandle {
        &self.focus_handle
    }

    pub(super) fn scroll_handle(&self) -> LiveScroll {
        self.scroll.clone()
    }

    /// Moves focus into the editor.
    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus_handle, cx);
    }

    /// Replaces the document, forgetting its undo history: opening a note.
    pub fn set_value(
        &mut self,
        text: impl Into<SharedString>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.text = text.into();
        self.history = History::default();
        self.closed.clear();
        self.marked = None;
        self.goal_x = None;
        self.draft = None;
        self.scroll.set_top(0.);
        self.reparse();
        self.selection = Selection::caret(self.note_start());
        cx.emit(LiveEditorEvent::Change);
        cx.notify();
    }

    /// Replaces the whole text as one undoable edit.
    pub fn replace_all(
        &mut self,
        text: impl Into<SharedString>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let text: SharedString = text.into();
        let caret = self.selection.head.min(text.len());
        let caret = (0..=caret)
            .rev()
            .find(|&at| text.is_char_boundary(at))
            .unwrap_or(0);
        self.apply(
            Change {
                text: text.to_string(),
                selection: Selection::caret(caret),
                closed: None,
            },
            Group::Single,
            cx,
        );
    }

    /// Replaces the selection with `text`, undoable.
    pub fn insert(&mut self, text: impl AsRef<str>, _window: &mut Window, cx: &mut Context<Self>) {
        self.replace_selection(text.as_ref(), Group::Single, cx);
    }

    /// Sets the selection; offsets clamp to the text.
    pub fn set_selected_range(&mut self, range: Range<usize>, cx: &mut Context<Self>) {
        let start = self.clamp_offset(range.start);
        let end = self.clamp_offset(range.end);
        self.set_selection(
            Selection {
                anchor: start,
                head: end,
            },
            cx,
        );
    }

    /// The bounds of `range` in window coordinates: its first row, or the
    /// checkbox it is the source of.
    pub fn range_to_bounds(&self, range: &Range<usize>) -> Option<Bounds<Pixels>> {
        let frame = self.frame.as_ref()?;
        let index = self.doc.line_at(range.start);
        let line = frame.lines.get(index)?;
        for decor in &line.view.decor {
            if let super::display::Decor::Checkbox { x, source, .. } = decor
                && source.start <= range.start
                && range.end <= source.end
            {
                let size_px = line.view.text.size.0;
                let origin =
                    self.doc_to_window(frame, x.0, line.top + (line.height - size_px) / 2.);
                return Some(Bounds::new(origin, size(px(size_px), px(size_px))));
            }
        }
        let spot = frame.caret(&self.doc, range.start)?;
        let end = frame.caret(&self.doc, range.end)?;
        let origin = self.doc_to_window(frame, spot.x, spot.y);
        let width = if end.y == spot.y {
            (end.x - spot.x).max(1.)
        } else {
            1.
        };
        Some(Bounds::new(origin, size(px(width), px(spot.height))))
    }

    /// The caret's frame-independent facts, for tests and the painter.
    pub(super) fn caret_visible(&self, window: &Window) -> bool {
        self.caret_on && self.focus_handle.is_focused(window)
    }

    pub(super) fn selection(&self) -> Selection {
        self.selection
    }

    pub(super) fn marked_range(&self) -> Option<Range<usize>> {
        self.marked.clone()
    }

    pub(super) fn document(&self) -> Rc<LiveDocument> {
        self.doc.clone()
    }

    pub(super) fn placeholder_text(&self) -> SharedString {
        self.placeholder.clone()
    }

    pub(super) fn find_bar(&self) -> &FindBar {
        &self.find
    }

    /// The pointer's shape, when not the text's I-beam.
    pub(super) fn pointer(&self) -> Option<CursorStyle> {
        if self.press.is_some_and_link() {
            return Some(CursorStyle::PointingHand);
        }
        self.pointer
    }

    pub(super) fn hover(&self) -> Hover {
        self.hover
    }

    /// The pointer left the editor.
    pub(super) fn clear_hover(&mut self, cx: &mut Context<Self>) {
        if self.hover != Hover::default() {
            self.hover = Hover::default();
            cx.notify();
        }
    }

    /// Whether the context menu was opened on a property row.
    pub(super) fn menu_on_property(&self) -> bool {
        self.menu_property.is_some()
    }

    pub(super) fn em(&self) -> f32 {
        self.em
    }

    /// The pixels behind an image key of a view.
    pub(super) fn image_data(&self, key: &str) -> Option<std::sync::Arc<gpui::RenderImage>> {
        self.images.data(key).or_else(|| self.diagrams.data(key))
    }

    /// A diagram finished rendering off the UI thread.
    #[cfg(feature = "mermaid")]
    pub(super) fn diagram_rendered(
        &mut self,
        hash: u64,
        result: Result<super::diagram::backend::Picture, String>,
        cx: &mut Context<Self>,
    ) {
        if self.diagrams.finish(hash, result) {
            self.views_revision = u64::MAX;
            self.frame_key = None;
            cx.notify();
        }
    }

    fn clamp_offset(&self, offset: usize) -> usize {
        let offset = offset.min(self.text.len());
        (0..=offset)
            .rev()
            .find(|&at| self.text.is_char_boundary(at))
            .unwrap_or(0)
    }

    fn doc_to_window(&self, frame: &Frame, x: f32, y: f32) -> Point<Pixels> {
        point(
            self.bounds.origin.x + px(frame.column_x + x),
            self.bounds.origin.y + px(y - self.scroll.top()),
        )
    }

    fn window_to_doc(&self, frame: &Frame, position: Point<Pixels>) -> (f32, f32) {
        (
            (position.x - self.bounds.origin.x).as_f32() - frame.column_x,
            (position.y - self.bounds.origin.y).as_f32() + self.scroll.top(),
        )
    }

    // ---- layout ---------------------------------------------------------

    /// Lays the document out for this frame, reusing the last frame when
    /// nothing it depends on changed.
    pub(super) fn layout(
        &mut self,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Rc<Frame> {
        self.bounds = bounds;
        if self.images_revision != self.revision || self.images.pending() {
            let base_dir = self.base_dir.as_deref();
            if self.images.refresh(&self.doc, base_dir, window, cx) {
                self.views_revision = u64::MAX;
                self.frame_key = None;
            }
            self.images_revision = self.revision;
        }
        let focused = self.focus_handle.is_focused(window);
        let selection = focused.then(|| self.selection.range());
        let mut colors = LiveColors::from_theme(cx);
        colors.background = self.surface.unwrap_or(colors.background);
        let look = Look {
            colors,
            em: self.em,
            scale: window.scale_factor(),
            family: cx.theme().font_family.clone(),
        };
        let (doc, text) = (self.doc.clone(), self.text.clone());
        self.diagrams
            .refresh(self.revision, &text, &doc, &look, window, cx);
        let key = FrameKey {
            revision: self.revision,
            selection: selection.clone(),
            size: bounds.size,
            em: self.em,
            colors,
        };
        let reusable = self
            .frame
            .as_ref()
            .filter(|_| self.frame_key.as_ref() == Some(&key));
        let frame = match reusable {
            Some(frame) => frame.clone(),
            None => {
                if self
                    .frame_key
                    .as_ref()
                    .is_some_and(|last| last.colors != colors || last.em != self.em)
                {
                    self.shapes.clear();
                    self.views_revision = u64::MAX;
                }
                self.refresh_views(selection, cx);
                let theme = cx.theme();
                let fonts = Fonts::new(
                    font(theme.font_family.clone()),
                    font(theme.mono_font_family.clone()),
                    window,
                );
                let style = frame::Style {
                    em: self.em,
                    fonts: &fonts,
                    colors: &colors,
                };
                let frame = Rc::new(frame::build(
                    &self.doc,
                    &self.views,
                    bounds.size.width.as_f32(),
                    bounds.size.height.as_f32(),
                    &style,
                    &mut self.shapes,
                    window,
                ));
                self.frame = Some(frame.clone());
                self.frame_key = Some(key);
                frame
            }
        };
        self.scroll
            .update_extent(bounds, frame.height, frame.max_scroll);
        if std::mem::take(&mut self.reveal_caret) {
            self.scroll_caret_into_view(&frame, bounds.size.height.as_f32());
        }
        frame
    }

    fn scroll_caret_into_view(&mut self, frame: &Frame, viewport: f32) {
        let Some(spot) = frame.caret(&self.doc, self.selection.head) else {
            return;
        };
        let margin = self.em * CARET_MARGIN_LINES;
        let top = self.scroll.top();
        if spot.y - margin < top {
            // near the top of the note, all the way up: the margin must not
            // leave the first lines scrolled off
            let first = frame
                .lines
                .first()
                .map_or(0., |line| line.top + line.height);
            let target = if spot.y <= first { 0. } else { spot.y - margin };
            self.scroll.set_top(target);
        } else if spot.y + spot.height + margin > top + viewport {
            self.scroll
                .set_top(spot.y + spot.height + margin - viewport);
        }
    }

    /// Rebuilds the line views that can differ: all of them after an edit,
    /// otherwise only lines the old or new selection touches, with the whole
    /// block around them since blocks reveal as one.
    fn refresh_views(&mut self, selection: Option<Range<usize>>, cx: &mut Context<Self>) {
        if self.tokens_revision != self.revision {
            let theme = gpui_kit::component::Theme::global(cx)
                .highlight_theme
                .clone();
            self.tokens.update(&self.text, &self.doc, &theme);
            self.tokens_revision = self.revision;
        }
        let doc = self.doc.clone();
        let text = self.text.clone();
        let tokens = &self.tokens;
        let lookup = |block: usize| tokens.block(block);
        let images = &self.images;
        let image_status = |src: &str| images.status(src);
        let diagrams = &self.diagrams;
        let diagram_status = |block: usize| diagrams.status(block);
        let context = ViewContext {
            text: &text,
            doc: &doc,
            reveal: Reveal {
                selection: selection.clone(),
            },
            em: self.em,
            tokens: &lookup,
            notes: self.notes.as_ref(),
            images: &image_status,
            diagrams: &diagram_status,
        };
        if self.views_revision != self.revision || self.views.len() != doc.lines.len() {
            self.views = (0..doc.lines.len())
                .map(|ix| Rc::new(build_view(&context, ix)))
                .collect();
        } else if self.view_selection != selection {
            let touched = |range: &Option<Range<usize>>| -> Option<Range<usize>> {
                let range = range.as_ref()?;
                let mut first = doc.line_at(range.start);
                let mut last = doc.line_at(range.end);
                if let Some(block) = doc.lines[first].block {
                    first = first.min(doc.blocks[block].lines.start);
                }
                if let Some(block) = doc.lines[last].block {
                    last = last.max(doc.blocks[block].lines.end.saturating_sub(1));
                }
                Some(first..last + 1)
            };
            let spans = [touched(&self.view_selection), touched(&selection)];
            for span in spans.into_iter().flatten() {
                for ix in span {
                    self.views[ix] = Rc::new(build_view(&context, ix));
                }
            }
        }
        self.views_revision = self.revision;
        self.view_selection = selection;
    }

    // ---- editing --------------------------------------------------------

    fn reparse(&mut self) {
        self.doc = Rc::new(parse_document(&self.text));
        self.revision += 1;
    }

    fn after_edit(&mut self, cx: &mut Context<Self>) {
        if self.find.open {
            self.refresh_matches(cx);
        }
        self.goal_x = None;
        self.reveal_caret = true;
        self.restart_blink(cx);
        cx.emit(LiveEditorEvent::Change);
        cx.notify();
    }

    fn apply(&mut self, change: Change, group: Group, cx: &mut Context<Self>) {
        let (edit, inserted_len) = ops::diff(&self.text, &change.text);
        let selection_moved = change.selection != self.selection;
        if edit.is_empty() && inserted_len == 0 {
            if selection_moved {
                self.selection = change.selection;
                self.goal_x = None;
                self.reveal_caret = true;
                self.restart_blink(cx);
                cx.notify();
            }
            return;
        }
        self.history.record(Step {
            at: edit.start,
            removed: self.text[edit.clone()].to_string(),
            inserted: change.text[edit.start..edit.start + inserted_len].to_string(),
            before: self.selection,
            after: change.selection,
            stamp: Instant::now(),
            group,
        });
        ops::map_closed(&mut self.closed, &edit, inserted_len);
        if let Some(at) = change.closed {
            self.closed.push(at);
        }
        self.text = change.text.into();
        self.selection = change.selection;
        self.reparse();
        // a row just emptied by hand goes the way a row added blank does
        let line = self.doc.line_at(self.selection.head);
        if self.blank_row(line).is_some() {
            self.draft = Some(line);
        }
        self.after_edit(cx);
        self.settle_draft(Some(line), cx);
    }

    fn replace_range(
        &mut self,
        range: Range<usize>,
        with: &str,
        group: Group,
        cx: &mut Context<Self>,
    ) {
        let text = format!(
            "{}{with}{}",
            &self.text[..range.start],
            &self.text[range.end..]
        );
        let caret = range.start + with.len();
        self.apply(
            Change {
                text,
                selection: Selection::caret(caret),
                closed: None,
            },
            group,
            cx,
        );
    }

    fn replace_selection(&mut self, with: &str, group: Group, cx: &mut Context<Self>) {
        self.replace_range(self.selection.range(), with, group, cx);
    }

    fn set_selection(&mut self, selection: Selection, cx: &mut Context<Self>) {
        self.goal_x = None;
        self.place_selection(selection, cx);
    }

    fn place_selection(&mut self, selection: Selection, cx: &mut Context<Self>) {
        self.selection = selection;
        self.reveal_caret = true;
        // a caret leaving its line retires the closers typed there
        let line = self.doc.line_at(selection.head);
        let span = self.doc.lines[line].range.clone();
        self.closed
            .retain(|at| span.start <= *at && *at <= span.end);
        self.restart_blink(cx);
        self.settle_draft(Some(line), cx);
        cx.notify();
    }

    fn move_caret(&mut self, offset: usize, extend: bool, cx: &mut Context<Self>) {
        let head = self.clamp_offset(offset);
        let anchor = if extend { self.selection.anchor } else { head };
        self.set_selection(Selection { anchor, head }, cx);
    }

    /// The block around `offset`.
    fn block_at(&self, offset: usize) -> Option<&LiveBlock> {
        let line = &self.doc.lines[self.doc.line_at(offset)];
        Some(&self.doc.blocks[line.block?])
    }

    /// The frontmatter and its properties, when `offset` is inside it.
    fn properties_at(&self, offset: usize) -> Option<(&LiveBlock, &[Property])> {
        let block = self.block_at(offset)?;
        match &block.kind {
            LiveBlockKind::Frontmatter { properties } => Some((block, properties)),
            _ => None,
        }
    }

    /// The table cell or property field holding `offset`.
    fn field_at(&self, offset: usize) -> Option<Range<usize>> {
        let block = self.block_at(offset)?;
        let fields: Vec<Range<usize>> = match &block.kind {
            LiveBlockKind::Table { rows, .. } => {
                rows.iter().flat_map(|row| row.cells.clone()).collect()
            }
            LiveBlockKind::Frontmatter { properties } => {
                properties.iter().flat_map(Property::fields).collect()
            }
            _ => return None,
        };
        fields
            .into_iter()
            .find(|field| field.start <= offset && offset <= field.end)
    }

    /// Where the note's body starts: past the Properties panel, where the
    /// caret lands on opening a note.
    fn note_start(&self) -> usize {
        self.doc
            .blocks
            .iter()
            .find(|block| matches!(block.kind, LiveBlockKind::Frontmatter { .. }))
            .map_or(0, |block| (block.range.end + 1).min(self.text.len()))
    }

    /// Inside the panel only its fields hold the caret; everything between
    /// them is stepped over in the direction of travel.
    fn snap_to_field(
        &self,
        block: &LiveBlock,
        properties: &[Property],
        offset: usize,
        forward: bool,
    ) -> usize {
        let fields: Vec<Range<usize>> = properties.iter().flat_map(Property::fields).collect();
        if fields
            .iter()
            .any(|field| field.start <= offset && offset <= field.end)
        {
            return offset;
        }
        if forward {
            if let Some(field) = fields.iter().find(|field| field.start >= offset) {
                return field.start;
            }
            return (block.range.end + 1).min(self.text.len());
        }
        fields
            .iter()
            .rev()
            .find(|field| field.end <= offset)
            .or(fields.first())
            .map_or(offset, |field| {
                if field.end <= offset {
                    field.end
                } else {
                    field.start
                }
            })
    }

    /// Only a table's cells hold the caret: the pipes around them and the
    /// delimiter row are stepped over in the direction of travel.
    fn snap(&self, offset: usize, forward: bool) -> usize {
        let index = self.doc.line_at(offset);
        let line = &self.doc.lines[index];
        let Some(block) = line.block.map(|b| &self.doc.blocks[b]) else {
            return offset;
        };
        if let LiveBlockKind::Frontmatter { properties } = &block.kind {
            return self.snap_to_field(block, properties, offset, forward);
        }
        let LiveBlockKind::Table {
            rows, delimiter, ..
        } = &block.kind
        else {
            return offset;
        };
        let last_cell_end = |row: &crate::live::TableRow| row.cells.last().map(|c| c.end);
        let first_cell_start = |row: &crate::live::TableRow| row.cells.first().map(|c| c.start);
        if index == *delimiter {
            let row = if forward {
                rows.iter().find(|r| r.line > index)
            } else {
                rows.iter().rev().find(|r| r.line < index)
            };
            let target = row.and_then(|row| {
                if forward {
                    first_cell_start(row)
                } else {
                    last_cell_end(row)
                }
            });
            return target.unwrap_or(offset);
        }
        let Some(row) = rows.iter().find(|r| r.line == index) else {
            return offset;
        };
        if row
            .cells
            .iter()
            .any(|cell| cell.start <= offset && offset <= cell.end)
        {
            return offset;
        }
        if forward {
            if let Some(cell) = row.cells.iter().find(|cell| cell.start >= offset) {
                return cell.start;
            }
            // past the last cell: the next line, whatever it is
            let next = line.range.end + 1;
            if next <= self.text.len() {
                return self.snap(next, true);
            }
            last_cell_end(row).unwrap_or(offset)
        } else {
            if let Some(cell) = row.cells.iter().rev().find(|cell| cell.end <= offset) {
                return cell.end;
            }
            if line.range.start == 0 {
                return first_cell_start(row).unwrap_or(0);
            }
            self.snap(line.range.start - 1, false)
        }
    }

    // ---- caret motion ---------------------------------------------------

    fn step_left(&self) -> usize {
        self.snap(ops::prev_grapheme(&self.text, self.selection.head), false)
    }

    fn step_right(&self) -> usize {
        self.snap(ops::next_grapheme(&self.text, self.selection.head), true)
    }

    pub(super) fn move_left(&mut self, _: &MoveLeft, _: &mut Window, cx: &mut Context<Self>) {
        let target = if self.selection.is_caret() {
            self.step_left()
        } else {
            self.selection.range().start
        };
        self.move_caret(target, false, cx);
    }

    pub(super) fn move_right(&mut self, _: &MoveRight, _: &mut Window, cx: &mut Context<Self>) {
        let target = if self.selection.is_caret() {
            self.step_right()
        } else {
            self.selection.range().end
        };
        self.move_caret(target, false, cx);
    }

    pub(super) fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.move_caret(self.step_left(), true, cx);
    }

    pub(super) fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.move_caret(self.step_right(), true, cx);
    }

    /// The offset one visual row up or down, keeping the goal column.
    fn vertical_target(&mut self, down: bool) -> Option<usize> {
        let frame = self.frame.clone()?;
        let spot = frame.caret(&self.doc, self.selection.head)?;
        let goal = *self.goal_x.get_or_insert(spot.x);
        if self.field_at(self.selection.head).is_some()
            && let Some(target) = self.table_neighbor(down)
        {
            return Some(self.snap(target, down));
        }
        // from the first or last row of the note the caret goes to its very
        // end, whatever padding the line keeps around that row
        let edge = if down { self.text.len() } else { 0 };
        if frame
            .caret(&self.doc, edge)
            .is_some_and(|end| end.y == spot.y)
        {
            return Some(edge);
        }
        let y = if down {
            spot.reach.end + 0.5
        } else {
            spot.reach.start - 0.5
        };
        if y >= frame.height {
            return Some(self.text.len());
        }
        let hit = frame.hit(goal, y);
        // a parked diagram opens from the side the caret comes from
        if let HitKind::Diagram(block) | HitKind::EditDiagram(block) = hit.kind {
            let range = &self.doc.blocks[block].range;
            return Some(if down { range.start } else { range.end });
        }
        Some(self.snap(self.clamp_offset(hit.offset), down))
    }

    fn move_vertically(&mut self, down: bool, extend: bool, cx: &mut Context<Self>) {
        if !extend && !self.selection.is_caret() {
            let range = self.selection.range();
            let edge = if down { range.end } else { range.start };
            self.selection = Selection::caret(edge);
        }
        let Some(head) = self.vertical_target(down) else {
            return;
        };
        let anchor = if extend { self.selection.anchor } else { head };
        self.place_selection(Selection { anchor, head }, cx);
    }

    pub(super) fn move_up(&mut self, _: &MoveUp, _: &mut Window, cx: &mut Context<Self>) {
        self.move_vertically(false, false, cx);
    }

    pub(super) fn move_down(&mut self, _: &MoveDown, _: &mut Window, cx: &mut Context<Self>) {
        self.move_vertically(true, false, cx);
    }

    pub(super) fn select_up(&mut self, _: &SelectUp, _: &mut Window, cx: &mut Context<Self>) {
        self.move_vertically(false, true, cx);
    }

    pub(super) fn select_down(&mut self, _: &SelectDown, _: &mut Window, cx: &mut Context<Self>) {
        self.move_vertically(true, true, cx);
    }

    fn page_target(&self, down: bool) -> Option<usize> {
        let frame = self.frame.as_ref()?;
        let spot = frame.caret(&self.doc, self.selection.head)?;
        let page = self.bounds.size.height.as_f32() * 0.9;
        let y = if down { spot.y + page } else { spot.y - page };
        Some(if y < 0. {
            0
        } else if y >= frame.height {
            self.text.len()
        } else {
            frame.hit(spot.x, y).offset
        })
    }

    pub(super) fn move_page_up(&mut self, _: &MovePageUp, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(target) = self.page_target(false) {
            self.move_caret(target, false, cx);
        }
    }

    pub(super) fn move_page_down(
        &mut self,
        _: &MovePageDown,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(target) = self.page_target(true) {
            self.move_caret(target, false, cx);
        }
    }

    /// Where the text of the caret's line starts, past its list or quote
    /// prefix and indent.
    fn content_start(&self, index: usize) -> usize {
        let line = &self.doc.lines[index];
        let body = &self.text[line.range.clone()];
        match &line.kind {
            LiveLineKind::List(item) => item.content_start().min(line.range.end),
            LiveLineKind::Heading { .. } => line.range.start,
            _ => {
                let from = line
                    .quote
                    .as_ref()
                    .map_or(line.range.start, |q| q.range.end);
                let rest = &body[from - line.range.start..];
                from + (rest.len() - rest.trim_start_matches([' ', '\t']).len())
            }
        }
    }

    /// Home: the start of a wrapped row, or the line's text, then its very
    /// start on a second press.
    fn home_target(&self) -> usize {
        let head = self.selection.head;
        if let Some(field) = self.field_at(head) {
            return field.start;
        }
        let index = self.doc.line_at(head);
        if let Some(start) = self.row_edge(head, false)
            && start > self.doc.lines[index].range.start
            && start != self.content_start(index)
        {
            return start;
        }
        let content = self.content_start(index);
        if head == content {
            self.doc.lines[index].range.start
        } else {
            content
        }
    }

    /// End: the end of the visual row.
    fn end_target(&self) -> usize {
        let head = self.selection.head;
        if let Some(field) = self.field_at(head) {
            return field.end;
        }
        self.row_edge(head, true)
            .unwrap_or(self.doc.lines[self.doc.line_at(head)].range.end)
    }

    /// The source offset at the start or end of the row showing `offset`.
    fn row_edge(&self, offset: usize, end: bool) -> Option<usize> {
        let frame = self.frame.as_ref()?;
        let line = frame.lines.get(self.doc.line_at(offset))?;
        let laid = line
            .boxes
            .iter()
            .find(|b| b.source.start <= offset && offset <= b.source.end)?;
        let view = &laid.text;
        let row = laid.shaped.row_of(view.display_of(offset));
        let display = &laid.shaped.rows[row].display;
        let bias = super::display::Bias::Left;
        if end {
            // a wrapped row ends on its trailing space, before the next row
            let last = row + 1 == laid.shaped.rows.len();
            let at = view.source_of(display.end, bias);
            Some(if last { laid.source.end.max(at) } else { at })
        } else {
            Some(view.source_of(display.start, super::display::Bias::Right))
        }
    }

    pub(super) fn move_home(&mut self, _: &MoveHome, _: &mut Window, cx: &mut Context<Self>) {
        self.move_caret(self.home_target(), false, cx);
    }

    pub(super) fn move_end(&mut self, _: &MoveEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.move_caret(self.end_target(), false, cx);
    }

    pub(super) fn move_to_start_of_line(
        &mut self,
        _: &MoveToStartOfLine,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_caret(self.home_target(), false, cx);
    }

    pub(super) fn move_to_end_of_line(
        &mut self,
        _: &MoveToEndOfLine,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_caret(self.end_target(), false, cx);
    }

    pub(super) fn select_to_start_of_line(
        &mut self,
        _: &SelectToStartOfLine,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_caret(self.home_target(), true, cx);
    }

    pub(super) fn select_to_end_of_line(
        &mut self,
        _: &SelectToEndOfLine,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_caret(self.end_target(), true, cx);
    }

    pub(super) fn move_to_start(
        &mut self,
        _: &MoveToStart,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_caret(self.note_start(), false, cx);
    }

    pub(super) fn move_to_end(&mut self, _: &MoveToEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.move_caret(self.text.len(), false, cx);
    }

    pub(super) fn select_to_start(
        &mut self,
        _: &SelectToStart,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_caret(0, true, cx);
    }

    pub(super) fn select_to_end(
        &mut self,
        _: &SelectToEnd,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_caret(self.text.len(), true, cx);
    }

    pub(super) fn move_to_previous_word(
        &mut self,
        _: &MoveToPreviousWord,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target = ops::word_left(&self.text, self.selection.head);
        self.move_caret(self.snap(target, false), false, cx);
    }

    pub(super) fn move_to_next_word(
        &mut self,
        _: &MoveToNextWord,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target = ops::word_right(&self.text, self.selection.head);
        self.move_caret(self.snap(target, true), false, cx);
    }

    pub(super) fn select_to_previous_word(
        &mut self,
        _: &SelectToPreviousWordStart,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target = ops::word_left(&self.text, self.selection.head);
        self.move_caret(self.snap(target, false), true, cx);
    }

    pub(super) fn select_to_next_word(
        &mut self,
        _: &SelectToNextWordEnd,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target = ops::word_right(&self.text, self.selection.head);
        self.move_caret(self.snap(target, true), true, cx);
    }

    pub(super) fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.selection = Selection {
            anchor: 0,
            head: self.text.len(),
        };
        self.goal_x = None;
        self.restart_blink(cx);
        cx.notify();
    }

    pub(super) fn escape(&mut self, _: &Escape, _: &mut Window, cx: &mut Context<Self>) {
        if self.selection.is_caret() {
            cx.propagate();
        } else {
            self.move_caret(self.selection.head, false, cx);
        }
    }

    // ---- deleting -------------------------------------------------------

    /// How far a deletion from the caret reaches toward `target`: a cell or
    /// property field keeps its syntax, and the fence closing the panel is
    /// not the body's to swallow.
    fn deletion_end(&self, target: usize) -> usize {
        let head = self.selection.head;
        if let Some(field) = self.field_at(head) {
            return target.clamp(field.start, field.end);
        }
        if self.properties_at(target).is_some() {
            return head;
        }
        target
    }

    /// Backspace in an empty list item takes the item away.
    fn remove_empty_item(&mut self, cx: &mut Context<Self>) -> bool {
        let head = self.selection.head;
        let Some((_, properties)) = self.properties_at(head) else {
            return false;
        };
        let found = properties.iter().find_map(|property| {
            let at = property
                .items
                .iter()
                .position(|item| item.text == (head..head) && item.line != property.line)?;
            Some((property.clone(), at))
        });
        let Some((property, at)) = found else {
            return false;
        };
        let caret = at
            .checked_sub(1)
            .and_then(|before| property.items.get(before))
            .map_or(property.key.end, |item| item.text.end);
        let mut change =
            ops::property_remove_item(&self.text, &self.doc, &property, Some(at), self.selection);
        change.selection = Selection::caret(caret);
        self.apply(change, Group::Single, cx);
        true
    }

    pub(super) fn backspace(&mut self, _: &Backspace, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selection.is_caret() {
            self.replace_selection("", Group::Single, cx);
            return;
        }
        if let Some(change) = ops::delete_pair(&self.text, self.selection) {
            self.apply(change, Group::Deleting, cx);
            return;
        }
        let head = self.selection.head;
        if head == 0 || self.remove_empty_item(cx) {
            return;
        }
        let start = self.deletion_end(ops::prev_grapheme(&self.text, head));
        self.replace_range(start..head, "", Group::Deleting, cx);
    }

    pub(super) fn delete(&mut self, _: &Delete, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selection.is_caret() {
            self.replace_selection("", Group::Single, cx);
            return;
        }
        let head = self.selection.head;
        if head < self.text.len() {
            let end = self.deletion_end(ops::next_grapheme(&self.text, head));
            self.replace_range(head..end, "", Group::Single, cx);
        }
    }

    pub(super) fn delete_word_left(
        &mut self,
        _: &DeleteToPreviousWordStart,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.selection.is_caret() {
            self.replace_selection("", Group::Single, cx);
            return;
        }
        let head = self.selection.head;
        let start = self.deletion_end(ops::word_left(&self.text, head));
        self.replace_range(start..head, "", Group::Single, cx);
    }

    pub(super) fn delete_word_right(
        &mut self,
        _: &DeleteToNextWordEnd,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.selection.is_caret() {
            self.replace_selection("", Group::Single, cx);
            return;
        }
        let head = self.selection.head;
        let end = self.deletion_end(ops::word_right(&self.text, head));
        self.replace_range(head..end, "", Group::Single, cx);
    }

    pub(super) fn delete_to_line_start(
        &mut self,
        _: &DeleteToBeginningOfLine,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let head = self.selection.head;
        let start = self.deletion_end(self.doc.lines[self.doc.line_at(head)].range.start);
        self.replace_range(start..head, "", Group::Single, cx);
    }

    pub(super) fn delete_to_line_end(
        &mut self,
        _: &DeleteToEndOfLine,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let head = self.selection.head;
        let end = self.deletion_end(self.doc.lines[self.doc.line_at(head)].range.end);
        self.replace_range(head..end, "", Group::Single, cx);
    }

    // ---- structure ------------------------------------------------------

    pub(super) fn enter(&mut self, action: &Enter, _: &mut Window, cx: &mut Context<Self>) {
        if action.secondary {
            return;
        }
        // Enter leaves a table cell for the same column one row down
        if let Some(target) = self.table_neighbor(true) {
            self.move_caret(target, false, cx);
            return;
        }
        if self.property_enter(cx) {
            return;
        }
        let change = if action.shift {
            ops::soft_break(&self.text, self.selection)
        } else {
            ops::enter(&self.text, self.selection)
        };
        self.apply(change, Group::Single, cx);
    }

    /// The cell one row off the caret's, when the caret is in a table.
    fn table_neighbor(&self, down: bool) -> Option<usize> {
        let head = self.selection.head;
        let index = self.doc.line_at(head);
        let line = &self.doc.lines[index];
        let block = &self.doc.blocks[line.block?];
        let LiveBlockKind::Table { rows, .. } = &block.kind else {
            return None;
        };
        let at = rows.iter().position(|row| row.line == index)?;
        let column = rows[at]
            .cells
            .iter()
            .position(|cell| cell.start <= head && head <= cell.end)?;
        let next = if down {
            rows.get(at + 1)
        } else {
            at.checked_sub(1).and_then(|at| rows.get(at))
        };
        match next {
            Some(row) => row.cells.get(column).map(|cell| cell.end),
            None if down => Some((block.range.end + 1).min(self.text.len())),
            None => Some(block.range.start.saturating_sub(1)),
        }
    }

    /// Tab steps through a table's cells; `None` outside a table.
    fn table_step(&self, forward: bool) -> Option<usize> {
        let head = self.selection.head;
        let index = self.doc.line_at(head);
        let line = &self.doc.lines[index];
        let block = &self.doc.blocks[line.block?];
        let LiveBlockKind::Table { rows, .. } = &block.kind else {
            return None;
        };
        let cells: Vec<&Range<usize>> = rows.iter().flat_map(|row| row.cells.iter()).collect();
        let at = cells
            .iter()
            .position(|cell| cell.start <= head && head <= cell.end)?;
        let next = if forward {
            cells.get(at + 1)
        } else {
            at.checked_sub(1).and_then(|at| cells.get(at))
        };
        next.map(|cell| cell.start).or(Some(head))
    }

    /// Enter in the panel: a key hands over to its value, a list item starts
    /// the next one, and a value keeps the caret, as Obsidian's does.
    fn property_enter(&mut self, cx: &mut Context<Self>) -> bool {
        let head = self.selection.head;
        let Some((_, properties)) = self.properties_at(head) else {
            return false;
        };
        let inside = |property: &&Property| {
            property
                .fields()
                .iter()
                .any(|field| field.start <= head && head <= field.end)
        };
        let Some(property) = properties.iter().find(inside).cloned() else {
            return false;
        };
        let in_key = !property.bare && property.key.start <= head && head <= property.key.end;
        if in_key {
            if property.fields().len() > 1 {
                self.move_caret(property.value.end, false, cx);
            }
            return true;
        }
        let item = property.items.iter().position(|item| {
            item.text.start <= head && head <= item.text.end && !item.text.is_empty()
        });
        let lone_tag = property.is_list() && !property.value.is_empty();
        if item.is_some() || lone_tag {
            self.add_item(property.line, item, cx);
        }
        true
    }

    /// Tab in the panel: key to value to the next key, then a new property.
    fn property_step(&self, forward: bool) -> Option<PropertyStep> {
        let head = self.selection.head;
        let (_, properties) = self.properties_at(head)?;
        let at = properties.iter().position(|property| {
            property
                .fields()
                .iter()
                .any(|field| field.start <= head && head <= field.end)
        })?;
        let property = &properties[at];
        let in_key = !property.bare && property.key.start <= head && head <= property.key.end;
        let key_of = |property: &Property| PropertyStep::Caret(property.key.end);
        if !forward {
            if !in_key {
                return Some(key_of(property));
            }
            let before = properties[..at].last();
            return Some(PropertyStep::Caret(before.map_or(head, |before| {
                before
                    .fields()
                    .last()
                    .map_or(before.key.end, |field| field.end)
            })));
        }
        if in_key {
            if !property.items.is_empty() {
                return Some(PropertyStep::AddItem(property.line));
            }
            if property.fields().len() > 1 {
                return Some(PropertyStep::Caret(property.value.end));
            }
        }
        match properties[at + 1..].iter().find(|next| !next.bare) {
            Some(next) => Some(key_of(next)),
            // a row still blank is where the new property would go
            None if self.blank_row(property.line).is_some() => Some(PropertyStep::Caret(head)),
            None => Some(PropertyStep::AddProperty),
        }
    }

    fn take_property_step(&mut self, forward: bool, cx: &mut Context<Self>) -> bool {
        match self.property_step(forward) {
            Some(PropertyStep::Caret(target)) => self.move_caret(target, false, cx),
            Some(PropertyStep::AddItem(line)) => self.add_item(line, None, cx),
            Some(PropertyStep::AddProperty) => self.add_property(cx),
            None => return false,
        }
        true
    }

    /// A row of the panel with nothing in it, as the bytes to delete: its
    /// whole line, an item's with the break before it.
    fn blank_row(&self, line: usize) -> Option<Range<usize>> {
        let offset = self.doc.lines.get(line)?.range.start;
        let (_, properties) = self.properties_at(offset)?;
        let whole = &self.doc.lines[line].range;
        for property in properties {
            let empty = property.key.is_empty() && property.value.is_empty();
            if property.line == line && empty && !property.bare && property.items.is_empty() {
                let next = self.doc.lines.get(line + 1)?;
                return Some(whole.start..next.range.start);
            }
            let blank_item = property.items.iter().any(|item| {
                item.line == line && item.line != property.line && item.text.is_empty()
            });
            if blank_item {
                return Some(self.doc.lines[line - 1].range.end..whole.end);
            }
        }
        None
    }

    /// Removes the draft row if it stayed blank and the caret is off it:
    /// `line` is the caret's line, `None` when the caret left the editor.
    fn settle_draft(&mut self, line: Option<usize>, cx: &mut Context<Self>) {
        let Some(draft) = self.draft.filter(|draft| Some(*draft) != line) else {
            return;
        };
        self.draft = None;
        let Some(row) = self.blank_row(draft) else {
            return;
        };
        let map = |at: usize| {
            if at >= row.end {
                at - row.len()
            } else {
                at.min(row.start)
            }
        };
        let selection = Selection {
            anchor: map(self.selection.anchor),
            head: map(self.selection.head),
        };
        let text = format!("{}{}", &self.text[..row.start], &self.text[row.end..]);
        // undoing the row's own insertion leaves nothing to undo later
        let inserted_here = self.history.undo.last().is_some_and(|step| {
            step.at == row.start
                && step.removed.is_empty()
                && step.inserted == self.text[row.clone()]
        });
        if inserted_here {
            self.history.undo.pop();
            self.text = text.into();
            self.selection = selection;
            self.reparse();
            self.after_edit(cx);
        } else {
            self.apply(
                Change {
                    text,
                    selection,
                    closed: None,
                },
                Group::Single,
                cx,
            );
        }
        // a caret that stayed on the row needs a field of its own again
        if self.selection.is_caret() {
            self.selection = Selection::caret(self.snap(self.selection.head, false));
        }
    }

    /// Starts a new property in the panel, or returns to a blank one.
    fn add_property(&mut self, cx: &mut Context<Self>) {
        let Some(block) = self
            .doc
            .blocks
            .iter()
            .find(|block| matches!(block.kind, LiveBlockKind::Frontmatter { .. }))
        else {
            return;
        };
        let lines = block.lines.clone();
        if let Some(line) = lines.clone().find(|line| self.blank_row(*line).is_some())
            && matches!(&self.doc.lines[line].kind, LiveLineKind::Frontmatter)
        {
            let offset = self.doc.lines[line].range.start;
            self.move_caret(self.snap(offset, true), false, cx);
            return;
        }
        let fence = self.doc.lines[lines.end - 1].range.start;
        let change = ops::add_property(&self.text, fence);
        self.draft = Some(lines.end - 1);
        self.apply(change, Group::Single, cx);
    }

    /// Starts a new item in the list property on `line`, after item `after`.
    fn add_item(&mut self, line: usize, after: Option<usize>, cx: &mut Context<Self>) {
        let offset = self.doc.lines[line].range.start;
        let Some((_, properties)) = self.properties_at(offset) else {
            return;
        };
        let Some(property) = properties.iter().find(|p| p.line == line) else {
            return;
        };
        let change = ops::property_add_item(&self.text, &self.doc, property, after);
        self.draft = None;
        self.apply(change, Group::Single, cx);
    }

    /// Takes away the list item around `offset`.
    fn remove_item(&mut self, offset: usize, cx: &mut Context<Self>) {
        let Some((_, properties)) = self.properties_at(offset) else {
            return;
        };
        let inside = |range: &Range<usize>| range.start <= offset && offset <= range.end;
        let found = properties.iter().find_map(|property| {
            let item = property.items.iter().position(|item| inside(&item.text));
            (item.is_some() || inside(&property.value)).then(|| (property.clone(), item))
        });
        let Some((property, item)) = found else {
            return;
        };
        let change =
            ops::property_remove_item(&self.text, &self.doc, &property, item, self.selection);
        self.apply(change, Group::Single, cx);
    }

    pub(super) fn remove_property(
        &mut self,
        _: &RemoveProperty,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(line) = self.menu_property.take() else {
            return;
        };
        let offset = self.doc.lines[line].range.start;
        let Some((_, properties)) = self.properties_at(offset) else {
            return;
        };
        let Some(property) = properties.iter().find(|p| p.line == line) else {
            return;
        };
        let change = ops::remove_property(&self.text, &self.doc, property, self.selection);
        self.apply(change, Group::Single, cx);
    }

    /// Adds to the table in block `block` from its add strip.
    fn add_to_table(&mut self, block: usize, strip: TableStrip, cx: &mut Context<Self>) {
        let change = match strip {
            TableStrip::Row => ops::table_add_row(&self.text, &self.doc, block),
            TableStrip::Column => ops::table_add_column(&self.text, &self.doc, block),
        };
        if let Some(change) = change {
            self.apply(change, Group::Single, cx);
        }
    }

    pub(super) fn indent_inline(
        &mut self,
        _: &IndentInline,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(target) = self.table_step(true) {
            self.move_caret(target, false, cx);
            return;
        }
        if self.take_property_step(true, cx) {
            return;
        }
        self.apply(ops::indent(&self.text, self.selection), Group::Single, cx);
    }

    pub(super) fn outdent_inline(
        &mut self,
        _: &OutdentInline,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(target) = self.table_step(false) {
            self.move_caret(target, false, cx);
            return;
        }
        if self.take_property_step(false, cx) {
            return;
        }
        self.apply(ops::outdent(&self.text, self.selection), Group::Single, cx);
    }

    pub(super) fn indent_lines(&mut self, _: &Indent, _: &mut Window, cx: &mut Context<Self>) {
        self.apply(ops::indent(&self.text, self.selection), Group::Single, cx);
    }

    pub(super) fn outdent_lines(&mut self, _: &Outdent, _: &mut Window, cx: &mut Context<Self>) {
        self.apply(ops::outdent(&self.text, self.selection), Group::Single, cx);
    }

    pub(super) fn toggle_bold(&mut self, _: &ToggleBold, _: &mut Window, cx: &mut Context<Self>) {
        let change = ops::toggle_wrap(&self.text, self.selection, "**");
        self.apply(change, Group::Single, cx);
    }

    pub(super) fn toggle_italic(
        &mut self,
        _: &ToggleItalic,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let change = ops::toggle_wrap(&self.text, self.selection, "*");
        self.apply(change, Group::Single, cx);
    }

    pub(super) fn insert_link(&mut self, _: &InsertLink, _: &mut Window, cx: &mut Context<Self>) {
        let change = ops::insert_link(&self.text, self.selection);
        self.apply(change, Group::Single, cx);
    }

    pub(super) fn toggle_task(&mut self, _: &ToggleTask, _: &mut Window, cx: &mut Context<Self>) {
        let change = ops::toggle_task_line(&self.text, self.selection);
        self.apply(change, Group::Single, cx);
    }

    pub(super) fn delete_line(&mut self, _: &DeleteLine, _: &mut Window, cx: &mut Context<Self>) {
        let change = ops::delete_line(&self.text, self.selection);
        self.apply(change, Group::Single, cx);
    }

    // ---- history --------------------------------------------------------

    pub(super) fn undo(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
        let Some(step) = self.history.undo.pop() else {
            return;
        };
        let end = step.at + step.inserted.len();
        let text = format!(
            "{}{}{}",
            &self.text[..step.at],
            step.removed,
            &self.text[end..]
        );
        self.text = text.into();
        self.selection = step.before;
        self.history.redo.push(step);
        self.closed.clear();
        self.reparse();
        self.after_edit(cx);
    }

    pub(super) fn redo(&mut self, _: &Redo, _: &mut Window, cx: &mut Context<Self>) {
        let Some(step) = self.history.redo.pop() else {
            return;
        };
        let end = step.at + step.removed.len();
        let text = format!(
            "{}{}{}",
            &self.text[..step.at],
            step.inserted,
            &self.text[end..]
        );
        self.text = text.into();
        self.selection = step.after;
        self.history.undo.push(step);
        self.closed.clear();
        self.reparse();
        self.after_edit(cx);
    }

    // ---- clipboard ------------------------------------------------------

    /// The selection, or the caret's whole line like every code editor.
    fn copy_range(&self) -> (Range<usize>, String) {
        let range = self.selection.range();
        if !range.is_empty() {
            return (range.clone(), self.text[range].to_string());
        }
        let index = self.doc.line_at(range.start);
        let line = &self.doc.lines[index];
        let end = self
            .doc
            .lines
            .get(index + 1)
            .map_or(line.range.end, |next| next.range.start);
        let mut copied = self.text[line.range.start..end].to_string();
        if !copied.ends_with('\n') {
            copied.push('\n');
        }
        (line.range.start..end, copied)
    }

    pub(super) fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        let (_, copied) = self.copy_range();
        cx.write_to_clipboard(ClipboardItem::new_string(copied));
    }

    pub(super) fn cut(&mut self, _: &Cut, _: &mut Window, cx: &mut Context<Self>) {
        let (range, copied) = self.copy_range();
        cx.write_to_clipboard(ClipboardItem::new_string(copied));
        self.replace_range(range, "", Group::Single, cx);
    }

    /// Pasted text as `at` can hold it: a field is one line.
    fn pasted_text<'a>(&self, text: &'a str, at: usize) -> Cow<'a, str> {
        if self.field_at(at).is_some() {
            Cow::Owned(text.replace(['\r', '\n'], " "))
        } else {
            Cow::Borrowed(text)
        }
    }

    pub(super) fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = cx.read_from_clipboard() else {
            return;
        };
        if let Some(handler) = self.paste_handler.clone() {
            // the hook may edit this very entity, so it runs after this
            // update releases it
            let editor = cx.entity();
            window.defer(cx, move |window, cx| {
                if !handler(&item, window, cx)
                    && let Some(text) = item.text()
                {
                    editor.update(cx, |this, cx| {
                        let text = this.pasted_text(&text, this.selection.head).into_owned();
                        this.replace_selection(&text, Group::Single, cx);
                    });
                }
            });
            return;
        }
        if let Some(text) = item.text() {
            let text = self.pasted_text(&text, self.selection.head).into_owned();
            self.replace_selection(&text, Group::Single, cx);
        }
    }

    // ---- find -----------------------------------------------------------

    fn open_find_bar(&mut self, replacing: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.find.open = true;
        self.find.replacing = replacing;
        // a one-line selection is what the user wants to look for
        let range = self.selection.range();
        let selected = &self.text[range.clone()];
        if !range.is_empty() && !selected.contains('\n') {
            let seed = selected.to_string();
            self.find
                .query
                .update(cx, |field, cx| field.set_value(seed, window, cx));
        }
        self.find
            .query
            .update(cx, |field, cx| field.focus(window, cx));
        self.refresh_matches(cx);
        cx.notify();
    }

    pub(super) fn search(&mut self, _: &Search, window: &mut Window, cx: &mut Context<Self>) {
        self.open_find_bar(false, window, cx);
    }

    pub(super) fn replace(&mut self, _: &Replace, window: &mut Window, cx: &mut Context<Self>) {
        self.open_find_bar(true, window, cx);
    }

    /// Closes the bar and hands the keyboard back to the text.
    pub(super) fn close_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.find.open = false;
        self.find.matches.clear();
        self.find.current = None;
        self.focus(window, cx);
        cx.notify();
    }

    pub(super) fn toggle_replace(&mut self, cx: &mut Context<Self>) {
        self.find.replacing = !self.find.replacing;
        cx.notify();
    }

    fn refresh_matches(&mut self, cx: &mut Context<Self>) {
        let query = self.find.query.read(cx).value();
        self.find.matches = ops::find_matches(&self.text, &query);
        let head = self.selection.range().start;
        self.find.current = self
            .find
            .matches
            .iter()
            .position(|m| m.start == head && m.end == self.selection.range().end);
    }

    fn find_query_changed(&mut self, cx: &mut Context<Self>) {
        self.refresh_matches(cx);
        // the first match at or after the caret, wrapping around
        let head = self.selection.range().start;
        let next = self
            .find
            .matches
            .iter()
            .position(|m| m.start >= head)
            .or((!self.find.matches.is_empty()).then_some(0));
        self.select_match(next, cx);
    }

    fn select_match(&mut self, index: Option<usize>, cx: &mut Context<Self>) {
        self.find.current = index;
        if let Some(found) = index.and_then(|ix| self.find.matches.get(ix)).cloned() {
            self.set_selection(
                Selection {
                    anchor: found.start,
                    head: found.end,
                },
                cx,
            );
        }
        cx.notify();
    }

    /// Moves to the next or previous match, wrapping at the ends.
    pub(super) fn find_step(&mut self, forward: bool, _: &mut Window, cx: &mut Context<Self>) {
        let count = self.find.matches.len();
        if count == 0 {
            return;
        }
        let head = self.selection.head;
        let next = match self.find.current {
            Some(ix) if forward => (ix + 1) % count,
            Some(ix) => (ix + count - 1) % count,
            None if forward => self
                .find
                .matches
                .iter()
                .position(|m| m.start >= head)
                .unwrap_or(0),
            None => self
                .find
                .matches
                .iter()
                .rposition(|m| m.end <= head)
                .unwrap_or(count - 1),
        };
        self.select_match(Some(next), cx);
    }

    /// Replaces the current match, then moves on to the next one.
    pub(super) fn replace_current(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(found) = self
            .find
            .current
            .and_then(|ix| self.find.matches.get(ix))
            .cloned()
        else {
            self.find_step(true, window, cx);
            return;
        };
        let with = self.find.replacement.read(cx).value();
        self.replace_range(found.clone(), &with, Group::Single, cx);
        let resume = found.start + with.len();
        let next = self.find.matches.iter().position(|m| m.start >= resume);
        let next = next.or((!self.find.matches.is_empty()).then_some(0));
        self.select_match(next, cx);
    }

    /// Replaces every match as one undo step.
    pub(super) fn replace_all_matches(&mut self, cx: &mut Context<Self>) {
        if self.find.matches.is_empty() {
            return;
        }
        let with = self.find.replacement.read(cx).value();
        let mut text = String::with_capacity(self.text.len());
        let mut cursor = 0;
        for found in &self.find.matches {
            text.push_str(&self.text[cursor..found.start]);
            text.push_str(&with);
            cursor = found.end;
        }
        text.push_str(&self.text[cursor..]);
        let caret = self.selection.head.min(text.len());
        let caret = (0..=caret)
            .rev()
            .find(|&at| text.is_char_boundary(at))
            .unwrap_or(0);
        self.apply(
            Change {
                text,
                selection: Selection::caret(caret),
                closed: None,
            },
            Group::Single,
            cx,
        );
    }

    // ---- mouse ----------------------------------------------------------

    fn hit_at(&self, position: Point<Pixels>) -> Option<Hit> {
        let frame = self.frame.as_ref()?;
        let (x, y) = self.window_to_doc(frame, position);
        let mut hit = frame.hit(x, y);
        // the frame can be one edit behind the text
        hit.offset = self.clamp_offset(hit.offset);
        Some(hit)
    }

    /// The link a rendered click would follow: a link whose syntax is hidden,
    /// with the pointer on its text.
    fn link_at(&self, hit: &Hit, focused: bool) -> Option<InlineObjectKind> {
        if hit.past_end || hit.kind != HitKind::Text {
            return None;
        }
        let object = frame::object_at(&self.doc, hit.offset)?;
        if !matches!(
            object.kind,
            InlineObjectKind::Link { .. } | InlineObjectKind::WikiLink { .. }
        ) {
            return None;
        }
        let revealed = focused && {
            let selection = self.selection.range();
            selection.start <= object.range.end && selection.end >= object.range.start
        };
        // the end of a link is where the next character's click lands too
        (!revealed && hit.offset < object.range.end).then(|| object.kind.clone())
    }

    pub(super) fn mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(hit) = self.hit_at(event.position) else {
            return;
        };
        let focused = self.focus_handle.is_focused(window);
        match &hit.kind {
            HitKind::Text => {}
            HitKind::Inert | HitKind::Diagram(_) => return,
            HitKind::Checkbox(range) => return self.toggle_box(range.clone(), cx),
            HitKind::EditDiagram(block) => {
                let first = self.doc.blocks[*block].lines.start + 1;
                return self.move_caret(self.doc.lines[first].range.start, false, cx);
            }
            HitKind::TableStrip(block, strip) => return self.add_to_table(*block, *strip, cx),
            HitKind::AddProperty => return self.add_property(cx),
            HitKind::AddItem(line) => return self.add_item(*line, None, cx),
            HitKind::RemoveItem(offset) => return self.remove_item(*offset, cx),
        }
        let plain = !event.modifiers.shift;
        match event.click_count {
            1 => {
                if plain && let Some(kind) = self.link_at(&hit, focused) {
                    self.press = Some(Press::Link {
                        kind,
                        origin: event.position,
                    });
                    cx.notify();
                    return;
                }
                self.move_caret(self.snap(hit.offset, true), event.modifiers.shift, cx);
            }
            2 => {
                let range = ops::group_at(&self.text, hit.offset);
                self.set_selection(
                    Selection {
                        anchor: range.start,
                        head: range.end,
                    },
                    cx,
                );
            }
            _ => {
                let index = self.doc.line_at(hit.offset);
                let line = &self.doc.lines[index];
                let end = self
                    .doc
                    .lines
                    .get(index + 1)
                    .map_or(line.range.end, |next| next.range.start);
                self.set_selection(
                    Selection {
                        anchor: line.range.start,
                        head: end,
                    },
                    cx,
                );
            }
        }
        self.press = Some(Press::Selecting);
    }

    /// A right press outside the selection moves the caret there, so the menu
    /// it opens acts on what was clicked.
    pub(super) fn right_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(hit) = self.hit_at(event.position) else {
            return;
        };
        self.menu_property = self.frame.as_ref().and_then(|frame| {
            let (_, y) = self.window_to_doc(frame, event.position);
            let line = frame.line_at(y);
            frame.lines[line].view.property.is_some().then_some(line)
        });
        if hit.kind != HitKind::Text {
            return;
        }
        let selection = self.selection.range();
        if !(selection.start <= hit.offset && hit.offset <= selection.end) {
            self.move_caret(self.snap(hit.offset, true), false, cx);
        }
    }

    fn toggle_box(&mut self, range: Range<usize>, cx: &mut Context<Self>) {
        if matches!(&self.text[range.clone()], "true" | "false") {
            let change = ops::toggle_property(&self.text, range, self.selection);
            return self.apply(change, Group::Single, cx);
        }
        let mark = if self.text[range.clone()] == *"[ ]" {
            "[x]"
        } else {
            "[ ]"
        };
        let text = format!(
            "{}{mark}{}",
            &self.text[..range.start],
            &self.text[range.end..]
        );
        self.apply(
            Change {
                text,
                selection: self.selection,
                closed: None,
            },
            Group::Single,
            cx,
        );
    }

    pub(super) fn mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match &self.press {
            Some(Press::Selecting) if event.pressed_button == Some(MouseButton::Left) => {
                let Some(hit) = self.hit_at(event.position) else {
                    return;
                };
                let head = self.snap(hit.offset, true);
                if head != self.selection.head {
                    self.goal_x = None;
                    self.selection.head = head;
                    self.restart_blink(cx);
                    cx.notify();
                }
                self.drag_scroll(event.position, cx);
            }
            Some(Press::Link { origin, .. }) => {
                let moved =
                    (event.position.x - origin.x).abs() + (event.position.y - origin.y).abs();
                if moved > px(4.) {
                    self.press = None;
                    cx.notify();
                }
            }
            _ => {
                let focused = self.focus_handle.is_focused(window);
                let pointer = self.hit_at(event.position).and_then(|hit| match hit.kind {
                    HitKind::Text if self.link_at(&hit, focused).is_some() => {
                        Some(CursorStyle::PointingHand)
                    }
                    HitKind::Text => None,
                    HitKind::Inert | HitKind::Diagram(_) => Some(CursorStyle::Arrow),
                    _ => Some(CursorStyle::PointingHand),
                });
                let hover = self.frame.as_ref().map_or(Hover::default(), |frame| {
                    let (x, y) = self.window_to_doc(frame, event.position);
                    frame.hover_at(x, y)
                });
                if pointer != self.pointer || hover != self.hover {
                    self.pointer = pointer;
                    self.hover = hover;
                    cx.notify();
                }
            }
        }
    }

    /// Dragging past the top or bottom edge scrolls toward the pointer.
    fn drag_scroll(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let top = self.bounds.origin.y;
        let bottom = top + self.bounds.size.height;
        let delta = if position.y < top {
            (position.y - top).as_f32()
        } else if position.y > bottom {
            (position.y - bottom).as_f32()
        } else {
            return;
        };
        self.scroll.set_top(self.scroll.top() + delta);
        cx.notify();
    }

    pub(super) fn mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        match self.press.take() {
            Some(Press::Link { kind, .. }) => {
                cx.emit(match kind {
                    InlineObjectKind::Link { url } if url.contains("://") => {
                        LiveEditorEvent::OpenUrl(url)
                    }
                    InlineObjectKind::Link { url } => LiveEditorEvent::OpenNote(url),
                    InlineObjectKind::WikiLink { target } => LiveEditorEvent::OpenNote(target),
                    InlineObjectKind::Image { src, .. } => LiveEditorEvent::OpenNote(src),
                });
                cx.notify();
            }
            Some(Press::Selecting) | None => {}
        }
    }

    pub(super) fn scroll_wheel(
        &mut self,
        event: &ScrollWheelEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let delta = event.delta.pixel_delta(px(self.em * WHEEL_LINE_EM));
        self.scroll.set_top(self.scroll.top() - delta.y.as_f32());
        cx.notify();
    }

    // ---- caret blink ----------------------------------------------------

    fn restart_blink(&mut self, cx: &mut Context<Self>) {
        self.blink_epoch += 1;
        self.caret_on = true;
        let epoch = self.blink_epoch;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(BLINK_INTERVAL).await;
                let running = this.update(cx, |this, cx| {
                    if this.blink_epoch != epoch {
                        return false;
                    }
                    this.caret_on = !this.caret_on;
                    cx.notify();
                    true
                });
                if !matches!(running, Ok(true)) {
                    break;
                }
            }
        })
        .detach();
    }

    // ---- utf-16 ---------------------------------------------------------

    fn offset_to_utf16(&self, offset: usize) -> usize {
        self.text[..offset.min(self.text.len())]
            .encode_utf16()
            .count()
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut units = 0;
        for (ix, c) in self.text.char_indices() {
            if units >= offset {
                return ix;
            }
            units += c.len_utf16();
        }
        self.text.len()
    }

    /// What must precede text inserted at `range` for the note to stay valid.
    fn insertion_lead(&self, range: &Range<usize>) -> Option<&'static str> {
        if !range.is_empty() {
            return None;
        }
        let (block, properties) = self.properties_at(range.start)?;
        if range.start > self.doc.lines[block.lines.end - 1].range.start {
            return Some("\n");
        }
        let empty_value = properties.iter().any(|property| {
            property.value == *range && !property.bare && property.items.is_empty()
        });
        (empty_value && self.text[..range.start].ends_with(':')).then_some(" ")
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn range_from_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range.start)..self.offset_from_utf16(range.end)
    }
}

/// `Option<Press>` queries.
trait PressExt {
    fn is_some_and_link(&self) -> bool;
}

impl PressExt for Option<Press> {
    fn is_some_and_link(&self) -> bool {
        matches!(self, Some(Press::Link { .. }))
    }
}

/// Binds the markdown shortcuts once per app. They sit in the editor's own key
/// context, so they cannot shadow an app's bindings elsewhere.
fn ensure_keys_bound(cx: &mut App) {
    struct Bound;
    impl gpui::Global for Bound {}
    if cx.has_global::<Bound>() {
        return;
    }
    cx.set_global(Bound);
    let context = Some("LiveEditor");
    cx.bind_keys([
        KeyBinding::new("secondary-b", ToggleBold, context),
        KeyBinding::new("secondary-i", ToggleItalic, context),
        KeyBinding::new("secondary-k", InsertLink, context),
        KeyBinding::new("secondary-l", ToggleTask, context),
        KeyBinding::new("secondary-d", DeleteLine, context),
        KeyBinding::new("secondary-shift-z", Redo, context),
    ]);
}

impl Focusable for LiveEditorState {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EntityInputHandler for LiveEditorState {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range);
        adjusted.replace(self.range_to_utf16(&range));
        Some(self.text[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selection.range()),
            reversed: self.selection.head < self.selection.anchor,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked.as_ref().map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.marked = None;
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target = range
            .as_ref()
            .map(|range| self.range_from_utf16(range))
            .or_else(|| self.marked.clone());
        self.marked = None;

        // the first character in an empty value brings the space `key:` lacks,
        // and text after a fence closing the note starts the body's first line
        let at = target.as_ref().map_or(self.selection.range(), Range::clone);
        if let Some(lead) = self.insertion_lead(&at) {
            self.replace_range(at, &format!("{lead}{text}"), Group::Typing, cx);
            return;
        }

        // a plain keystroke goes through auto-pairing
        if target.is_none() {
            let mut chars = text.chars();
            if let (Some(ch), None) = (chars.next(), chars.next())
                && let Some(change) = ops::type_char(&self.text, self.selection, ch, &self.closed)
            {
                self.apply(change, Group::Single, cx);
                return;
            }
        }
        let target = target.unwrap_or_else(|| self.selection.range());
        self.replace_range(target, text, Group::Typing, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        new_text: &str,
        new_selected: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target = range
            .as_ref()
            .map(|range| self.range_from_utf16(range))
            .or_else(|| self.marked.clone())
            .unwrap_or_else(|| self.selection.range());
        let start = target.start;
        self.replace_range(target, new_text, Group::Single, cx);
        self.marked = (!new_text.is_empty()).then(|| start..start + new_text.len());
        if let Some(selected) = new_selected {
            let inner = new_text
                .char_indices()
                .map(|(ix, _)| ix)
                .chain(std::iter::once(new_text.len()));
            let mut units = 0;
            let mut anchor = new_text.len();
            let mut head = new_text.len();
            for (ix, c) in inner.zip(new_text.chars().map(Some).chain(std::iter::once(None))) {
                if units == selected.start {
                    anchor = ix;
                }
                if units == selected.end {
                    head = ix;
                }
                units += c.map_or(0, char::len_utf16);
            }
            self.selection = Selection {
                anchor: start + anchor,
                head: start + head,
            };
        }
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        _element_bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let range = self.range_from_utf16(&range);
        self.range_to_bounds(&range)
    }

    fn character_index_for_point(
        &mut self,
        position: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let hit = self.hit_at(position)?;
        Some(self.offset_to_utf16(hit.offset))
    }
}

/// The scrollbar for `scroll`.
pub(super) fn scrollbar(scroll: &LiveScroll) -> Scrollbar {
    Scrollbar::vertical(scroll).id("live-editor-scrollbar")
}

/// Marks the entity as the input target of `bounds`.
pub(super) fn input_handler(
    bounds: Bounds<Pixels>,
    state: Entity<LiveEditorState>,
) -> gpui::ElementInputHandler<LiveEditorState> {
    gpui::ElementInputHandler::new(bounds, state)
}
