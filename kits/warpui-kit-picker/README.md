# warpui-kit-picker

The filtered select-style picker for [gpui-kit](https://crates.io/crates/gpui-kit) applications: a trigger over a filter input and a virtualized, a11y-labelled list — with the data kept entirely app-side.

## Usage

The shell re-reads a source closure on every render with the current filter string; the app observes whatever entities feed it:

```rust
use warpui_kit_picker::{
    FilteredPicker, OverlayAnchor, PickerBody, PickerData, PickerRow, PickerSpec,
    PickerTarget,
};
use std::sync::Arc;

let picker = cx.new(|cx| {
    let mut picker = FilteredPicker::new(
        spec,                          // PickerSpec: ids, chrome, visible rows, icons
        PickerTarget::Field(field.clone()), // or Callback for custom handling
        Box::new(|filter: &str, cx: &App| PickerData {
            body: PickerBody::Rows(rows_for(filter, cx)),
            selected: current_selection(cx),
            footer: None,              // app-built element under the list
            fonts: tokens_for(cx),
            accent: accent_for(cx),
            show_tooltips: true,
            refreshing: false,       // true spins the refresh button
        }),
        window,
        cx,
    );
    picker.observe_entity(&store, cx); // re-render when the data changes
    picker
});
```

Three mount shapes via `PickerSpec::overlay` + `OverlayAnchor`:

| Shape | Behavior |
| --- | --- |
| in-flow (default) | The popup is a child below the trigger (`mouse_down_out` dismissal) |
| `Rail` | The trigger renders; the host appends `overlay_popup()` as its **last child** so it paints over the rail without `deferred` (whose hitboxes misplace nested interactive elements) |
| `Composer` | No trigger — the host mounts the entity only while its pill is open; a full-bleed catcher dismisses, picks and dismissal land through closures holding the owner weakly |

The new-branch-style flows are app-built: return `PickerBody::Replace(...)` to take over the panel body, or a `footer` element under the list. Trailing row actions (the armed delete) are rendered by the shell; the arm/confirm cycle stays in the app via `with_on_trailing`.

`set_disabled(true)` dims the trigger and refuses to open it, for a slot whose
value an adjacent control owns (warp-agent's Variant field while "always select
the highest variant" is on). The data stays live, so switching it back on needs
no rebuild.

## License

MIT
