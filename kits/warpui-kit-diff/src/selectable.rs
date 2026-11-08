//! A selectable run for the diff body.
//!
//! The patch is drawn as one [`StyledText`] so selection can cross lines: the
//! window selection system needs every line's geometry registered together,
//! and a row-per-div layout cannot provide that. This element is the diff's
//! adaptation of gpui-base's `SelectableText`; that one builds its own
//! unstyled text, while the diff needs the runs that carry token colors and
//! emphasis, so the `StyledText` is passed in.

use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;

use gpui::{
    App, BorderStyle, Bounds, Corners, Edges, Element, ElementId, GlobalElementId, Hitbox,
    HitboxBehavior, Hsla, InspectorElementId, IntoElement, LayoutId, PaintQuad, Pixels, Point,
    SharedString, StyledText, Window, transparent_black,
};
use gpui_kit::base::{TextSelectionHandle, TextSelectionRegistration, TextSelectionRun};
use warpui_kit_core::palette;

/// Per-element state: the selection handle and the shaped text. `StyledText`
/// is not cloneable and its layout caches the shaping, so the element reuses
/// one instance across frames — a selection repaint must not re-shape a 64KB
/// patch on every mouse move.
pub struct Retained {
    key: u64,
    handle: TextSelectionHandle,
    styled: StyledText,
    /// The selected text, recomputed from the snapshot in paint. The window
    /// selection's own projection walks every character of the run, which is
    /// far too slow for a 64KB patch; the copy callback returns this instead.
    copy: Rc<RefCell<String>>,
}

/// One selectable text block: the whole visible diff, gutters excluded.
pub struct SelectableText {
    id: ElementId,
    text: SharedString,
    styled_text: StyledText,
    key: u64,
}

impl SelectableText {
    /// `text` must be the exact string `styled_text` was built from; the
    /// selection run maps that text onto the layout. `key` identifies the
    /// text, runs and font size; the retained layout is reused while it holds.
    pub fn new(
        id: impl Into<ElementId>,
        text: SharedString,
        styled_text: StyledText,
        key: u64,
    ) -> Self {
        Self {
            id: id.into(),
            text,
            styled_text,
            key,
        }
    }

    fn paint_selection(
        layout: &gpui::TextLayout,
        range: Range<usize>,
        color: Hsla,
        window: &mut Window,
    ) {
        let (Some(start), Some(end)) = (
            layout.position_for_index(range.start),
            layout.position_for_index(range.end),
        ) else {
            return;
        };
        for bounds in selection_quad_bounds(start, end, layout.bounds(), layout.line_height()) {
            window.paint_quad(PaintQuad {
                bounds,
                background: color.into(),
                corner_radii: Corners::default(),
                border_widths: Edges::default(),
                border_color: transparent_black(),
                border_style: BorderStyle::default(),
            });
        }
    }
}

/// The quads covering a selection from `start` to `end`, full-width on the
/// lines between them.
fn selection_quad_bounds(
    start: Point<Pixels>,
    end: Point<Pixels>,
    bounds: Bounds<Pixels>,
    line_height: Pixels,
) -> Vec<Bounds<Pixels>> {
    if start.y == end.y {
        return vec![Bounds::from_corners(
            start,
            Point::new(end.x, end.y + line_height),
        )];
    }

    let mut quads = vec![Bounds::from_corners(
        start,
        Point::new(bounds.right(), start.y + line_height),
    )];
    if end.y > start.y + line_height {
        quads.push(Bounds::from_corners(
            Point::new(bounds.left(), start.y + line_height),
            Point::new(bounds.right(), end.y),
        ));
    }
    quads.push(Bounds::from_corners(
        Point::new(bounds.left(), end.y),
        Point::new(end.x, end.y + line_height),
    ));
    quads
}

/// The caret index for a window point; both `Ok` and `Err` carry the nearest
/// boundary.
fn index_at(layout: &gpui::TextLayout, point: Point<Pixels>) -> usize {
    layout
        .index_for_position(point)
        .unwrap_or_else(|index| index)
}

impl IntoElement for SelectableText {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for SelectableText {
    type RequestLayoutState = Rc<RefCell<Retained>>;
    type PrepaintState = Hitbox;

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        global_id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        // The participant owns no view of its own, so a drag would only show
        // when something else re-rendered the page. Notify the current view
        // on every selection event, the way a TextView does.
        let view = window.current_view();
        let fallback = self.text.to_string();
        let key = self.key;
        let styled = std::mem::replace(&mut self.styled_text, StyledText::new(""));
        let retained = window.with_element_state(
            global_id.expect("SelectableText must have a stable element id"),
            |retained: Option<Rc<RefCell<Retained>>>, _| {
                if let Some(retained) = retained
                    && retained.borrow().key == key
                {
                    return (retained.clone(), retained);
                }
                let handle = TextSelectionHandle::new(fallback, cx);
                let copy = Rc::new(RefCell::new(String::new()));
                let copy_for_callback = copy.clone();
                handle.copy_with(move |_cx| copy_for_callback.borrow().clone(), cx);
                handle
                    .subscribe(move |_event, cx| cx.notify(view), cx)
                    .detach();
                let retained = Rc::new(RefCell::new(Retained {
                    key,
                    handle,
                    styled,
                    copy,
                }));
                (retained.clone(), retained)
            },
        );
        let layout_id = retained
            .borrow_mut()
            .styled
            .request_layout(global_id, inspector_id, window, cx)
            .0;
        (layout_id, retained)
    }

    fn prepaint(
        &mut self,
        global_id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        retained: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let mut state = retained.borrow_mut();
        state
            .styled
            .prepaint(global_id, inspector_id, bounds, &mut (), window, cx);
        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        let registration = TextSelectionRegistration::new(hitbox.clone(), bounds)
            .with_text_bounds(vec![bounds])
            .with_rendered_element(&state.handle, window, cx);
        state.handle.register(registration, window, cx);
        hitbox
    }

    fn paint(
        &mut self,
        global_id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        retained: &mut Self::RequestLayoutState,
        _: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let mut state = retained.borrow_mut();
        let layout = state.styled.layout().clone();
        let color: Hsla = palette::selection(&palette::current_ramp().accent_colors()).into();
        let snapshot = state.handle.snapshot(cx);
        if snapshot.is_none() {
            // No selection: refresh the run so the next drag hit-tests the
            // current bounds. With no selection the projection is trivial.
            state.handle.update_runs(
                &[TextSelectionRun::new(
                    self.text.clone(),
                    layout.clone(),
                    bounds,
                )],
                cx,
            );
        }
        let mut copy = String::new();
        if let Some(points) = snapshot.and_then(|snapshot| snapshot.window_points()) {
            let anchor = index_at(&layout, points.anchor());
            let cursor = index_at(&layout, points.cursor());
            let (start, end) = if anchor <= cursor {
                (anchor, cursor)
            } else {
                (cursor, anchor)
            };
            let start = start.min(self.text.len());
            let end = end.min(self.text.len());
            if start < end && self.text.is_char_boundary(start) && self.text.is_char_boundary(end) {
                Self::paint_selection(&layout, start..end, color, window);
                copy.push_str(&self.text[start..end]);
            }
        }
        *state.copy.borrow_mut() = copy;
        state.styled.paint(
            global_id,
            inspector_id,
            bounds,
            &mut (),
            &mut (),
            window,
            cx,
        );
    }
}
