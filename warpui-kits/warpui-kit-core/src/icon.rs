//! The tinted svg icon builder shared by every surface.

use gpui::Styled as _;

/// A tinted svg icon. The path is the app's own asset path — this crate ships
/// no assets, callers pass e.g. `"icons/icon-check.svg"`.
pub fn icon(path: &str, size: gpui::Pixels, color: impl Into<gpui::Hsla>) -> gpui::Svg {
    // flex_shrink_0: an svg is a leaf with no intrinsic size, so the default
    // flex-shrink can squeeze it to zero bounds in a tight container — gpui
    // then fails the render with "can't render at a zero size" on every repaint
    gpui::svg()
        .path(path)
        .size(size)
        .flex_shrink_0()
        .text_color(color.into())
}
