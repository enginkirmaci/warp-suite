//! Zone math: port of the .NET app's `Math/FindRectangle` algorithm plus the
//! layout-to-monitor scaling used by `SnapControl`.
//!
//! A layout is a set of split lines. The rectangle finder turns those lines
//! (and the four screen edges) into non-overlapping snap areas. Layout
//! overlays are additional, explicitly-positioned merged zones drawn on top.

use std::collections::{HashMap, HashSet};

use crate::geometry::{Rect, SplitDirection};
use crate::settings::Layout;

const TOP: u8 = 1;
const BOTTOM: u8 = 2;
const LEFT: u8 = 4;
const RIGHT: u8 = 8;

/// A merged overlay zone: `full` is the snap target, `mini` is the normal
/// state preview rectangle (the .NET `SnapOverlay` / `SnapFullOverlay` pair).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OverlayZone {
    pub full: Rect,
    pub mini: Rect,
}

/// A scaled layout split line, used to draw the .NET `SnapBorder` bands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SplitLine {
    pub start: (i32, i32),
    pub end: (i32, i32),
    pub direction: SplitDirection,
}

/// Which zone the pointer is over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZoneRef {
    Area(usize),
    Overlay(usize),
}

/// Zones for one monitor, in coordinates local to its work area.
#[derive(Clone, Debug, Default)]
pub struct MonitorZones {
    pub areas: Vec<Rect>,
    pub overlays: Vec<OverlayZone>,
    pub lines: Vec<SplitLine>,
    /// Physical width of the translucent split-line band (12 DIP in .NET).
    pub line_band: i32,
    /// Physical width of the 1 DIP reference line inside the band.
    pub line_width: i32,
}

impl MonitorZones {
    pub fn build(width: i32, height: i32, layout: Option<&Layout>) -> Self {
        Self::build_scaled(width, height, layout, 1.0)
    }

    /// `dpi_scale` is the monitor scale factor (96 DPI = 1.0). The .NET app
    /// keeps `AreaPadding` in DIPs while layout lines scale with the work
    /// area, so padding must be scaled by DPI, not by the layout factor.
    pub fn build_scaled(width: i32, height: i32, layout: Option<&Layout>, dpi_scale: f32) -> Self {
        let dpi_scale = dpi_scale.max(0.0);
        let line_band = (12.0 * dpi_scale).round().max(1.0) as i32;
        let line_width = dpi_scale.round().max(1.0) as i32;
        let full = Rect::from_size(0, 0, width.max(0), height.max(0));
        let Some(layout) = layout else {
            return Self {
                areas: vec![full],
                overlays: Vec::new(),
                lines: Vec::new(),
                line_band,
                line_width,
            };
        };

        let (fx, fy) = layout.scaled_lines(width, height);
        let mut segments = Vec::with_capacity(layout.layout_lines.len());

        for line in &layout.layout_lines {
            let start = line.point.scale(fx, fy);
            let size = line.size.scale(fx, fy);
            let (a, b) = match line.split_direction {
                SplitDirection::Vertical => (
                    (start.x as i32, start.y as i32),
                    (start.x as i32, (start.y + size.height) as i32),
                ),
                SplitDirection::Horizontal => (
                    (start.x as i32, start.y as i32),
                    ((start.x + size.width) as i32, start.y as i32),
                ),
            };
            segments.push(Segment {
                a,
                b,
                orientation: line.split_direction,
            });
        }

        let lines = segments
            .iter()
            .map(|segment| SplitLine {
                start: segment.a,
                end: segment.b,
                direction: segment.orientation,
            })
            .collect();

        let mut areas = find_rectangles(width.max(0), height.max(0), &segments);
        let pad_x = (layout.area_padding * dpi_scale as f64).round() as i32;
        let pad_y = (layout.area_padding * dpi_scale as f64).round() as i32;
        if pad_x != 0 || pad_y != 0 {
            areas = areas
                .into_iter()
                .map(|rect| rect.inset(pad_x, pad_y))
                .filter(|rect| !rect.is_empty())
                .collect();
        }

        // Deterministic order: the .NET HashSet order is arbitrary and nothing
        // depends on it, but stable output keeps tests and dumps readable.
        areas.sort_by_key(|rect| (rect.top, rect.left));

        let mut overlays = Vec::with_capacity(layout.layout_overlays.len());
        for overlay in &layout.layout_overlays {
            let full = Rect::from_size(
                (overlay.point.x * fx) as i32,
                (overlay.point.y * fy) as i32,
                (overlay.size.width * fx) as i32,
                (overlay.size.height * fy) as i32,
            );
            if full.is_empty() {
                continue;
            }

            let mini = match &overlay.mini_overlay {
                Some(mini) => Rect::from_size(
                    (mini.point.x * fx) as i32,
                    (mini.point.y * fy) as i32,
                    (mini.size.width * fx) as i32,
                    (mini.size.height * fy) as i32,
                ),
                None => {
                    // SnapOverlay.SetPos(null): 30% of the full zone, centred.
                    let width = (full.width() as f32 * 0.3) as i32;
                    let height = (full.height() as f32 * 0.3) as i32;
                    Rect::from_size(
                        full.left + (full.width() - width) / 2,
                        full.top + (full.height() - height) / 2,
                        width,
                        height,
                    )
                }
            };

            overlays.push(OverlayZone { full, mini });
        }

        Self {
            areas,
            overlays,
            lines,
            line_band,
            line_width,
        }
    }

    /// Visual hit test. Overlays are drawn above areas, so they win; among
    /// mini overlays the last one added is on top. All mini overlays sit above
    /// every full overlay in the .NET visual tree (`MainFullOverlay` is below
    /// `MainOverlay`), so minis are tested first. `current_overlay` keeps a
    /// hovered merged zone selected while the pointer is anywhere in its full
    /// area.
    pub fn hit_test(&self, x: i32, y: i32, current_overlay: Option<usize>) -> Option<ZoneRef> {
        for (index, overlay) in self.overlays.iter().enumerate().rev() {
            if overlay.mini.contains(x, y) {
                return Some(ZoneRef::Overlay(index));
            }
        }

        if let Some(index) = current_overlay
            && let Some(overlay) = self.overlays.get(index)
            && overlay.full.contains(x, y)
        {
            return Some(ZoneRef::Overlay(index));
        }

        self.areas
            .iter()
            .position(|area| area.contains(x, y))
            .map(ZoneRef::Area)
    }

    pub fn target(&self, zone: ZoneRef) -> Option<Rect> {
        match zone {
            ZoneRef::Area(index) => self.areas.get(index).copied(),
            ZoneRef::Overlay(index) => self.overlays.get(index).map(|overlay| overlay.full),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.areas.is_empty() && self.overlays.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Rectangle finder (Cyotek Slice Rectangle Sample, as used by SnapIt)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Segment {
    a: (i32, i32),
    b: (i32, i32),
    orientation: SplitDirection,
}

fn find_rectangles(width: i32, height: i32, extra: &[Segment]) -> Vec<Rect> {
    let mut segments = Vec::with_capacity(extra.len() + 4);
    segments.push(Segment {
        a: (0, 0),
        b: (width, 0),
        orientation: SplitDirection::Horizontal,
    });
    segments.push(Segment {
        a: (0, height),
        b: (width, height),
        orientation: SplitDirection::Horizontal,
    });
    segments.push(Segment {
        a: (0, 0),
        b: (0, height),
        orientation: SplitDirection::Vertical,
    });
    segments.push(Segment {
        a: (width, 0),
        b: (width, height),
        orientation: SplitDirection::Vertical,
    });
    segments.extend_from_slice(extra);

    fix_points(&mut segments);
    segments.sort_by_key(|segment| (segment.a.0, segment.a.1));

    let points = calculate_points(&segments);
    calculate_rectangles(&points)
}

fn fix_points(segments: &mut [Segment]) {
    let mut xs = Vec::with_capacity(segments.len() * 2);
    let mut ys = Vec::with_capacity(segments.len() * 2);
    for segment in segments.iter() {
        xs.push(segment.a.0);
        ys.push(segment.a.1);
        xs.push(segment.b.0);
        ys.push(segment.b.1);
    }

    const TOLERANCE: i32 = 4;
    let in_range =
        |value: i32, candidate: i32| value - TOLERANCE < candidate && candidate < value + TOLERANCE;

    for segment in segments.iter_mut().skip(4) {
        let location_x = xs.iter().copied().find(|&x| in_range(segment.a.0, x));
        let location_y = ys.iter().copied().find(|&y| in_range(segment.a.1, y));
        let end_x = xs.iter().copied().find(|&x| in_range(segment.b.0, x));
        let end_y = ys.iter().copied().find(|&y| in_range(segment.b.1, y));

        segment.a = (
            location_x.unwrap_or(segment.a.0),
            location_y.unwrap_or(segment.a.1),
        );
        segment.b = (end_x.unwrap_or(segment.b.0), end_y.unwrap_or(segment.b.1));
    }
}

fn update_point(points: &mut HashMap<(i32, i32), u8>, location: (i32, i32), flags: u8) {
    points
        .entry(location)
        .and_modify(|connections| *connections |= flags)
        .or_insert(flags);
}

fn calculate_points(segments: &[Segment]) -> HashMap<(i32, i32), u8> {
    let mut points = HashMap::new();

    for (index, segment) in segments.iter().enumerate() {
        let start_flags = match segment.orientation {
            SplitDirection::Horizontal => LEFT,
            SplitDirection::Vertical => TOP,
        };
        let end_flags = match segment.orientation {
            SplitDirection::Horizontal => RIGHT,
            SplitDirection::Vertical => BOTTOM,
        };
        update_point(&mut points, segment.a, start_flags);
        update_point(&mut points, segment.b, end_flags);

        for (other_index, other) in segments.iter().enumerate() {
            if index == other_index {
                continue;
            }

            let Some(intersection) = find_line_intersection(segment.a, segment.b, other.a, other.b)
            else {
                continue;
            };

            // The original treats Point.Empty (0,0) as "no intersection".
            if intersection == (0, 0) {
                continue;
            }

            let mut flags = 0;
            if intersection != segment.a && intersection != segment.b {
                flags |= match segment.orientation {
                    SplitDirection::Horizontal => LEFT | RIGHT,
                    SplitDirection::Vertical => TOP | BOTTOM,
                };
            } else if intersection != other.a && intersection != other.b {
                flags |= match other.orientation {
                    SplitDirection::Horizontal => LEFT | RIGHT,
                    SplitDirection::Vertical => TOP | BOTTOM,
                };
            }

            if flags != 0 {
                update_point(&mut points, intersection, flags);
            }
        }
    }

    points
}

fn calculate_rectangles(points: &HashMap<(i32, i32), u8>) -> Vec<Rect> {
    let mut horizontal: Vec<((i32, i32), u8)> = points
        .iter()
        .map(|(point, flags)| (*point, *flags))
        .collect();
    horizontal.sort_by_key(|(point, _)| point.0);

    let mut vertical = horizontal.clone();
    vertical.sort_by_key(|(point, _)| point.1);

    let mut by_coord = HashMap::with_capacity(points.len());
    for (point, flags) in points {
        by_coord.insert(*point, *flags);
    }

    let mut seen = HashSet::new();
    let mut rectangles = Vec::new();

    let mut top_lefts: Vec<((i32, i32), u8)> = points
        .iter()
        .map(|(point, flags)| (*point, *flags))
        .collect();
    top_lefts.sort_by_key(|(point, _)| (point.0, point.1));

    for (top_left, flags) in top_lefts {
        if flags & (LEFT | TOP) != LEFT | TOP {
            continue;
        }

        let top_right = horizontal.iter().find(|((x, y), other)| {
            *x > top_left.0 && *y == top_left.1 && (*other & (RIGHT | TOP) == RIGHT | TOP)
        });

        let Some((top_right, _)) = top_right else {
            continue;
        };

        let bottom_left = vertical.iter().find(|((x, y), other)| {
            *x == top_left.0 && *y > top_left.1 && (*other & (LEFT | BOTTOM) == LEFT | BOTTOM)
        });

        let Some((bottom_left, _)) = bottom_left else {
            continue;
        };

        let bottom_right = (top_right.0, bottom_left.1);
        let Some(bottom_right_flags) = by_coord.get(&bottom_right) else {
            continue;
        };

        if *bottom_right_flags & (RIGHT | BOTTOM) != RIGHT | BOTTOM {
            continue;
        }

        let rect = Rect::new(top_left.0, top_left.1, bottom_right.0, bottom_right.1);
        if !rect.is_empty() && seen.insert((rect.left, rect.top, rect.right, rect.bottom)) {
            rectangles.push(rect);
        }
    }

    rectangles
}

/// Integer line intersection, matching the original C# implementation
/// (including truncating integer division).
fn find_line_intersection(
    start1: (i32, i32),
    end1: (i32, i32),
    start2: (i32, i32),
    end2: (i32, i32),
) -> Option<(i32, i32)> {
    let denom =
        (end1.0 - start1.0) * (end2.1 - start2.1) - (end1.1 - start1.1) * (end2.0 - start2.0);
    if denom == 0 {
        return None;
    }

    let numer =
        (start1.1 - start2.1) * (end2.0 - start2.0) - (start1.0 - start2.0) * (end2.1 - start2.1);
    let r = numer / denom;

    let numer2 =
        (start1.1 - start2.1) * (end1.0 - start1.0) - (start1.0 - start2.0) * (end1.1 - start1.1);
    let s = numer2 / denom;

    if (0..=1).contains(&r) && (0..=1).contains(&s) {
        Some((
            start1.0 + r * (end1.0 - start1.0),
            start1.1 + r * (end1.1 - start1.1),
        ))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shipped_layout() -> Layout {
        serde_json::from_str(include_str!(
            "../packaging/windows/settings/snapit/layouts/3PartHorizontal.json"
        ))
        .expect("layout parse")
    }

    #[test]
    fn no_layout_is_one_full_screen_area() {
        let zones = MonitorZones::build(1920, 1040, None);
        assert_eq!(zones.areas, vec![Rect::new(0, 0, 1920, 1040)]);
        assert!(zones.overlays.is_empty());
    }

    #[test]
    fn three_part_layout_produces_five_areas() {
        let layout = shipped_layout();
        let zones = MonitorZones::build(2752, 1104, Some(&layout));

        assert_eq!(zones.areas.len(), 5, "areas: {:?}", zones.areas);
        assert_eq!(
            zones.areas,
            vec![
                Rect::new(0, 0, 917, 552),
                Rect::new(917, 0, 1834, 1104),
                Rect::new(1834, 0, 2752, 552),
                Rect::new(0, 552, 917, 1104),
                Rect::new(1834, 552, 2752, 1104),
            ]
        );
        assert_eq!(zones.overlays.len(), 6);
    }

    #[test]
    fn layout_scales_to_other_resolutions() {
        let layout = shipped_layout();
        let zones = MonitorZones::build(1376, 552, Some(&layout));
        assert_eq!(zones.areas.len(), 5);
        assert_eq!(zones.areas[1], Rect::new(458, 0, 917, 552));
        assert_eq!(zones.areas[4], Rect::new(917, 276, 1376, 552));
    }

    #[test]
    fn area_padding_shrinks_zones() {
        let mut layout = shipped_layout();
        layout.area_padding = 10.0;
        let zones = MonitorZones::build(2752, 1104, Some(&layout));
        assert_eq!(zones.areas[0], Rect::new(10, 10, 907, 542));
    }

    #[test]
    fn area_padding_follows_dpi_scale() {
        let mut layout = shipped_layout();
        layout.area_padding = 10.0;
        let zones = MonitorZones::build_scaled(2752, 1104, Some(&layout), 1.5);
        assert_eq!(zones.areas[0], Rect::new(15, 15, 902, 537));
    }

    #[test]
    fn overlays_win_over_areas_and_full_area_keeps_hover() {
        let layout = shipped_layout();
        let zones = MonitorZones::build(2752, 1104, Some(&layout));

        // The first mini overlay sits inside the left third.
        let mini = zones.overlays[0].mini;
        let x = mini.left + mini.width() / 2;
        let y = mini.top + mini.height() / 2;
        assert_eq!(zones.hit_test(x, y, None), Some(ZoneRef::Overlay(0)));

        // A point in the full overlay but outside the mini only stays selected
        // while that overlay is the current one.
        let full = zones.overlays[0].full;
        let x = full.left + 5;
        let y = full.bottom - 5;
        assert_eq!(zones.hit_test(x, y, None), Some(ZoneRef::Area(3)));
        assert_eq!(zones.hit_test(x, y, Some(0)), Some(ZoneRef::Overlay(0)));
    }

    #[test]
    fn mini_overlays_are_above_any_full_overlay() {
        let zones = MonitorZones {
            areas: vec![Rect::new(0, 0, 100, 100)],
            overlays: vec![
                OverlayZone {
                    full: Rect::new(0, 0, 50, 50),
                    mini: Rect::new(40, 40, 45, 45),
                },
                OverlayZone {
                    full: Rect::new(0, 0, 100, 100),
                    mini: Rect::new(10, 10, 20, 20),
                },
            ],
            ..MonitorZones::default()
        };

        // (42, 42) is inside overlay 0's mini and overlay 1's (current) full
        // area; the mini is visually on top in the .NET control tree.
        assert_eq!(zones.hit_test(42, 42, Some(1)), Some(ZoneRef::Overlay(0)));
    }

    #[test]
    fn empty_layout_still_yields_the_screen_area() {
        let layout = Layout::default();
        let zones = MonitorZones::build(800, 600, Some(&layout));
        assert_eq!(zones.areas, vec![Rect::new(0, 0, 800, 600)]);
    }
}
