//! warpui-kit-picker: the filtered select-style picker shared by the family.
//!
//! A standalone entity so filter typing repaints only the picker, never the
//! page around it. The shell — trigger, filter input, virtualized rows,
//! selection/dismiss wiring, a11y — lives here; the data stays with the app:
//! a source closure is re-read on every render with the current filter
//! string, and the app observes whatever entities feed it
//! ([`FilteredPicker::observe_entity`]).
//!
//! Three mount shapes, driven by [`PickerSpec::overlay`]:
//!
//! - **Field** (settings slots): the picker owns a trigger; a pick writes the
//!   value straight into an [`InputState`] ([`PickerTarget::Field`]) or lands
//!   in a callback ([`PickerTarget::Callback`]). The popup is an in-flow
//!   child below the trigger.
//! - **Rail overlay** (`overlay` + `Rail`): the trigger renders; the host
//!   appends [`FilteredPicker::overlay_popup`] as its LAST child so it paints
//!   over the rail's actions without gpui::deferred, whose hitboxes misplace
//!   nested interactive elements.
//! - **Composer overlay** (`overlay` + `Composer`): no trigger; the host
//!   mounts the picker element only while its pill is open. A full-bleed
//!   catcher dismisses on click-elsewhere and picks/dismissal land through
//!   the [`PickerTarget::Callback`] and `on_dismiss` closures that hold the
//!   owner only weakly.

use gpui::prelude::*;
use gpui::{
    App, Bounds, Context, Entity, Focusable as _, FontWeight, KeyDownEvent, MouseButton, Pixels,
    SharedString, Window, anchored, deferred, div, px, uniform_list,
};
use gpui_kit::base::ElementExt as _;
use gpui_kit::component::input::InputState;
use std::cell::Cell;
use std::rc::Rc;
use warpui_kit_core::palette;
use warpui_kit_core::{A11y as _, FontTokens};

const ROW_HEIGHT: f32 = 28.;
/// Composer-overlay card width.
const OVERLAY_WIDTH: f32 = 280.;
/// The floating field panel's narrowest width: enough for a long model name
/// beside its provider group header.
const FIELD_PANEL_MIN_WIDTH: f32 = 260.;

/// A composer-overlay card's placement for one trigger, in the catcher's
/// coordinates: left-aligned with the trigger and just above its top, never
/// past the host's right edge. Both rects are window bounds (prepaint's);
/// the pair is `(left, distance from the host's bottom to the card's
/// bottom)`. Apps that draw their own composer catcher (a flat list, not a
/// [`FilteredPicker`]) use this to sit on the same geometry.
pub fn composer_placement(trigger: Bounds<Pixels>, host: Bounds<Pixels>) -> (Pixels, Pixels) {
    const GAP: f32 = 8.;
    const EDGE: f32 = 8.;
    let left = (trigger.origin.x - host.origin.x).max(px(EDGE));
    let widest = (host.size.width - px(OVERLAY_WIDTH + EDGE)).max(px(EDGE));
    let bottom = host.origin.y + host.size.height - trigger.origin.y + px(GAP);
    (left.min(widest), bottom)
}

/// The filter field's glyph, embedded as source: this crate ships no assets
/// and cannot rely on the app embedding a `search` icon (warp-station names
/// its icons `icon-*.svg`, so a path here used to render nothing, silently).
/// Same strategy as kit-markdown's editor glyphs; the art is the family
/// design source's `search` (Lucide, stroke 1.9).
const SEARCH: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><circle cx="11" cy="11" r="8"/><path d="m21 21-4.3-4.3"/></svg>"##;

// ---- data model ------------------------------------------------------------

/// The asset paths the shell paints. This crate ships no assets — callers
/// pass their own.
#[derive(Clone, Copy)]
pub struct PickerIcons {
    pub check: &'static str,
    pub chevron_down: &'static str,
    pub trash: &'static str,
    pub refresh: &'static str,
}

/// The trigger's current selection: the payload, its display label and an
/// optional leading badge (the model provider).
#[derive(Clone, Default)]
pub struct Selection {
    pub value: String,
    pub label: String,
    pub badge: Option<String>,
}

/// The armed trailing delete action on a row (branch mode). The arm/confirm
/// cycle is app state — the shell only renders the flag and reports clicks.
#[derive(Clone)]
pub struct TrailingDelete {
    pub id: String,
    pub selector: String,
    pub armed: bool,
    /// Tooltip while unarmed; the armed state appends its own warning.
    pub tooltip: &'static str,
}

/// One selectable row.
#[derive(Clone)]
pub struct PickerItem {
    /// Element identity.
    pub id: String,
    /// Debug selector used by the headless probes.
    pub selector: String,
    /// Model id or branch name: the pick payload.
    pub value: String,
    /// Display label; shed its head before handing over if it would overflow.
    pub label: String,
    pub selected: bool,
    /// A remote-tracking row (faint).
    pub muted: bool,
    /// The armed trash action, pinned to the right edge.
    pub trailing_delete: Option<TrailingDelete>,
}

/// Group headers are inert labels between rows.
#[derive(Clone)]
pub enum PickerRow {
    Group {
        id: String,
        label: String,
        selector: Option<String>,
    },
    Item(PickerItem),
    Message {
        id: String,
        label: String,
    },
}

/// What the panel shows: the filtered rows, or a full-body replacement (the
/// new-branch form hides the list entirely).
pub enum PickerBody {
    Rows(Vec<PickerRow>),
    Replace(gpui::AnyElement),
}

/// One render's worth of picker state, produced by the app's source closure.
pub struct PickerData {
    pub body: PickerBody,
    pub selected: Selection,
    /// App-built element under the list (the "+ New branch…" row or the open
    /// form). Only shown when the body is `Rows`.
    pub footer: Option<gpui::AnyElement>,
    pub fonts: FontTokens,
    pub accent: gpui::Rgba,
    pub show_tooltips: bool,
    /// True while the picker's refresh action is in flight. The refresh
    /// button spins and ignores further clicks until the source clears it.
    pub refreshing: bool,
}

/// Re-read on every render with the current filter string. The window rides
/// along for width-aware label shaping (head-trimmed branch pills).
pub type DataSource = Box<dyn Fn(&str, &Window, &App) -> PickerData>;

/// Where a pick lands.
pub enum PickerTarget {
    /// The trigger writes the pick straight into the input and closes.
    Field(Entity<InputState>),
    /// Selection goes through the owner (branch checkout, composer tile).
    Callback(PickFn),
}

/// A pick: the payload goes through the owner.
pub type PickFn = Box<dyn Fn(&str, &mut Window, &mut App)>;
/// Overlay dismissal through the owner.
pub type DismissFn = Box<dyn Fn(&mut App)>;
/// The optional refresh button's action.
pub type RefreshFn = Box<dyn Fn(&mut App)>;
/// A trailing (delete) click; the app arms and confirms. Returns `true` when
/// the action completed (the confirm click) — the shell then dismisses the
/// popup itself, so the app never re-enters the picker from its own listener.
pub type TrailingFn = Box<dyn Fn(&str, &mut App) -> bool>;

/// The `provider/name` split of a catalog model id (`""` when unsplit).
pub fn model_provider(model: &str) -> &str {
    model
        .split_once('/')
        .map(|(provider, _)| provider)
        .unwrap_or("")
}

/// The name half of a catalog model id (the whole id when unsplit).
pub fn model_name(model: &str) -> &str {
    model.split_once('/').map(|(_, name)| name).unwrap_or(model)
}

// ---- spec ------------------------------------------------------------------

/// Trigger text scale: the settings-slot look (small label, small badge) or
/// the composer look (body label, micro badge).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TriggerScale {
    Compact,
    Regular,
}

/// Panel chrome: the tight menu popup or the raised popover card.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PanelChrome {
    Menus,
    Cards,
}

/// Where an overlay popup anchors.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum OverlayAnchor {
    /// Host-mounted over a rail: covers the trigger + actions row without
    /// reflowing.
    Rail,
    /// Self-rendered catcher with the card anchored to the composer's corner.
    Composer,
    /// A settings slot: the trigger stays in the form, the panel floats in the
    /// window layer below it, so a long list covers the rows underneath
    /// instead of pushing them down. The panel measures the trigger, so a slot
    /// of any height opens at its own edge.
    Field,
}

/// The look, layout and id choices a picker instance is built with.
#[derive(Clone, Copy)]
pub struct PickerSpec {
    pub key: &'static str,
    /// The trigger's a11y label (its aria-label).
    pub trigger_label: &'static str,
    pub filter_placeholder: &'static str,
    pub icons: PickerIcons,
    pub trigger_icon: Option<&'static str>,
    /// Show the selection badge in the trigger.
    pub provider_badge: bool,
    /// A refresh button beside the trigger.
    pub refresh: bool,
    /// The refresh button's a11y label.
    pub refresh_label: &'static str,
    /// Stop mousedown propagation on the trigger (the window's outside-click
    /// dismissal would otherwise eat the toggle click).
    pub stop_trigger_mousedown: bool,
    pub visible_rows: usize,
    /// Selected row paints in the accent (branch mode) instead of a wash.
    pub selected_accent: bool,
    /// The popup is not rendered in-flow; see [`OverlayAnchor`].
    pub overlay: bool,
    pub anchor: OverlayAnchor,
    pub trigger_scale: TriggerScale,
    pub trigger_rounded_lg: bool,
    pub trigger_min_h: Option<f32>,
    pub panel_chrome: PanelChrome,
    /// Element id overrides for the headless probes; the defaults derive
    /// from `key`.
    pub trigger_id: Option<&'static str>,
    pub panel_id: Option<&'static str>,
    pub filter_selector: Option<&'static str>,
    pub list_id: Option<&'static str>,
}

// ---- the picker ------------------------------------------------------------

pub struct FilteredPicker {
    spec: PickerSpec,
    target: PickerTarget,
    data: DataSource,
    on_dismiss: Option<DismissFn>,
    on_refresh: Option<RefreshFn>,
    on_trailing: Option<TrailingFn>,
    filter: Entity<InputState>,
    open: bool,
    /// The host switched this slot off (a value an adjacent toggle owns): the
    /// trigger keeps its shape but dims, and a click does not open the list
    /// until the host switches it back on.
    disabled: bool,
    /// The field slot's trigger bounds, written from prepaint and read when
    /// the floating panel opens. It lives on the entity because a fresh
    /// `Cell` per render would always be empty.
    trigger_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// The composer overlay's card placement, in the catcher's coordinates:
    /// the card's left edge and the distance from the host's bottom to the
    /// card's bottom edge. `None` keeps the corner default.
    composer_placement: Rc<Cell<Option<(Pixels, Pixels)>>>,
    _subscriptions: Vec<gpui::Subscription>,
}

impl FilteredPicker {
    pub fn new(
        spec: PickerSpec,
        target: PickerTarget,
        data: DataSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder(spec.filter_placeholder));
        let sub_filter = cx.observe(&filter, |_, _, cx| cx.notify());
        let open = spec.overlay && spec.anchor == OverlayAnchor::Composer;
        Self {
            spec,
            target,
            data,
            on_dismiss: None,
            on_refresh: None,
            on_trailing: None,
            filter,
            open,
            disabled: false,
            trigger_bounds: Rc::new(Cell::new(None)),
            composer_placement: Rc::new(Cell::new(None)),
            _subscriptions: vec![sub_filter],
        }
    }

    /// Re-render when an app entity changes (the store, the field, …). The
    /// shell cannot know what feeds the source closure, so the app wires its
    /// own entities here.
    pub fn observe_entity<T: 'static>(&mut self, entity: &Entity<T>, cx: &mut Context<Self>) {
        self._subscriptions
            .push(cx.observe(entity, |_, _, cx| cx.notify()));
    }

    /// Overlay dismissal lands here (the composer tile unmounts the picker;
    /// rail hosts clear their ephemeral state). Also fires after a pick.
    pub fn set_on_dismiss(&mut self, f: DismissFn) -> &mut Self {
        self.on_dismiss = Some(f);
        self
    }

    pub fn set_on_refresh(&mut self, f: RefreshFn) -> &mut Self {
        self.on_refresh = Some(f);
        self
    }

    /// A trailing-delete click (armed or not — the app decides).
    pub fn set_on_trailing(&mut self, f: TrailingFn) -> &mut Self {
        self.on_trailing = Some(f);
        self
    }

    /// Replaces the source closure once the picker exists — a data closure
    /// that needs the picker's own handle installs the real one after
    /// construction.
    pub fn set_data_source(&mut self, data: DataSource) {
        self.data = data;
    }

    /// Clears the filter without focusing; host resets run this on repo
    /// switches so the next open lists everything.
    pub fn clear_filter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.filter.update(cx, |f, cx| f.set_value("", window, cx));
        cx.notify();
    }

    /// Places the composer overlay's card against its trigger instead of the
    /// host's bottom-right corner: left-aligned with the trigger, its bottom
    /// edge just above the trigger's top. `trigger` and `host` are window
    /// bounds (the pill's and the tile's), as prepaint reports them; the host
    /// rect is what the catcher covers, so the card never leaves it.
    pub fn set_composer_anchor(
        &mut self,
        trigger: Bounds<Pixels>,
        host: Bounds<Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.composer_placement
            .set(Some(composer_placement(trigger, host)));
        cx.notify();
    }

    /// Closes the popup. The rail mode wires this to the window's
    /// outside-click and Escape dismissal; the composer mode routes through
    /// the owner's `on_dismiss`. Returns whether it was open.
    pub fn dismiss(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.open {
            return false;
        }
        self.open = false;
        if let Some(close) = &self.on_dismiss {
            close(cx);
        }
        cx.notify();
        true
    }

    /// Clear and focus the filter; host pills call this on open.
    pub fn reset_filter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.filter.update(cx, |state, cx| {
            state.set_value("", window, cx);
            state.focus(window, cx);
        });
        cx.notify();
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn set_open(&mut self, open: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.disabled && open {
            return;
        }
        if open && !self.open {
            self.filter.update(cx, |state, cx| {
                state.set_value("", window, cx);
                state.focus(window, cx);
            });
        }
        self.open = open;
        cx.notify();
    }

    /// Switches the trigger off without discarding its data: a disabled
    /// trigger dims, refuses to open, and closes a list already showing.
    pub fn set_disabled(&mut self, disabled: bool, cx: &mut Context<Self>) {
        if self.disabled == disabled {
            return;
        }
        self.disabled = disabled;
        if disabled && self.open {
            self.open = false;
            if let Some(close) = &self.on_dismiss {
                close(cx);
            }
        }
        cx.notify();
    }

    fn toggle_open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.set_open(!self.open, window, cx);
    }

    fn choose(&mut self, value: &str, window: &mut Window, cx: &mut Context<Self>) {
        match &self.target {
            PickerTarget::Field(state) => {
                let state = state.clone();
                state.update(cx, |state, cx| state.set_value(value, window, cx));
                self.open = false;
            }
            PickerTarget::Callback(pick) => {
                // a pick closes the dropdown and resets the filter, so the
                // next open lists everything
                self.filter.update(cx, |f, cx| f.set_value("", window, cx));
                pick(value, window, cx);
                self.open = false;
                if let Some(close) = &self.on_dismiss {
                    close(cx);
                }
            }
        }
        cx.notify();
    }

    fn read_data(&self, window: &Window, cx: &App) -> PickerData {
        let filter = self.filter.read(cx).value().trim().to_lowercase();
        (self.data)(&filter, window, cx)
    }

    fn root_id(&self) -> String {
        if self.spec.overlay {
            format!("overlay-picker-{}", self.spec.key)
        } else {
            format!("model-picker-{}", self.spec.key)
        }
    }

    fn trigger_id(&self) -> String {
        self.spec
            .trigger_id
            .map(str::to_string)
            .unwrap_or_else(|| format!("model-trigger-{}", self.spec.key))
    }

    fn panel_id(&self) -> String {
        self.spec
            .panel_id
            .map(str::to_string)
            .unwrap_or_else(|| format!("model-panel-{}", self.spec.key))
    }

    fn filter_selector(&self) -> String {
        self.spec
            .filter_selector
            .map(str::to_string)
            .unwrap_or_else(|| format!("model-filter-{}", self.spec.key))
    }

    fn list_id(&self) -> String {
        self.spec
            .list_id
            .map(str::to_string)
            .unwrap_or_else(|| format!("model-list-{}", self.spec.key))
    }

    // ---- rows ----

    fn group_row(
        id: &str,
        label: &str,
        selector: Option<&str>,
        font_meta: f32,
    ) -> gpui::AnyElement {
        let mut row = div()
            .id(SharedString::from(id.to_string()))
            .w_full()
            .h(px(ROW_HEIGHT))
            .px_2()
            .flex()
            .items_center()
            .text_size(px(font_meta))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(palette::text_muted());
        if let Some(selector) = selector {
            let selector = selector.to_string();
            row = row.debug_selector(move || selector.clone());
        }
        row.child(label.to_string()).into_any_element()
    }

    fn message_row(id: &str, label: &str, font_body: f32) -> gpui::AnyElement {
        div()
            .id(SharedString::from(id.to_string()))
            .w_full()
            .h(px(ROW_HEIGHT))
            .px_2()
            .flex()
            .items_center()
            .text_size(px(font_body))
            .text_color(palette::text_muted())
            .child(label.to_string())
            .into_any_element()
    }

    fn item_row(
        item: &PickerItem,
        spec: &PickerSpec,
        fonts: FontTokens,
        accent: gpui::Rgba,
        show_tooltips: bool,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let font_body = fonts.font_body;
        let icon_small = fonts.icon_small;
        let color = if item.selected && spec.selected_accent {
            accent
        } else if item.muted {
            palette::text_muted()
        } else if spec.selected_accent {
            palette::text_dim()
        } else {
            palette::text()
        };
        let weight = if item.selected && spec.selected_accent {
            FontWeight::SEMIBOLD
        } else {
            FontWeight::NORMAL
        };
        let value = item.value.clone();
        let id = item.id.clone();
        let selector = item.selector.clone();
        let mut row = div()
            .id(SharedString::from(id))
            .debug_selector(move || selector.clone())
            .a11y_option(item.label.clone(), item.selected)
            .w_full()
            .h(px(ROW_HEIGHT))
            .cursor_pointer()
            .rounded_md()
            .px_2()
            .flex()
            .items_center()
            .when(spec.selected_accent, |r| r.gap_1().group("picker-row"))
            .when(!spec.selected_accent, |r| r.gap_2())
            .when(item.selected, |r| r.bg(palette::HOVER))
            .hover(|r| r.bg(palette::HOVER))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.choose(&value.clone(), window, cx);
            }))
            .child(
                div()
                    .size(px(icon_small))
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(item.selected, |marker| {
                        marker.child(warpui_kit_core::icon(
                            spec.icons.check,
                            px(icon_small),
                            accent,
                        ))
                    }),
            )
            .child(
                // fills the row so a trailing action pins to the right edge —
                // hugging the label puts the invisible trash mid-row, under
                // the click meant for the branch name
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(font_body))
                    .text_color(color)
                    .font_weight(weight)
                    .child(item.label.clone()),
            );
        if let Some(trailing) = &item.trailing_delete {
            let armed = trailing.armed;
            let name = item.value.clone();
            let tooltip = if armed {
                "Click again to delete"
            } else {
                trailing.tooltip
            };
            let selector = trailing.selector.clone();
            let id = trailing.id.clone();
            row = row.child(warpui_kit_core::tooltip::with_tooltip(
                div()
                    .id(SharedString::from(id))
                    .debug_selector(move || selector.clone())
                    .a11y_button(if armed {
                        format!("Confirm delete {name}")
                    } else {
                        format!("Delete {name}")
                    })
                    .cursor_pointer()
                    .rounded_md()
                    .size(px(18.))
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .justify_center()
                    .opacity(0.)
                    .when(armed, |d| d.opacity(1.))
                    .group_hover("picker-row", |s| s.opacity(1.))
                    .hover(|s| s.bg(palette::tint(palette::DANGER)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        // never the row's pick
                        cx.stop_propagation();
                        if let Some(on_trailing) = &this.on_trailing {
                            if on_trailing(&name, cx) {
                                // confirmed: the shell closes; the app's
                                // on_dismiss clears its state
                                this.dismiss(cx);
                            } else {
                                cx.notify();
                            }
                        }
                    }))
                    .child(warpui_kit_core::icon(
                        spec.icons.trash,
                        px(icon_small - 2.),
                        if armed {
                            palette::DANGER
                        } else {
                            palette::text_muted()
                        },
                    )),
                show_tooltips,
                tooltip,
            ));
        }
        row.into_any_element()
    }

    fn build_list(
        &mut self,
        rows: &[PickerRow],
        fonts: FontTokens,
        accent: gpui::Rgba,
        show_tooltips: bool,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let spec = self.spec;
        let list_id = self.list_id();
        let (font_meta, font_body) = (fonts.font_meta, fonts.font_body);
        let rows = rows.to_vec();
        let list_height = rows.len().min(spec.visible_rows) as f32 * ROW_HEIGHT;
        uniform_list(
            SharedString::from(list_id),
            rows.len(),
            cx.processor(move |_this, range: std::ops::Range<usize>, _window, cx| {
                range
                    .map(|ix| match &rows[ix] {
                        PickerRow::Group {
                            id,
                            label,
                            selector,
                        } => Self::group_row(id, label, selector.as_deref(), font_meta),
                        PickerRow::Item(item) => {
                            Self::item_row(item, &spec, fonts, accent, show_tooltips, cx)
                        }
                        PickerRow::Message { id, label } => Self::message_row(id, label, font_body),
                    })
                    .collect()
            }),
        )
        .w_full()
        .h(px(list_height))
        .into_any_element()
    }

    // ---- chrome ----

    fn build_trigger(
        &self,
        data: &PickerData,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let trigger_id = self.trigger_id();
        let selector = trigger_id.clone();
        let fonts = &data.fonts;
        let (label_size, badge_size) = match self.spec.trigger_scale {
            TriggerScale::Compact => (fonts.font_body, fonts.font_sub),
            TriggerScale::Regular => (fonts.font_heading, fonts.font_control),
        };
        let mut trigger = div()
            .id(SharedString::from(trigger_id))
            .debug_selector(move || selector.clone())
            .role(gpui::Role::ComboBox)
            .aria_label(self.spec.trigger_label)
            .aria_expanded(self.open)
            .aria_value(data.selected.label.clone())
            .flex_1()
            .min_w_0()
            .cursor_pointer()
            .when(self.disabled, |trigger| trigger.cursor_default())
            .when_some(self.spec.trigger_min_h, |t, min_h| t.min_h(px(min_h)))
            .when(self.spec.trigger_rounded_lg, |t| t.rounded_lg())
            .when(!self.spec.trigger_rounded_lg, |t| t.rounded_md())
            .border_1()
            .border_color(if self.open {
                data.accent
            } else {
                palette::hairline()
            })
            .bg(palette::well())
            .px_2()
            .py_1()
            .flex()
            .items_center()
            .gap_2()
            .when(!self.disabled, |trigger| {
                trigger.hover(|s| s.border_color(palette::text_muted()))
            })
            .when(self.disabled, |trigger| trigger.opacity(0.45));
        if self.spec.stop_trigger_mousedown {
            trigger = trigger.on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
        }
        trigger = trigger
            .on_click(cx.listener(|this, _, window, cx| this.toggle_open(window, cx)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap_2()
                    .when_some(self.spec.trigger_icon, |row, icon_path| {
                        row.child(warpui_kit_core::icon(
                            icon_path,
                            px(fonts.icon_small),
                            palette::text_muted(),
                        ))
                    })
                    .when(self.spec.provider_badge, |row| match &data.selected.badge {
                        Some(badge) if !badge.is_empty() => row.child(
                            div()
                                .px_1p5()
                                .py_0p5()
                                .rounded_sm()
                                .bg(palette::raised())
                                .text_size(px(badge_size))
                                .text_color(palette::text_muted())
                                .child(badge.clone()),
                        ),
                        _ => row,
                    })
                    .child(
                        div()
                            .truncate()
                            .text_size(px(label_size))
                            .text_color(if data.selected.value.is_empty() {
                                palette::text_muted()
                            } else {
                                palette::text()
                            })
                            .child(data.selected.label.clone()),
                    ),
            )
            .child(warpui_kit_core::icon(
                self.spec.icons.chevron_down,
                px(fonts.icon_small),
                palette::text_muted(),
            ));
        trigger
    }

    fn refresh_button(
        &self,
        data: &PickerData,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let fonts = data.fonts;
        let refreshing = data.refreshing;
        let selector = format!("picker-refresh-{}", self.spec.key);
        let debug_selector = selector.clone();
        let label = if refreshing {
            "Refreshing models"
        } else {
            self.spec.refresh_label
        };
        div()
            .id(SharedString::from(selector))
            .debug_selector(move || debug_selector.clone())
            .a11y_button(label)
            .when(!refreshing, |button| button.cursor_pointer())
            .rounded_md()
            .px_2()
            .py_1()
            .flex()
            .items_center()
            .text_size(px(fonts.font_body))
            .text_color(palette::text_muted())
            .border_1()
            .border_color(palette::hairline())
            .when(!refreshing, |button| button.hover(|s| s.bg(palette::HOVER)))
            .on_click(cx.listener(move |this, _, _, cx| {
                // a second click while the first is in flight would queue a
                // duplicate backend read
                if refreshing {
                    return;
                }
                if let Some(on_refresh) = &this.on_refresh {
                    on_refresh(cx);
                    cx.notify();
                }
            }))
            .child(if refreshing {
                warpui_kit_core::motion::spinner(
                    SharedString::from(format!("picker-refresh-spin-{}", self.spec.key)),
                    self.spec.icons.refresh,
                    px(fonts.icon_small),
                    palette::text_muted(),
                )
            } else {
                warpui_kit_core::icon(
                    self.spec.icons.refresh,
                    px(fonts.icon_small),
                    palette::text_muted(),
                )
                .into_any_element()
            })
    }

    fn build_trigger_row(&self, data: &PickerData, cx: &mut Context<Self>) -> gpui::AnyElement {
        div()
            .w_full()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .child(self.build_trigger(data, cx))
            .when(self.spec.refresh, |row| {
                row.child(self.refresh_button(data, cx))
            })
            .into_any_element()
    }

    fn build_panel(
        &mut self,
        data: PickerData,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let panel_id = self.panel_id();
        let panel_selector = panel_id.clone();
        let mut panel = div()
            .id(SharedString::from(panel_id))
            .debug_selector(move || panel_selector.clone())
            .when(self.spec.panel_chrome == PanelChrome::Cards, |p| {
                p.rounded_lg().shadow_lg()
            })
            .when(self.spec.panel_chrome == PanelChrome::Menus, |p| {
                p.rounded_md().shadow_md()
            })
            .p_1()
            .gap_1()
            .border_1()
            .border_color(palette::hairline())
            .bg(palette::popup())
            .flex()
            .flex_col();
        if self.spec.overlay
            && matches!(self.spec.anchor, OverlayAnchor::Rail | OverlayAnchor::Field)
        {
            panel = panel
                // presses inside the popup (filter field, scrollbar, rows)
                // must not reach the window's outside-click dismissal
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .occlude();
        }
        if self.spec.overlay && self.spec.anchor == OverlayAnchor::Field {
            // a floating panel has no parent to inherit width from: it takes
            // the trigger's measured width and a floor, so a narrow settings
            // slot still fits the model names
            panel = panel.min_w(px(FIELD_PANEL_MIN_WIDTH));
        }
        if !self.spec.overlay {
            // the settings-slot popup spans the slot's width
            panel = panel.w_full();
        }
        match data.body {
            PickerBody::Replace(element) => panel.child(element),
            PickerBody::Rows(rows) => {
                let filter_selector = self.filter_selector();
                let placeholder = SharedString::from(self.spec.filter_placeholder.to_string());
                let list = self.build_list(&rows, data.fonts, data.accent, data.show_tooltips, cx);
                // the prototype draws the picker's filter as the one
                // `.field` with its search icon, not a bare kit input
                let handle = self.filter.read(cx).focus_handle(cx);
                let icon = warpui_kit_core::icon_data(
                    SEARCH.as_bytes(),
                    px(warpui_kit_core::fonts::icon_small()),
                    palette::text_muted(),
                );
                let mut content = panel
                    .child(
                        div()
                            .id(SharedString::from(filter_selector.clone()))
                            .debug_selector(move || filter_selector.clone())
                            .role(gpui::Role::SearchInput)
                            .aria_label(placeholder.clone())
                            .aria_placeholder(placeholder)
                            .child(
                                warpui_kit_core::field::boxed(
                                    "picker-filter-field",
                                    &handle,
                                    window,
                                    warpui_kit_core::shell::field_height(),
                                )
                                .child(icon)
                                .child(warpui_kit_core::field::input(&self.filter)),
                            ),
                    )
                    .child(list);
                if let Some(footer) = data.footer {
                    content = content.child(footer);
                }
                content
            }
        }
    }

    /// The rail-mode popup as an overlay element for the host: the caller
    /// appends it AFTER its other children so later-sibling paint order puts
    /// it on top (no deferred) and every hitbox registers in the normal pass.
    pub fn overlay_popup(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        if !self.spec.overlay || self.spec.anchor != OverlayAnchor::Rail || !self.open {
            return None;
        }
        let data = self.read_data(window, cx);
        let panel = self.build_panel(data, window, cx);
        // anchored to the rail (the nearest positioned ancestor)
        Some(
            panel
                .absolute()
                .top(px(-4.))
                .left(px(8.))
                .right(px(8.))
                .into_any_element(),
        )
    }
}

impl Render for FilteredPicker {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Composer overlay: this entity IS the catcher + anchored card; the
        // host mounts it only while its pill is open. build_panel already
        // applies the Cards chrome, so the card only sizes it.
        if self.spec.overlay && self.spec.anchor == OverlayAnchor::Composer {
            let data = self.read_data(window, cx);
            let panel = self.build_panel(data, window, cx);
            // the host places the card against its pill when it can (see
            // set_composer_anchor); the corner default is the fallback
            let card = div().absolute();
            let card = match self.composer_placement.get() {
                Some((left, bottom)) => card.left(left).bottom(bottom),
                None => card.bottom_10().right_8(),
            };
            return div()
                .id("picker-catcher")
                .debug_selector(|| "picker-catcher".into())
                .absolute()
                .inset_0()
                // hitbox hover is geometric in gpui: without occlusion the
                // composer/transcript under the dropdown keeps receiving clicks
                .occlude()
                // click-elsewhere dismiss; popup rows are children, so
                // they stay above the catcher and keep working
                .on_click(cx.listener(|this, _, _, cx| {
                    this.dismiss(cx);
                }))
                // Escape closes from anywhere inside the card: the filter
                // holds focus while the popup is open, and key events bubble
                // up this catcher's subtree
                .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                    if event.keystroke.key == "escape" {
                        this.dismiss(cx);
                        cx.stop_propagation();
                    }
                }))
                .child(card.child(panel.w(px(OVERLAY_WIDTH))))
                .into_any_element();
        }

        let data = self.read_data(window, cx);

        // Field mode floats: the trigger stays in the form, the panel rides the
        // window layer just under it. The trigger reports its own bounds from
        // prepaint, so the panel opens at that slot's edge whatever its height,
        // and a 300px list covers the rows below instead of stretching the
        // card around them.
        if self.spec.overlay && self.spec.anchor == OverlayAnchor::Field {
            let trigger_row = self.build_trigger_row(&data, cx);
            let measured = self.trigger_bounds.clone();
            let field = div()
                .id(SharedString::from(self.root_id()))
                .relative()
                .w_full()
                .child(
                    div()
                        .id("field-trigger-bounds")
                        .w_full()
                        .on_prepaint(move |bounds, _, _| measured.set(Some(bounds)))
                        .child(trigger_row),
                );
            if !self.open {
                return field.into_any_element();
            }
            let panel = self.build_panel(data, window, cx).w_full();
            // the panel hangs off the trigger's bottom edge: `bounds` are
            // window coordinates from prepaint, and this is the same window
            // the anchored layer positions in
            let at = self
                .trigger_bounds
                .get()
                .map(|b| gpui::point(b.origin.x, b.origin.y + b.size.height))
                .unwrap_or_default();
            // The catcher is the kit's own menu pattern: a window-sized layer
            // in the popup priority band, with the card nested in a second
            // anchored pass. A trigger-sized catcher never sees a click that
            // lands elsewhere on the form, so the picker would stay open.
            let window_size = window.bounds().size;
            // The catcher paints over the whole window, but it is an absolute
            // child of the field, so its origin must be the field's origin
            // moved back to the window's. Without this the catcher starts at
            // the field (a right-hand settings column now) and its
            // window-sized area runs off the window, so a click on empty
            // space never reaches it.
            let catcher_origin = self
                .trigger_bounds
                .get()
                .map(|b| b.origin)
                .unwrap_or_default();
            return field
                .child(deferred(
                    div()
                        .id("field-catcher")
                        .debug_selector(|| "field-catcher".into())
                        .absolute()
                        .top(-catcher_origin.y)
                        .left(-catcher_origin.x)
                        .w(window_size.width)
                        .h(window_size.height)
                        // geometric hitboxes: without this the form under the
                        // panel keeps taking the click meant for a row
                        .occlude()
                        // click-elsewhere dismiss; the card is a child, so its
                        // own presses stay above the catcher and keep working
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, _, cx| {
                                this.dismiss(cx);
                            }),
                        )
                        .child(
                            anchored()
                                .position(at)
                                .offset(gpui::point(px(0.), px(4.)))
                                .snap_to_window_with_margin(px(8.))
                                // a long list flips up rather than running off
                                // the bottom of the settings card
                                .child(panel),
                        ),
                ))
                .into_any_element();
        }

        // Rail mode: the trigger; the popup is host-mounted via overlay_popup.
        // In-flow mode (no overlay) keeps the panel under the trigger.
        let trigger_row = self.build_trigger_row(&data, cx);
        let popup = if !self.spec.overlay && self.open {
            Some(self.build_panel(data, window, cx))
        } else {
            None
        };

        let root_id = self.root_id();
        div()
            .id(SharedString::from(root_id))
            .when(!self.spec.overlay, |p| p.relative())
            .w_full()
            .flex()
            .flex_col()
            .gap_1()
            .when(!self.spec.overlay && self.open, |picker| {
                picker.on_mouse_down_out(cx.listener(|this, _, _, cx| {
                    this.dismiss(cx);
                }))
            })
            .child(trigger_row)
            .children(popup)
            .into_any_element()
    }
}
