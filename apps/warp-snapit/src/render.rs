//! Software renderer for the overlay windows.
//!
//! The .NET app composes Avalonia controls; the Rust port draws the same
//! shapes into a premultiplied RGBA pixmap with tiny-skia. The Windows layer
//! copies that pixmap into a 32-bpp DIB and hands it to `UpdateLayeredWindow`.

use tiny_skia::{
    Color as SkColor, FillRule, LineCap, Paint, PathBuilder, Pixmap, Rect as SkRect, Stroke,
    Transform,
};

use crate::geometry::{Rect, SplitDirection};
use crate::settings::{Color, Theme};
use crate::zones::{MonitorZones, SplitLine, ZoneRef};

fn sk_color(color: Color) -> SkColor {
    SkColor::from_rgba8(color.r, color.g, color.b, color.a)
}

fn paint(color: Color) -> Paint<'static> {
    let mut paint = Paint::default();
    paint.set_color(sk_color(color));
    paint.anti_alias = true;
    paint
}

fn to_sk_rect(rect: Rect) -> Option<SkRect> {
    SkRect::from_ltrb(
        rect.left as f32,
        rect.top as f32,
        rect.right as f32,
        rect.bottom as f32,
    )
}

fn fill(pixmap: &mut Pixmap, rect: Rect, color: Color) {
    if rect.is_empty() || color.a == 0 {
        return;
    }
    let Some(rect) = to_sk_rect(rect) else {
        return;
    };
    pixmap.fill_rect(rect, &paint(color), Transform::identity(), None);
}

/// Draws a border inside `rect` using four filled strips (crisp at any size).
fn border(pixmap: &mut Pixmap, rect: Rect, color: Color, thickness: f32) {
    let thickness = thickness.max(0.0).round() as i32;
    if thickness == 0 || rect.is_empty() || color.a == 0 {
        return;
    }

    let t = thickness;
    let width = rect.width();
    let height = rect.height();
    if width <= 2 * t || height <= 2 * t {
        // Too small for four strips: a plain outline is the best we can do.
        fill(pixmap, rect, color);
        return;
    }

    fill(
        pixmap,
        Rect::new(rect.left, rect.top, rect.right, rect.top + t),
        color,
    );
    fill(
        pixmap,
        Rect::new(rect.left, rect.bottom - t, rect.right, rect.bottom),
        color,
    );
    fill(
        pixmap,
        Rect::new(rect.left, rect.top + t, rect.left + t, rect.bottom - t),
        color,
    );
    fill(
        pixmap,
        Rect::new(rect.right - t, rect.top + t, rect.right, rect.bottom - t),
        color,
    );
}

/// The merged-zone preview icon: a double-headed diagonal resize arrow.
fn merge_icon(pixmap: &mut Pixmap, rect: Rect, color: Color) {
    if rect.is_empty() || color.a == 0 {
        return;
    }

    let size = (rect.width().max(rect.height()) as f32 * 0.15).max(6.0);
    if size < 4.0 || rect.width() < 8 || rect.height() < 8 {
        return;
    }

    let cx = rect.left as f32 + rect.width() as f32 / 2.0;
    let cy = rect.top as f32 + rect.height() as f32 / 2.0;

    // Unit diagonal from bottom-left to top-right.
    let diagonal = std::f32::consts::FRAC_1_SQRT_2;
    let dx = diagonal;
    let dy = -diagonal;

    let half_length = size * 0.42;
    let head_length = size * 0.24;
    let head_half = size * 0.17;

    let start = (cx - dx * half_length, cy - dy * half_length);
    let end = (cx + dx * half_length, cy + dy * half_length);

    let mut builder = PathBuilder::new();
    builder.move_to(start.0, start.1);
    builder.line_to(end.0, end.1);
    if let Some(path) = builder.finish() {
        let stroke = Stroke {
            width: size * 0.11,
            line_cap: LineCap::Butt,
            ..Stroke::default()
        };
        pixmap.stroke_path(&path, &paint(color), &stroke, Transform::identity(), None);
    }

    let mut head = |tip: (f32, f32), direction: (f32, f32)| {
        let base = (
            tip.0 - direction.0 * head_length,
            tip.1 - direction.1 * head_length,
        );
        let perp = (-direction.1 * head_half, direction.0 * head_half);

        let mut builder = PathBuilder::new();
        builder.move_to(tip.0, tip.1);
        builder.line_to(base.0 + perp.0, base.1 + perp.1);
        builder.line_to(base.0 - perp.0, base.1 - perp.1);
        builder.close();
        if let Some(path) = builder.finish() {
            pixmap.fill_path(
                &path,
                &paint(color),
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
    };

    head(start, (-dx, -dy));
    head(end, (dx, dy));
}

fn draw_zone(
    pixmap: &mut Pixmap,
    rect: Rect,
    fill_color: Color,
    border_color: Color,
    border_thickness: f32,
    icon: bool,
) {
    fill(pixmap, rect, fill_color);
    border(pixmap, rect, border_color, border_thickness);
    if icon {
        merge_icon(pixmap, rect, border_color);
    }
}

/// The 12-DIP `SnapBorder` band that sits on every layout split line.
fn line_band_rect(line: SplitLine, band: i32) -> Rect {
    let half = (band / 2).max(0);
    let (x1, y1) = line.start;
    let (x2, y2) = line.end;

    match line.direction {
        SplitDirection::Vertical => Rect::new(x1 - half, y1.min(y2), x1 + half, y1.max(y2)),
        SplitDirection::Horizontal => Rect::new(x1.min(x2), y1 - half, x1.max(x2), y1 + half),
    }
}

/// The 1-DIP reference line drawn inside the band (`ReferenceBorder`).
fn line_reference_rect(line: SplitLine, width: i32) -> Rect {
    let width = width.max(1);
    let (x1, y1) = line.start;
    let (x2, y2) = line.end;

    match line.direction {
        SplitDirection::Vertical => Rect::new(x1, y1.min(y2), x1 + width, y1.max(y2)),
        SplitDirection::Horizontal => Rect::new(x1.min(x2), y1, x1.max(x2), y1 + width),
    }
}

/// The four non-draggable screen-edge borders. Their 12-DIP bands are
/// positioned half off-screen, so only the inner half is visible.
fn draw_screen_edges(pixmap: &mut Pixmap, width: i32, height: i32, color: Color, band: i32) {
    let half = (band / 2).max(0);
    if half == 0 || width <= 0 || height <= 0 {
        return;
    }

    fill(pixmap, Rect::new(0, 0, width, half), color);
    fill(pixmap, Rect::new(0, height - half, width, height), color);
    fill(pixmap, Rect::new(0, 0, half, height), color);
    fill(pixmap, Rect::new(width - half, 0, width, height), color);
}

/// Renders one monitor's overlay. `hover` refers to `zones`.
pub fn render(
    width: u32,
    height: u32,
    theme: &Theme,
    zones: &MonitorZones,
    hover: Option<ZoneRef>,
) -> Pixmap {
    let mut pixmap = Pixmap::new(width.max(1), height.max(1)).expect("pixmap allocation");

    let overlay = theme.overlay_color.with_opacity(theme.opacity);
    let highlight = theme.highlight_color.with_opacity(theme.opacity);
    let border = theme.border_color.with_opacity(theme.opacity);

    // Base snap areas. The hovered one gets the highlight colour instead of
    // the normal overlay colour (matching Avalonia's background swap).
    for (index, area) in zones.areas.iter().enumerate() {
        let hovered = hover == Some(ZoneRef::Area(index));
        draw_zone(
            &mut pixmap,
            *area,
            if hovered { highlight } else { overlay },
            border,
            theme.border_thickness as f32,
            false,
        );
    }

    // The .NET SnapControl draws screen-edge and layout split-line borders
    // above the snap areas but below the merged-zone overlays.
    let width = pixmap.width() as i32;
    let height = pixmap.height() as i32;
    draw_screen_edges(&mut pixmap, width, height, overlay, zones.line_band);
    for line in &zones.lines {
        fill(&mut pixmap, line_band_rect(*line, zones.line_band), overlay);
    }
    for line in &zones.lines {
        fill(
            &mut pixmap,
            line_reference_rect(*line, zones.line_width),
            border,
        );
    }

    // Merged zones. In the .NET visual tree every mini overlay lives in
    // `MainOverlay`, above every full overlay in `MainFullOverlay`, so draw
    // the hovered full area first and all mini previews afterwards.
    if let Some(ZoneRef::Overlay(index)) = hover
        && let Some(zone) = zones.overlays.get(index)
    {
        draw_zone(
            &mut pixmap,
            zone.full,
            highlight,
            border,
            theme.border_thickness as f32,
            false,
        );
    }

    for (index, zone) in zones.overlays.iter().enumerate() {
        if hover == Some(ZoneRef::Overlay(index)) {
            continue;
        }
        draw_zone(
            &mut pixmap,
            zone.mini,
            overlay,
            border,
            theme.border_thickness as f32,
            true,
        );
    }

    pixmap
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Color;

    fn theme() -> Theme {
        Theme {
            highlight_color: Color::new(200, 0, 0, 0),
            overlay_color: Color::new(150, 0, 0, 255),
            border_color: Color::new(200, 150, 150, 150),
            border_thickness: 1.0,
            opacity: 1.0,
        }
    }

    fn pixel(pixmap: &Pixmap, x: u32, y: u32) -> [u8; 4] {
        let offset = ((y * pixmap.width() + x) * 4) as usize;
        let data = pixmap.data();
        [
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]
    }

    #[test]
    fn renders_area_fill_and_border() {
        let zones = MonitorZones::build(200, 100, None);
        let pixmap = render(200, 100, &theme(), &zones, None);

        let center = pixel(&pixmap, 100, 50);
        assert!(center[3] > 0, "centre should be translucent");
        assert!(center[2] > center[0], "overlay colour is blue");

        let corner = pixel(&pixmap, 0, 0);
        assert!(corner[3] > center[3], "border should be more opaque");
    }

    #[test]
    fn hover_switches_to_highlight() {
        let zones = MonitorZones::build(200, 100, None);
        let pixmap = render(200, 100, &theme(), &zones, Some(ZoneRef::Area(0)));
        let center = pixel(&pixmap, 100, 50);
        assert_eq!(center[3], 200);
        assert!(center[0] < 10 && center[1] < 10 && center[2] < 10);
    }

    #[test]
    fn mini_overlays_are_drawn_above_a_hovered_full_overlay() {
        let zones = MonitorZones {
            areas: vec![Rect::new(0, 0, 100, 100)],
            overlays: vec![
                crate::zones::OverlayZone {
                    full: Rect::new(0, 0, 50, 50),
                    mini: Rect::new(40, 40, 45, 45),
                },
                crate::zones::OverlayZone {
                    full: Rect::new(0, 0, 100, 100),
                    mini: Rect::new(10, 10, 20, 20),
                },
            ],
            ..MonitorZones::default()
        };

        let pixmap = render(100, 100, &theme(), &zones, Some(ZoneRef::Overlay(1)));
        let inside_mini = pixel(&pixmap, 42, 42);
        assert!(
            inside_mini[2] > 0,
            "the mini preview must remain above the hovered full overlay"
        );
    }

    #[test]
    fn layout_split_lines_are_drawn() {
        let layout: crate::settings::Layout = serde_json::from_str(include_str!(
            "../packaging/windows/settings/snapit/layouts/3PartHorizontal.json"
        ))
        .unwrap();
        let zones = MonitorZones::build(2752, 1104, Some(&layout));
        let pixmap = render(2752, 1104, &theme(), &zones, None);

        // The first vertical split line is at x = 917.3333.
        let line = pixel(&pixmap, 917, 100);
        let area = pixel(&pixmap, 500, 100);
        assert!(
            line[3] > area[3],
            "split line should be more opaque than the area fill"
        );
    }

    #[test]
    fn overlay_mini_is_drawn() {
        let layout: crate::settings::Layout = serde_json::from_str(include_str!(
            "../packaging/windows/settings/snapit/layouts/3PartHorizontal.json"
        ))
        .unwrap();
        let zones = MonitorZones::build(2752, 1104, Some(&layout));
        let mini = zones.overlays[0].mini;
        let pixmap = render(2752, 1104, &theme(), &zones, None);

        let x = (mini.left + mini.width() / 2) as u32;
        let y = (mini.top + mini.height() / 2) as u32;
        assert!(pixel(&pixmap, x, y)[3] > 0);
    }
}
