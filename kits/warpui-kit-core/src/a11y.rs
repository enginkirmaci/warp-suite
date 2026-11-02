//! Accessibility wiring: gpui reports an accessibility node only for an id-ed
//! element with an explicit role, and a node without a label is invisible to
//! AT-SPI clients — so every hand-rolled control passes through one of these.

use gpui::SharedString;
use gpui::Toggled;

pub trait A11y: gpui::StatefulInteractiveElement + Sized {
    fn a11y_button(self, label: impl Into<SharedString>) -> Self {
        self.role(gpui::Role::Button).aria_label(label)
    }

    fn a11y_toggle(self, label: impl Into<SharedString>, on: bool) -> Self {
        self.role(gpui::Role::Button)
            .aria_label(label)
            .aria_toggled(if on { Toggled::True } else { Toggled::False })
    }

    fn a11y_switch(self, label: impl Into<SharedString>, on: bool) -> Self {
        self.role(gpui::Role::Switch)
            .aria_label(label)
            .aria_toggled(if on { Toggled::True } else { Toggled::False })
    }

    fn a11y_menu_item(self, label: impl Into<SharedString>) -> Self {
        self.role(gpui::Role::MenuItem).aria_label(label)
    }

    fn a11y_tab(self, label: impl Into<SharedString>, selected: bool) -> Self {
        self.role(gpui::Role::Tab)
            .aria_label(label)
            .aria_selected(selected)
    }

    fn a11y_option(self, label: impl Into<SharedString>, selected: bool) -> Self {
        self.role(gpui::Role::ListBoxOption)
            .aria_label(label)
            .aria_selected(selected)
    }

    /// `expanded` only attaches when the item can collapse (group headers,
    /// folder rows); plain leaves pass `None`.
    fn a11y_tree_item(self, label: impl Into<SharedString>, expanded: Option<bool>) -> Self {
        let item = self.role(gpui::Role::TreeItem).aria_label(label);
        match expanded {
            Some(expanded) => item.aria_expanded(expanded),
            None => item,
        }
    }
}

impl<T: gpui::StatefulInteractiveElement> A11y for T {}
