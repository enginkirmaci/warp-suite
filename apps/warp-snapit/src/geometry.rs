//! Small geometry types shared by the layout math and the Windows engine.
//!
//! `Point`/`Size` are layout-space (float) values; `Rect` is physical pixels,
//! matching how the .NET app scales a layout onto a monitor's work area.

/// A point in layout space.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub fn scale(self, fx: f32, fy: f32) -> Self {
        Self {
            x: self.x * fx,
            y: self.y * fy,
        }
    }
}

/// A size in layout space.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

impl Size {
    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }

    pub fn scale(self, fx: f32, fy: f32) -> Self {
        Self {
            width: self.width * fx,
            height: self.height * fy,
        }
    }
}

/// A physical-pixel rectangle, `right`/`bottom` exclusive.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub const fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    pub const fn from_size(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self {
            left: x,
            top: y,
            right: x + width,
            bottom: y + height,
        }
    }

    pub fn width(self) -> i32 {
        self.right - self.left
    }

    pub fn height(self) -> i32 {
        self.bottom - self.top
    }

    pub fn is_empty(self) -> bool {
        self.right <= self.left || self.bottom <= self.top
    }

    /// Inclusive on the top/left edges, exclusive on bottom/right.
    pub fn contains(self, x: i32, y: i32) -> bool {
        x >= self.left && x < self.right && y >= self.top && y < self.bottom
    }

    pub fn offset(self, dx: i32, dy: i32) -> Self {
        Self {
            left: self.left + dx,
            top: self.top + dy,
            right: self.right + dx,
            bottom: self.bottom + dy,
        }
    }

    /// Shrinks the rectangle on every side. Returns an empty rect when the
    /// padding consumes it.
    pub fn inset(self, dx: i32, dy: i32) -> Self {
        let rect = Self {
            left: self.left + dx,
            top: self.top + dy,
            right: self.right - dx,
            bottom: self.bottom - dy,
        };
        if rect.is_empty() {
            Self::default()
        } else {
            rect
        }
    }

    /// Expands the rectangle on every side.
    pub fn expand(self, dx: i32, dy: i32) -> Self {
        Self {
            left: self.left - dx,
            top: self.top - dy,
            right: self.right + dx,
            bottom: self.bottom + dy,
        }
    }
}

/// A line of a layout: a vertical or horizontal split.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "PascalCase")]
#[derive(Default)]
pub enum SplitDirection {
    #[default]
    Vertical,
    Horizontal,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_contains_uses_exclusive_edges() {
        let rect = Rect::new(10, 10, 20, 20);
        assert!(rect.contains(10, 10));
        assert!(rect.contains(19, 19));
        assert!(!rect.contains(20, 19));
        assert!(!rect.contains(19, 20));
    }

    #[test]
    fn inset_collapses_to_empty() {
        let rect = Rect::new(0, 0, 10, 10);
        assert!(rect.inset(5, 5).is_empty());
        assert_eq!(rect.inset(1, 2), Rect::new(1, 2, 9, 8));
    }
}
