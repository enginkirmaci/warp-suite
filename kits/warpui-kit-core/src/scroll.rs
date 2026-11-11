//! Wheels in nested scrollers.
//!
//! GPUI hands one wheel event to every scrollable hitbox under the pointer, so
//! an inner scroller and the page around it both move on the same tick: the
//! outer one "engages" while the inner one still has content to show. A nested
//! scroller should take the step itself and only let it pass when it is hard
//! against the edge in the wheel's direction — the way a browser chains a
//! wheel from an inner box out to the page.
//!
//! The check runs from the nested scroller's own `.on_scroll` handler. An
//! element's `.on_scroll` listeners are registered before its built-in scroll
//! listener, so in the bubble phase the built-in one runs first and has
//! already applied the step by the time the handler sees the handle.
//! [`consume_wheel`] therefore reconstructs the offset the step started from
//! and tests that edge, not the post-step one: a scroller one notch away from
//! its end takes that notch and still stops the outer one.

use gpui::{App, Pixels, Point, ScrollEvent, ScrollHandle, Window, point, px};

/// True when a vertical scroller at `offset` (≤ 0, clamped to `max_offset`)
/// has room to move for `delta` — the "the inner one handled it" test. A
/// wheel event routed to an inner scrollable must not also scroll an outer
/// one, but a scroller already hard against the edge in the wheel's
/// direction lets the event pass, as browsers chain it.
pub fn wheel_reaches_content(
    offset: Point<Pixels>,
    max_offset: Point<Pixels>,
    delta: Pixels,
) -> bool {
    // compare the raw floats: `Pixels` orders with `total_cmp`, which puts
    // -0.0 before 0.0, so a scroller at rest (offset 0, max 0) would read as
    // having room toward the bottom once the offset is negated
    let (offset, max, delta) = (offset.y.as_f32(), max_offset.y.as_f32(), delta.as_f32());
    if delta < 0.0 {
        // scrolling toward the bottom: room is whatever is left below
        -offset < max
    } else {
        // scrolling back toward the top: any offset off the top moves
        offset < 0.0
    }
}

/// The offset `handle` had before a step of `delta`: the built-in scroll
/// listener has already applied the step when a nested scroller's
/// `.on_scroll` handler runs, and the edge test is about where the step
/// started.
pub fn offset_before_wheel(handle: &ScrollHandle, delta: Pixels) -> Point<Pixels> {
    let max = handle.max_offset();
    point(px(0.), (handle.offset().y - delta).clamp(-max.y, px(0.)))
}

/// Consumes a wheel step for a nested scroller: stops propagation while the
/// inner one has room to move, so the outer one stays put; at the edge the
/// step is left to bubble and chains outward. A horizontal-only step is never
/// consumed: it is not this scroller's to take.
///
/// Call it from the scrollable element's own `.on_scroll`, with the same
/// element tracked by `handle`.
pub fn consume_wheel(handle: &ScrollHandle, event: &ScrollEvent, window: &Window, cx: &mut App) {
    let delta = event.delta.pixel_delta(window.line_height()).y;
    consume_wheel_by(handle, delta, cx);
}

/// [`consume_wheel`] for a scrollable whose built-in listener turns lines into
/// pixels with a different line height (a `gpui::list` uses 20). Pass the same
/// pixel delta that listener applied.
pub fn consume_wheel_by(handle: &ScrollHandle, delta: Pixels, cx: &mut App) {
    if delta == px(0.) {
        return;
    }
    if wheel_reaches_content(
        offset_before_wheel(handle, delta),
        handle.max_offset(),
        delta,
    ) {
        cx.stop_propagation();
    }
}

#[cfg(test)]
mod tests {
    use super::wheel_reaches_content;
    use gpui::{point, px};

    #[test]
    fn a_scrolled_inner_scroller_takes_the_step_either_way() {
        let max = point(px(0.), px(500.));
        assert!(wheel_reaches_content(
            point(px(0.), px(-120.)),
            max,
            px(-50.)
        ));
        assert!(wheel_reaches_content(
            point(px(0.), px(-120.)),
            max,
            px(50.)
        ));
    }

    #[test]
    fn an_edge_scroller_lets_the_step_chain() {
        let max = point(px(0.), px(500.));
        let top = point(px(0.), px(0.));
        let bottom = point(px(0.), px(-500.));
        assert!(!wheel_reaches_content(top, max, px(50.)));
        assert!(!wheel_reaches_content(bottom, max, px(-50.)));
        assert!(wheel_reaches_content(top, max, px(-50.)));
        assert!(wheel_reaches_content(bottom, max, px(50.)));
    }

    #[test]
    fn a_fitting_scroller_never_takes_the_step() {
        let zero = point(px(0.), px(0.));
        assert!(!wheel_reaches_content(zero, zero, px(-50.)));
        assert!(!wheel_reaches_content(zero, zero, px(50.)));
    }
}
