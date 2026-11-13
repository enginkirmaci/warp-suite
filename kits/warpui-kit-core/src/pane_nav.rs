//! The family's pane key map.
//!
//! A list owns its cursor and its focus handle ([`crate::list_nav`]); this
//! module only answers the other half of the keyboard: when `←`/`→` or
//! `Tab`/`Shift+Tab` should hand the keyboard to the list beside the focused
//! one. A host keeps one [`PaneNav`], clears it at the top of its render and
//! registers each pane that is actually on screen, left to right, from the
//! code that draws it. A pane that is not drawn is not a stop, and the row
//! never wraps: `→` on the rightmost pane moves nowhere.
//!
//! `Tab` is the same move as `→` and `Shift+Tab` the same as `←`, so a list
//! that only knows the tab habit still lands on its neighbour. Arrow keys are
//! the tree's in a tree; a tree keeps `←`/`→` for collapse/expand and opts
//! into `Tab` alone (the host checks [`direction`] before its tree handler).

use gpui::{App, FocusHandle, KeyDownEvent, Modifiers, Window};
use std::cell::RefCell;

/// Which list a pane key asks for.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PaneDir {
    Left,
    Right,
}

/// Classify a key event as a pane move, or `None` when the key belongs to the
/// list cursor, a field, a shortcut or the platform.
///
/// A control/alt/platform/function chord is a shortcut, not a pane move;
/// `Shift` is allowed only for `Tab` (`Shift+Tab`), because a list may use
/// `Shift`+arrow for its own selection. A held key does not walk the row: one
/// press, one pane.
pub fn direction(event: &KeyDownEvent) -> Option<PaneDir> {
    direction_for(
        &event.keystroke.key,
        event.keystroke.modifiers,
        event.is_held,
    )
}

/// [`direction`] for the parts a test can build without a window.
pub fn direction_for(key: &str, modifiers: Modifiers, is_held: bool) -> Option<PaneDir> {
    if is_held || modifiers.control || modifiers.alt || modifiers.platform || modifiers.function {
        return None;
    }
    match key {
        "tab" => Some(if modifiers.shift {
            PaneDir::Left
        } else {
            PaneDir::Right
        }),
        "left" if !modifiers.shift => Some(PaneDir::Left),
        "right" if !modifiers.shift => Some(PaneDir::Right),
        _ => None,
    }
}

/// The panes on screen this frame, in left-to-right order.
///
/// A host owns one instance and shares it with the views that draw the panes
/// (an `Rc<PaneNav>` in practice). The order is empty until the host's render
/// starts its frame; registering twice in one frame duplicates the pane, so
/// each view registers once, in its own render.
#[derive(Default)]
pub struct PaneNav {
    panes: RefCell<Vec<FocusHandle>>,
}

impl PaneNav {
    pub fn new() -> Self {
        Self::default()
    }

    /// Drops last frame's order. Call once, before the first pane registers.
    pub fn begin_frame(&self) {
        self.panes.borrow_mut().clear();
    }

    /// Appends a pane that is on screen, in left-to-right order.
    pub fn register(&self, handle: &FocusHandle) {
        self.panes.borrow_mut().push(handle.clone());
    }

    /// Focuses the pane next to `from` and reports whether one answered.
    /// Returns `false` when `from` is not registered or the row ends there,
    /// so the caller can leave the key unhandled instead of swallowing it.
    pub fn move_from(
        &self,
        from: &FocusHandle,
        dir: PaneDir,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        let target = {
            let panes = self.panes.borrow();
            let Some(ix) = panes.iter().position(|pane| pane == from) else {
                return false;
            };
            let next = match dir {
                PaneDir::Left => ix.checked_sub(1),
                PaneDir::Right => (ix + 1 < panes.len()).then_some(ix + 1),
            };
            let Some(next) = next else {
                return false;
            };
            panes[next].clone()
        };
        window.focus(&target, cx);
        true
    }

    /// How many panes the last frame registered.
    pub fn len(&self) -> usize {
        self.panes.borrow().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain() -> Modifiers {
        Modifiers::default()
    }

    fn shift() -> Modifiers {
        Modifiers {
            shift: true,
            ..Modifiers::default()
        }
    }

    #[test]
    fn arrows_move_left_and_right() {
        assert_eq!(direction_for("left", plain(), false), Some(PaneDir::Left));
        assert_eq!(direction_for("right", plain(), false), Some(PaneDir::Right));
    }

    #[test]
    fn tab_moves_right_and_shift_tab_moves_left() {
        assert_eq!(direction_for("tab", plain(), false), Some(PaneDir::Right));
        assert_eq!(direction_for("tab", shift(), false), Some(PaneDir::Left));
    }

    #[test]
    fn shifted_arrows_are_not_pane_moves() {
        assert_eq!(direction_for("left", shift(), false), None);
        assert_eq!(direction_for("right", shift(), false), None);
    }

    #[test]
    fn shortcuts_and_held_keys_are_not_pane_moves() {
        for modifiers in [
            Modifiers {
                control: true,
                ..Modifiers::default()
            },
            Modifiers {
                alt: true,
                ..Modifiers::default()
            },
            Modifiers {
                platform: true,
                ..Modifiers::default()
            },
            Modifiers {
                function: true,
                ..Modifiers::default()
            },
        ] {
            assert_eq!(direction_for("tab", modifiers, false), None);
            assert_eq!(direction_for("right", modifiers, false), None);
        }
        assert_eq!(direction_for("right", plain(), true), None);
        assert_eq!(direction_for("tab", plain(), true), None);
    }

    #[test]
    fn the_cursor_keys_are_not_pane_moves() {
        for key in ["up", "down", "home", "end", "pageup", "pagedown", "enter"] {
            assert_eq!(direction_for(key, plain(), false), None, "{key}");
        }
    }

    #[test]
    fn a_fresh_order_is_empty_and_registration_keeps_order() {
        let nav = PaneNav::new();
        assert!(nav.is_empty());
        nav.begin_frame();
        assert_eq!(nav.len(), 0);
    }
}
