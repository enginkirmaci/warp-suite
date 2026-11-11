//! A wheel over a scroller nested in another moves the inner one first; the
//! outer one only gets a step once the inner is hard against the edge in that
//! direction. `fold_body` is the suite's one nested scroller (a fold inside the
//! chat transcript's list), so this probe hosts it in a plain outer scroller
//! and checks both sides of the chain.

use gpui::{
    Context, Entity, IntoElement, ListAlignment, ListState, Modifiers, Render, ScrollDelta,
    ScrollEvent, ScrollHandle, ScrollSource, TestAppContext, TouchPhase, VisualTestContext, Window,
    div, list, point, prelude::*, px,
};

/// Taller than the fold's 260 cap, so the fold always has room to scroll.
const FOLD_CONTENT: f32 = 900.;

struct Nested {
    outer: ScrollHandle,
    fold: ScrollHandle,
}

impl Render for Nested {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .id("outer-scroll")
            .overflow_y_scroll()
            .track_scroll(&self.outer)
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .child(div().h(px(180.)).child("above the fold"))
                    .child(
                        warpui_kit_core::fold_body("nested-fold", &self.fold)
                            .debug_selector(|| "nested-fold".to_string())
                            // the probe's outer column sizes to the window;
                            // without this the fold would shrink below its cap
                            .flex_shrink_0()
                            .child(div().h(px(FOLD_CONTENT)).child("fold content")),
                    )
                    .child(
                        div()
                            .debug_selector(|| "below-fold".to_string())
                            .h(px(2400.))
                            .child("below the fold"),
                    ),
            )
    }
}

fn open(cx: &mut TestAppContext) -> (Entity<Nested>, &mut VisualTestContext) {
    cx.update(gpui_kit::init);
    let (root, vcx) = cx.add_window_view(|_, _| Nested {
        outer: ScrollHandle::default(),
        fold: ScrollHandle::default(),
    });
    vcx.run_until_parked();
    vcx.update(|window, cx| drop(window.draw(cx)));
    (root, vcx)
}

fn handles(root: &Entity<Nested>, vcx: &mut VisualTestContext) -> (ScrollHandle, ScrollHandle) {
    vcx.update(|_, cx| {
        let nested = root.read(cx);
        (nested.outer.clone(), nested.fold.clone())
    })
}

fn wheel(vcx: &mut VisualTestContext, at: gpui::Point<gpui::Pixels>, dy: f32) {
    vcx.simulate_event(ScrollEvent {
        position: at,
        delta: ScrollDelta::Pixels(point(px(0.), px(dy))),
        modifiers: Modifiers::default(),
        touch_phase: TouchPhase::Moved,
        source: ScrollSource::Pointer,
    });
    vcx.update(|window, cx| drop(window.draw(cx)));
}

fn center(vcx: &mut VisualTestContext, selector: &'static str) -> gpui::Point<gpui::Pixels> {
    vcx.debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} was not drawn"))
        .center()
}

#[gpui_kit::gpui::test]
fn the_fold_takes_the_wheel_and_the_transcript_only_moves_at_its_edge(cx: &mut TestAppContext) {
    let (root, vcx) = open(cx);
    let (outer, fold) = handles(&root, vcx);
    let at = center(vcx, "nested-fold");

    // inside the fold, the fold moves and the transcript stays put
    for _ in 0..3 {
        wheel(vcx, at, -120.);
    }
    assert!(
        fold.offset().y < px(0.),
        "the fold scrolled: {:?}",
        fold.offset()
    );
    assert_eq!(
        outer.offset().y,
        px(0.),
        "the transcript must not move while the fold has room"
    );

    // drive the fold to its bottom; the step that lands it there is still the
    // fold's, so the transcript is untouched
    wheel(vcx, at, -200.);
    wheel(vcx, at, -200.);
    assert_eq!(
        fold.offset().y,
        -fold.max_offset().y,
        "the fold sits at its bottom"
    );
    assert_eq!(
        outer.offset().y,
        px(0.),
        "reaching the fold's edge does not move the transcript by itself"
    );

    // one more step at the edge chains to the transcript
    wheel(vcx, at, -120.);
    assert!(
        outer.offset().y < px(0.),
        "the step past the fold's bottom chains outward: {:?}",
        outer.offset()
    );

    // scrolling back up belongs to the fold again until it reaches its top
    let outer_before = outer.offset().y;
    wheel(vcx, at, 120.);
    assert!(
        fold.offset().y > -fold.max_offset().y,
        "the fold moved back up: {:?}",
        fold.offset()
    );
    assert_eq!(
        outer.offset().y,
        outer_before,
        "the transcript stays put while the fold scrolls up"
    );
}

#[gpui_kit::gpui::test]
fn a_wheel_outside_the_fold_moves_the_transcript(cx: &mut TestAppContext) {
    let (root, vcx) = open(cx);
    let (outer, fold) = handles(&root, vcx);
    // inside the window but below the fold's 260-high box
    let at = gpui::point(px(400.), px(560.));

    wheel(vcx, at, -120.);
    assert!(
        outer.offset().y < px(0.),
        "a wheel over the page scrolls it: {:?}",
        outer.offset()
    );
    assert_eq!(fold.offset().y, px(0.), "the fold was not under the wheel");
}

/// The real shape: the fold sits in a `gpui::list` row (the chat transcript),
/// not in a plain scroller.
struct ListNested {
    state: ListState,
    fold: ScrollHandle,
}

impl Render for ListNested {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let fold = self.fold.clone();
        div().size_full().child(
            list(self.state.clone(), move |ix, _, _| match ix {
                0 => div().h(px(180.)).child("above the fold").into_any_element(),
                1 => warpui_kit_core::fold_body("list-fold", &fold)
                    .debug_selector(|| "list-fold".to_string())
                    .flex_shrink_0()
                    .child(div().h(px(FOLD_CONTENT)).child("fold content"))
                    .into_any_element(),
                _ => div().h(px(400.)).child("row").into_any_element(),
            })
            .size_full(),
        )
    }
}

#[gpui_kit::gpui::test]
fn a_fold_in_the_transcript_list_takes_the_wheel_before_the_list(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let state = ListState::new(4, ListAlignment::Top, px(400.));
    let (root, vcx) = cx.add_window_view({
        let state = state.clone();
        move |_, _| ListNested {
            state,
            fold: ScrollHandle::default(),
        }
    });
    vcx.run_until_parked();
    vcx.update(|window, cx| drop(window.draw(cx)));
    let fold = vcx.update(|_, cx| root.read(cx).fold.clone());
    let at = center(vcx, "list-fold");

    wheel(vcx, at, -120.);
    assert!(
        fold.offset().y < px(0.),
        "the fold scrolled: {:?}",
        fold.offset()
    );
    let top = state.logical_scroll_top();
    assert_eq!(
        (top.item_ix, top.offset_in_item),
        (0, px(0.)),
        "the transcript list must not move while the fold has room"
    );

    // enough steps to land the fold on its bottom edge, then one to chain
    for _ in 0..3 {
        wheel(vcx, at, -200.);
    }
    let top = state.logical_scroll_top();
    assert_eq!(
        (top.item_ix, top.offset_in_item),
        (0, px(0.)),
        "landing on the fold's edge is still the fold's step"
    );
    wheel(vcx, at, -120.);
    let top = state.logical_scroll_top();
    assert!(
        top.item_ix > 0 || top.offset_in_item > px(0.),
        "the step past the fold's edge chains to the transcript: {top:?}"
    );
}
