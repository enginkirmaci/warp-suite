//! Headless probes for the shared picker shell: the field mode renders rows,
//! a row click lands the pick in the target and closes the popup; the
//! composer overlay renders its catcher and routes pick + dismissal through
//! the owner's closures.

use std::sync::Arc;

use gpui::prelude::*;
use gpui::{App, Entity, Modifiers, TestAppContext, VisualTestContext, Window, div, point, px};
use gpui_kit::component::input::InputState;
use warpui_kit_core::palette;
use warpui_kit_core::{FontTokens, font_tokens};
use warpui_kit_picker::{
    DismissFn, FilteredPicker, OverlayAnchor, PanelChrome, PickFn, PickerBody, PickerData,
    PickerIcons, PickerItem, PickerRow, PickerSpec, PickerTarget, Selection, TriggerScale,
};

const ICONS: PickerIcons = PickerIcons {
    check: "icons/check.svg",
    chevron_down: "icons/chevron.svg",
    trash: "icons/trash.svg",
    refresh: "icons/refresh.svg",
};

const MODELS: [&str; 3] = ["anthropic/claude", "openai/gpt", "zeta/mini"];

fn spec(overlay: bool, anchor: OverlayAnchor) -> PickerSpec {
    PickerSpec {
        key: "test",
        trigger_label: "Test picker",
        filter_placeholder: "Filter",
        icons: ICONS,
        trigger_icon: None,
        provider_badge: true,
        refresh: false,
        refresh_label: "Refresh",
        stop_trigger_mousedown: false,
        visible_rows: 5,
        selected_accent: false,
        overlay,
        anchor,
        trigger_scale: TriggerScale::Compact,
        trigger_rounded_lg: false,
        trigger_min_h: None,
        panel_chrome: PanelChrome::Menus,
        trigger_id: None,
        panel_id: None,
        filter_selector: None,
        list_id: None,
    }
}

fn data(_filter: &str, _window: &Window, _cx: &App) -> PickerData {
    let fonts: FontTokens = font_tokens(None, 1.0);
    PickerData {
        body: PickerBody::Rows(
            MODELS
                .iter()
                .map(|model| {
                    PickerRow::Item(PickerItem {
                        id: format!("row-{model}"),
                        selector: format!("row-{}", model.replace('/', "-")),
                        value: model.to_string(),
                        label: model.to_string(),
                        selected: false,
                        muted: false,
                        trailing_delete: None,
                    })
                })
                .collect(),
        ),
        selected: Selection {
            value: String::new(),
            label: "Pick".into(),
            badge: None,
        },
        footer: None,
        fonts,
        accent: palette::accent(),
        show_tooltips: false,
    }
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} must be on screen"));
    cx.simulate_click(
        point(bounds.center().x, bounds.center().y),
        Modifiers::default(),
    );
}

struct Host {
    field: Entity<InputState>,
    picker: Entity<FilteredPicker>,
}

impl Render for Host {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        // a fixed-size stage so the composer catcher's inset_0 has a box to
        // cover
        div()
            .w(px(800.))
            .h(px(600.))
            .relative()
            .flex()
            .flex_col()
            .child(self.picker.clone())
            // a form row under the slot: a floating panel must not move it
            .child(
                div()
                    .id("form-row-after")
                    .debug_selector(|| "form-row-after".into())
                    .h(px(60.))
                    .w_full(),
            )
    }
}

#[gpui_kit::gpui::test]
fn field_pick_writes_the_target_and_closes(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (handle, cx) = cx.add_window_view(|window, cx| {
        let field = cx.new(|cx| InputState::new(window, cx).placeholder("target"));
        let picker = cx.new(|cx| {
            let mut picker = FilteredPicker::new(
                spec(false, OverlayAnchor::Rail),
                PickerTarget::Field(field.clone()),
                Box::new(data),
                window,
                cx,
            );
            picker.observe_entity(&field, cx);
            picker
        });
        picker.update(cx, |p, cx| p.observe_entity(&field, cx));
        Host { field, picker }
    });
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    click(cx, "model-trigger-test");
    cx.update(|window, cx| drop(window.draw(cx)));
    assert!(
        cx.debug_bounds("model-panel-test").is_some(),
        "the popup opens under the trigger"
    );

    click(cx, "row-openai-gpt");
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    assert!(
        cx.debug_bounds("model-panel-test").is_none(),
        "a pick closes the popup"
    );
    let value = handle.update(cx, |host, cx| {
        host.field.read(cx).value().trim().to_string()
    });
    assert_eq!(value, "openai/gpt", "the pick lands in the target field");
}

#[gpui_kit::gpui::test]
fn composer_overlay_routes_pick_and_dismiss(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let picks: Arc<std::sync::Mutex<Vec<String>>> = Arc::default();
    let dismissals: Arc<std::sync::Mutex<usize>> = Arc::default();

    let (_handle, cx) = cx.add_window_view(|window, cx| {
        let pick: PickFn = {
            let picks = picks.clone();
            Box::new(move |value, _window, _cx| {
                picks.lock().unwrap().push(value.to_string());
            })
        };
        let close: DismissFn = {
            let dismissals = dismissals.clone();
            Box::new(move |_cx: &mut App| {
                *dismissals.lock().unwrap() += 1;
            })
        };
        let picker = cx.new(|cx| {
            let mut picker = FilteredPicker::new(
                spec(true, OverlayAnchor::Composer),
                PickerTarget::Callback(pick),
                Box::new(data),
                window,
                cx,
            );
            picker.set_on_dismiss(close);
            picker
        });
        Host {
            field: cx.new(|cx| InputState::new(window, cx).placeholder("unused")),
            picker,
        }
    });
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    assert!(
        cx.debug_bounds("picker-catcher").is_some(),
        "the catcher covers the window"
    );
    assert!(
        cx.debug_bounds("model-panel-test").is_some(),
        "the anchored card shows the rows"
    );

    click(cx, "row-zeta-mini");
    cx.run_until_parked();
    assert_eq!(
        picks.lock().unwrap().as_slice(),
        ["zeta/mini"],
        "the pick routes through the owner's closure"
    );
    assert_eq!(
        *dismissals.lock().unwrap(),
        1,
        "a pick dismisses through the owner"
    );
}

/// A catcher click (no row) dismisses through the owner without a pick. One
/// entity per scenario: a real host unmounts the picker once it closes.
#[gpui_kit::gpui::test]
fn composer_catcher_click_dismisses_without_pick(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let picks: Arc<std::sync::Mutex<Vec<String>>> = Arc::default();
    let dismissals: Arc<std::sync::Mutex<usize>> = Arc::default();

    let (_handle, cx) = cx.add_window_view(|window, cx| {
        let pick: PickFn = {
            let picks = picks.clone();
            Box::new(move |value, _window, _cx| {
                picks.lock().unwrap().push(value.to_string());
            })
        };
        let close: DismissFn = {
            let dismissals = dismissals.clone();
            Box::new(move |_cx: &mut App| {
                *dismissals.lock().unwrap() += 1;
            })
        };
        let picker = cx.new(|cx| {
            let mut picker = FilteredPicker::new(
                spec(true, OverlayAnchor::Composer),
                PickerTarget::Callback(pick),
                Box::new(data),
                window,
                cx,
            );
            picker.set_on_dismiss(close);
            picker
        });
        Host {
            field: cx.new(|cx| InputState::new(window, cx).placeholder("unused")),
            picker,
        }
    });
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    click(cx, "picker-catcher");
    cx.run_until_parked();
    assert!(
        picks.lock().unwrap().is_empty(),
        "a catcher click is not a pick"
    );
    assert_eq!(*dismissals.lock().unwrap(), 1, "a catcher click dismisses");
}

/// A settings slot floats: the panel must be drawn outside the trigger's own
/// box, so the form under it is covered rather than pushed down. The host
/// below puts a fixed-height form row after the picker; if the panel were
/// in-flow, that row would move.
#[gpui_kit::gpui::test]
fn field_anchor_floats_the_panel_over_the_form(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (_handle, cx) = cx.add_window_view(|window, cx| {
        let field = cx.new(|cx| InputState::new(window, cx).placeholder("target"));
        let picker = cx.new(|cx| {
            FilteredPicker::new(
                spec(true, OverlayAnchor::Field),
                PickerTarget::Field(field.clone()),
                Box::new(data),
                window,
                cx,
            )
        });
        Host { field, picker }
    });
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    let trigger = cx
        .debug_bounds("model-trigger-test")
        .expect("the trigger renders in the slot");
    let row_before = cx
        .debug_bounds("form-row-after")
        .expect("the form row after the slot renders");
    click(cx, "model-trigger-test");
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    let panel = cx
        .debug_bounds("model-panel-test")
        .expect("the floating panel opens");
    assert!(
        panel.origin.y >= trigger.origin.y + trigger.size.height,
        "the panel opens below the trigger (panel {panel:?}, trigger {trigger:?})"
    );
    assert!(
        panel.origin.y < trigger.origin.y + trigger.size.height + px(24.),
        "the panel sits just under the trigger, not at the stage's end (panel {panel:?}, trigger {trigger:?})"
    );
    // the row after the picker keeps its place: a floating panel costs no
    // layout height, which is the bug the screenshot showed
    let row_after = cx
        .debug_bounds("form-row-after")
        .expect("the form row after the slot still renders");
    assert_eq!(
        row_before.origin.y, row_after.origin.y,
        "an open floating panel must not push the form down"
    );
}

/// The composer card opens against its own pill when the host reports the
/// trigger: left-aligned with it and just above its top, never at the
/// host's bottom-right corner (the bug the chat tile showed).
#[gpui_kit::gpui::test]
fn composer_anchor_sits_above_its_trigger(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut picker_slot: Option<Entity<FilteredPicker>> = None;
    let (_handle, cx) = cx.add_window_view(|window, cx| {
        let picker = cx.new(|cx| {
            FilteredPicker::new(
                spec(true, OverlayAnchor::Composer),
                PickerTarget::Callback(Box::new(|_, _, _| {})),
                Box::new(data),
                window,
                cx,
            )
        });
        picker_slot = Some(picker.clone());
        Host {
            field: cx.new(|cx| InputState::new(window, cx).placeholder("unused")),
            picker,
        }
    });
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    let trigger = gpui::Bounds {
        origin: point(px(120.), px(420.)),
        size: gpui::size(px(140.), px(28.)),
    };
    let host = gpui::Bounds {
        origin: point(px(0.), px(0.)),
        size: gpui::size(px(800.), px(600.)),
    };
    let picker = picker_slot.expect("the host built the picker");
    picker.update(cx, |p, cx| p.set_composer_anchor(trigger, host, cx));
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    let panel = cx
        .debug_bounds("model-panel-test")
        .expect("the card renders over the catcher");
    assert!(
        (panel.origin.x - trigger.origin.x).abs() <= px(2.),
        "the card is left-aligned with its pill (panel {panel:?}, trigger {trigger:?})"
    );
    let panel_bottom = panel.origin.y + panel.size.height;
    let wanted_bottom = trigger.origin.y - px(8.);
    assert!(
        (panel_bottom - wanted_bottom).abs() <= px(2.),
        "the card sits 8px above the pill's top (panel bottom {panel_bottom:?}, wanted {wanted_bottom:?})"
    );
}

/// A host mounts the composer picker only while its pill is open, so the
/// entity outlives a dismissal. Re-opening must re-arm click-elsewhere:
/// without `set_open(true)` the stale closed flag made the catcher a no-op.
#[gpui_kit::gpui::test]
fn composer_dismissal_rearms_after_reopen(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let dismissals: Arc<std::sync::Mutex<usize>> = Arc::default();
    let mut picker_slot: Option<Entity<FilteredPicker>> = None;
    let (_handle, cx) = cx.add_window_view(|window, cx| {
        let close: DismissFn = {
            let dismissals = dismissals.clone();
            Box::new(move |_cx: &mut App| {
                *dismissals.lock().unwrap() += 1;
            })
        };
        let picker = cx.new(|cx| {
            let mut picker = FilteredPicker::new(
                spec(true, OverlayAnchor::Composer),
                PickerTarget::Callback(Box::new(|_, _, _| {})),
                Box::new(data),
                window,
                cx,
            );
            picker.set_on_dismiss(close);
            picker
        });
        picker_slot = Some(picker.clone());
        Host {
            field: cx.new(|cx| InputState::new(window, cx).placeholder("unused")),
            picker,
        }
    });
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    click(cx, "picker-catcher");
    cx.run_until_parked();
    assert_eq!(
        *dismissals.lock().unwrap(),
        1,
        "the first click-elsewhere dismisses"
    );

    // the host unmounted the entity and mounts it again on the next open
    let picker = picker_slot.expect("the host built the picker");
    cx.update(|window, cx| {
        picker.update(cx, |p, cx| {
            p.set_open(true, window, cx);
        });
    });
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    click(cx, "picker-catcher");
    cx.run_until_parked();
    assert_eq!(
        *dismissals.lock().unwrap(),
        2,
        "a reopened picker dismisses again on click-elsewhere"
    );
}

/// Escape closes the composer popup from the focused filter: the key event
/// bubbles through the catcher, so the owner's dismissal runs even though the
/// card itself never takes focus.
#[gpui_kit::gpui::test]
fn composer_escape_dismisses(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let dismissals: Arc<std::sync::Mutex<usize>> = Arc::default();
    let mut picker_slot: Option<Entity<FilteredPicker>> = None;
    let (_handle, cx) = cx.add_window_view(|window, cx| {
        let close: DismissFn = {
            let dismissals = dismissals.clone();
            Box::new(move |_cx: &mut App| {
                *dismissals.lock().unwrap() += 1;
            })
        };
        let picker = cx.new(|cx| {
            let mut picker = FilteredPicker::new(
                spec(true, OverlayAnchor::Composer),
                PickerTarget::Callback(Box::new(|_, _, _| {})),
                Box::new(data),
                window,
                cx,
            );
            picker.set_on_dismiss(close);
            picker
        });
        picker_slot = Some(picker.clone());
        Host {
            field: cx.new(|cx| InputState::new(window, cx).placeholder("unused")),
            picker,
        }
    });
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    // the host focuses the filter on open (FilteredPicker::reset_filter)
    let picker = picker_slot.expect("the host built the picker");
    cx.update(|window, cx| {
        picker.update(cx, |p, cx| p.reset_filter(window, cx));
    });
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert_eq!(
        *dismissals.lock().unwrap(),
        1,
        "Escape dismisses the composer popup through the owner"
    );
}
