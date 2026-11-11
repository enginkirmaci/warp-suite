//! The family's list key map.
//!
//! A list owns its cursor, its selection and its focus handle; this module
//! only answers two questions every list asks the same way: which key is this,
//! and where does the cursor go. Keeping the map here means a repo list, a
//! chat tree and a note tree step on the same keys instead of each drifting
//! (see `docs/agents/design-system.md`, *Keyboard navigation*).
//!
//! The host stays responsible for focus: call [`list_key`] from the list
//! element's own `.on_key_down`, never from the window. A control or alt
//! chord belongs to a shortcut, not to the cursor, so it is not a list key.

use gpui::{KeyDownEvent, Modifiers};

/// How many rows a page key moves when the host has no viewport measurement.
pub const DEFAULT_PAGE: usize = 10;

/// One key a list understands.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ListKey {
    /// Move the cursor one row up, clamped at the first.
    Up,
    /// Move the cursor one row down, clamped at the last.
    Down,
    /// Move the cursor one page up, clamped at the first.
    PageUp,
    /// Move the cursor one page down, clamped at the last.
    PageDown,
    /// Move the cursor to the first row.
    Home,
    /// Move the cursor to the last row.
    End,
    /// Collapse the row, or go to its parent in a tree.
    Left,
    /// Expand the row, or go to its first child in a tree.
    Right,
    /// Open or activate the cursor row.
    Enter,
    /// Open the cursor row's context menu (`Shift+F10`).
    Menu,
}

impl ListKey {
    /// Whether this key moves the cursor. `Enter` and `Menu` act on the row
    /// instead, and a host handles them after [`step`] returns `None`.
    pub fn moves(self) -> bool {
        matches!(
            self,
            Self::Up | Self::Down | Self::PageUp | Self::PageDown | Self::Home | Self::End
        )
    }
}

/// Classify a key event as a list key, or `None` when the list should leave it
/// to the focused widget, the app or the platform.
pub fn list_key(event: &KeyDownEvent) -> Option<ListKey> {
    key_for(&event.keystroke.key, event.keystroke.modifiers)
}

/// [`list_key`] for the parts a test can build without a window.
pub fn key_for(key: &str, modifiers: Modifiers) -> Option<ListKey> {
    // a control/alt/platform chord is a shortcut, not a cursor step; shift is
    // allowed because no list here selects a range
    if modifiers.control || modifiers.alt || modifiers.platform {
        return None;
    }
    match key {
        "up" => Some(ListKey::Up),
        "down" => Some(ListKey::Down),
        "pageup" | "page_up" => Some(ListKey::PageUp),
        "pagedown" | "page_down" => Some(ListKey::PageDown),
        "home" => Some(ListKey::Home),
        "end" => Some(ListKey::End),
        "left" => Some(ListKey::Left),
        "right" => Some(ListKey::Right),
        // Shift+Enter is "open in a second place" where a list supports it,
        // not the plain activate the map owns
        "enter" if !modifiers.shift => Some(ListKey::Enter),
        "f10" if modifiers.shift => Some(ListKey::Menu),
        _ => None,
    }
}

/// Where the cursor goes for `key`, or `None` when the key does not move it
/// (including an empty list).
///
/// A missing or stale cursor starts at the near end: `Up`/`PageUp` land on the
/// last row, everything else on the first. Steps clamp rather than wrap, so
/// holding a key settles at the end of the list.
pub fn step(cursor: Option<usize>, len: usize, key: ListKey, page: usize) -> Option<usize> {
    if len == 0 {
        return None;
    }
    let last = len - 1;
    let page = page.max(1);
    let current = cursor.filter(|ix| *ix < len);
    match key {
        ListKey::Up => Some(current.map_or(last, |ix| ix.saturating_sub(1))),
        ListKey::Down => Some(current.map_or(0, |ix| (ix + 1).min(last))),
        ListKey::PageUp => Some(current.map_or(last, |ix| ix.saturating_sub(page))),
        ListKey::PageDown => Some(current.map_or(0, |ix| (ix + page).min(last))),
        ListKey::Home => Some(0),
        ListKey::End => Some(last),
        ListKey::Left | ListKey::Right | ListKey::Enter | ListKey::Menu => None,
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
    fn an_empty_list_has_no_step() {
        for key in [
            ListKey::Up,
            ListKey::Down,
            ListKey::Home,
            ListKey::End,
            ListKey::PageUp,
            ListKey::PageDown,
        ] {
            assert_eq!(step(None, 0, key, 10), None, "{key:?}");
        }
    }

    #[test]
    fn arrows_move_one_and_clamp() {
        assert_eq!(step(Some(3), 5, ListKey::Up, 10), Some(2));
        assert_eq!(step(Some(3), 5, ListKey::Down, 10), Some(4));
        assert_eq!(step(Some(0), 5, ListKey::Up, 10), Some(0));
        assert_eq!(step(Some(4), 5, ListKey::Down, 10), Some(4));
    }

    #[test]
    fn a_missing_cursor_starts_at_the_near_end() {
        assert_eq!(step(None, 5, ListKey::Up, 10), Some(4));
        assert_eq!(step(None, 5, ListKey::PageUp, 10), Some(4));
        assert_eq!(step(None, 5, ListKey::Down, 10), Some(0));
        assert_eq!(step(None, 5, ListKey::PageDown, 10), Some(0));
    }

    #[test]
    fn a_stale_cursor_is_treated_as_missing() {
        assert_eq!(step(Some(99), 5, ListKey::Down, 10), Some(0));
        assert_eq!(step(Some(99), 5, ListKey::Up, 10), Some(4));
    }

    #[test]
    fn home_and_end_jump_to_the_ends() {
        assert_eq!(step(Some(2), 5, ListKey::Home, 10), Some(0));
        assert_eq!(step(Some(2), 5, ListKey::End, 10), Some(4));
    }

    #[test]
    fn page_steps_use_the_page_size_and_clamp() {
        assert_eq!(step(Some(5), 20, ListKey::PageDown, 4), Some(9));
        assert_eq!(step(Some(5), 20, ListKey::PageUp, 4), Some(1));
        assert_eq!(step(Some(1), 20, ListKey::PageUp, 4), Some(0));
        assert_eq!(step(Some(18), 20, ListKey::PageDown, 4), Some(19));
    }

    #[test]
    fn action_keys_do_not_step() {
        for key in [ListKey::Left, ListKey::Right, ListKey::Enter, ListKey::Menu] {
            assert_eq!(step(Some(2), 5, key, 10), None, "{key:?}");
            assert!(!key.moves());
        }
    }

    #[test]
    fn the_key_map_matches_the_documented_keys() {
        assert_eq!(key_for("up", plain()), Some(ListKey::Up));
        assert_eq!(key_for("down", plain()), Some(ListKey::Down));
        assert_eq!(key_for("pageup", plain()), Some(ListKey::PageUp));
        assert_eq!(key_for("pagedown", plain()), Some(ListKey::PageDown));
        assert_eq!(key_for("home", plain()), Some(ListKey::Home));
        assert_eq!(key_for("end", plain()), Some(ListKey::End));
        assert_eq!(key_for("left", plain()), Some(ListKey::Left));
        assert_eq!(key_for("right", plain()), Some(ListKey::Right));
        assert_eq!(key_for("enter", plain()), Some(ListKey::Enter));
        assert_eq!(key_for("f10", shift()), Some(ListKey::Menu));
    }

    #[test]
    fn shortcuts_are_not_list_keys() {
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
        ] {
            assert_eq!(key_for("up", modifiers), None);
            assert_eq!(key_for("enter", modifiers), None);
        }
    }

    #[test]
    fn shift_enter_is_not_plain_enter() {
        assert_eq!(key_for("enter", shift()), None);
        assert_eq!(key_for("f10", plain()), None);
    }
}
