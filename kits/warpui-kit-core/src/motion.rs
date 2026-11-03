//! Motion for the whole family: short, and only ever opacity or a transform. It
//! says that something changed or that work is running, it never decorates.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use gpui::prelude::*;
use gpui::{
    Animation, AnimationExt as _, AnyElement, Div, ElementId, Hsla, Transformation, div,
    percentage, px, relative, size,
};

use crate::icon::icon;

/// Hover and press.
pub const FAST: Duration = Duration::from_millis(120);
/// Entrances, switches, toasts and dialogs.
pub const BASE: Duration = Duration::from_millis(180);
/// One turn of the spinner.
pub const SPIN: Duration = Duration::from_millis(800);

/// One pass of the progress line.
pub const PROGRESS: Duration = Duration::from_millis(1200);
/// How far an entrance rises, in pixels.
const RISE: f32 = 6.;
/// The share of its track the progress line covers.
const PROGRESS_SHARE: f32 = 0.3;

static REDUCED: AtomicBool = AtomicBool::new(false);

/// Turns all of it off, spinners included. The busy words stay, so nothing
/// depends on movement. Call `cx.refresh_windows()` after a change at runtime.
pub fn set_reduced(on: bool) {
    REDUCED.store(on, Ordering::Relaxed);
}

pub fn reduced() -> bool {
    REDUCED.load(Ordering::Relaxed)
}

/// What the desktop asks for. It runs a process, so it belongs at startup.
#[cfg(all(unix, not(target_os = "macos")))]
pub fn system_prefers_reduced() -> bool {
    std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "enable-animations"])
        .output()
        .is_ok_and(|output| String::from_utf8_lossy(&output.stdout).trim() == "false")
}

/// What the desktop asks for. It runs a process, so it belongs at startup.
#[cfg(windows)]
pub fn system_prefers_reduced() -> bool {
    use std::os::windows::process::CommandExt as _;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    std::process::Command::new("reg")
        .args([
            "query",
            r"HKCU\Control Panel\Desktop\WindowMetrics",
            "/v",
            "MinAnimate",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .is_ok_and(|output| {
            String::from_utf8_lossy(&output.stdout)
                .split_whitespace()
                .last()
                == Some("0")
        })
}

/// What the desktop asks for. macOS isn't a target, so it stays quiet.
#[cfg(target_os = "macos")]
pub fn system_prefers_reduced() -> bool {
    false
}

/// A cubic ease-out, close to the design's `cubic-bezier(.2, 0, 0, 1)`: quick
/// out, long settle.
pub fn ease(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

/// Running work: an icon turning at a constant speed. The path is the app's own
/// asset, as in [`icon()`], and the id must be stable across frames or the turn
/// restarts. Always pair it with a word: the motion may be switched off.
pub fn spinner(
    id: impl Into<ElementId>,
    path: &str,
    size: gpui::Pixels,
    color: impl Into<gpui::Hsla>,
) -> AnyElement {
    let glyph = icon(path, size, color);
    if reduced() {
        return glyph.into_any_element();
    }
    glyph
        .with_animation(id, Animation::new(SPIN).repeat(), |svg, delta| {
            svg.with_transformation(Transformation::rotate(percentage(delta)))
        })
        .into_any_element()
}

/// Fades `element` in as it rises the last few pixels. The animation runs once
/// per id, so an id that changes only with the view plays only on a real change
/// and a redraw leaves it still.
pub fn enter<E>(id: impl Into<ElementId>, element: E) -> AnyElement
where
    E: IntoElement + Styled + 'static,
{
    if reduced() {
        return element.into_any_element();
    }
    element
        .with_animation(
            id,
            Animation::new(BASE).with_easing(ease),
            |element, delta| {
                element
                    .opacity(delta)
                    .relative()
                    .top(px(RISE * (1.0 - delta)))
            },
        )
        .into_any_element()
}

/// A glyph that jumps once when `id` changes: the favorite star.
pub fn pop(id: impl Into<ElementId>, glyph: gpui::Svg) -> AnyElement {
    if reduced() {
        return glyph.into_any_element();
    }
    glyph
        .with_animation(id, Animation::new(BASE).with_easing(ease), |svg, delta| {
            // 1.0 at both ends, 1.3 at the top of the jump
            let scale = 1.0 + 0.3 * (delta * std::f32::consts::PI).sin();
            svg.with_transformation(Transformation::scale(size(scale, scale)))
        })
        .into_any_element()
}

/// The 2 px line under a head while work runs on what it shows. It holds still
/// when motion is off, and the words beside it say the rest.
pub fn progress_line(id: impl Into<ElementId>, color: impl Into<Hsla>) -> AnyElement {
    let color = color.into();
    let track = div().absolute().left_0().bottom_0().h(px(2.));
    if reduced() {
        return track.w_full().bg(color).into_any_element();
    }
    // The sweep starts left of its track so the line enters from nothing, and
    // an unclipped inset draws past the pane it belongs to — out over the rail
    // in a list head's case. The track clips that overhang.
    track
        .right_0()
        .overflow_hidden()
        .child(
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .w(relative(PROGRESS_SHARE))
                .bg(color)
                .with_animation(id, Animation::new(PROGRESS).repeat(), |line: Div, delta| {
                    line.left(relative(delta * (1.0 + PROGRESS_SHARE) - PROGRESS_SHARE))
                }),
        )
        .into_any_element()
}

/// The accent mark under a selected tab, growing in from the left. It fills the
/// track it is put in, and stands whole when motion is off.
pub fn grow_mark(id: impl Into<ElementId>, color: impl Into<Hsla>) -> AnyElement {
    let fill = div().h_full().bg(color.into());
    if reduced() {
        return fill.w_full().into_any_element();
    }
    fill.with_animation(
        id,
        Animation::new(BASE).with_easing(ease),
        |fill: Div, delta| fill.w(relative(delta)),
    )
    .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_easing_starts_and_lands_on_its_ends() {
        assert_eq!(ease(0.0), 0.0);
        assert_eq!(ease(1.0), 1.0);
    }

    #[test]
    fn reduced_motion_is_a_switch_the_family_shares() {
        set_reduced(true);
        assert!(reduced());
        set_reduced(false);
        assert!(!reduced());
    }

    #[test]
    fn the_easing_is_past_halfway_early() {
        assert!(ease(0.3) > 0.5);
    }
}
