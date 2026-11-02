//! One layered overlay window per active monitor.

use anyhow::Result;
use windows::Win32::Foundation::HWND;
use windows::core::w;

use crate::geometry::Rect;
use crate::render;
use crate::settings::{Layout, Theme};
use crate::win32::{self, LayerSurface, Monitor};
use crate::zones::{MonitorZones, ZoneRef};

pub struct Overlay {
    pub hwnd: HWND,
    pub monitor: Monitor,
    pub zones: MonitorZones,
    surface: LayerSurface,
    visible: bool,
}

impl Overlay {
    pub fn new(monitor: Monitor, layout: Option<&Layout>, theme: &Theme) -> Result<Self> {
        let work = monitor.work;
        let zones = MonitorZones::build_scaled(
            work.width(),
            work.height(),
            layout,
            monitor.dpi as f32 / 96.0,
        );
        let hwnd = win32::create_overlay_window(w!("SnapItRustOverlay"), work)?;

        let surface =
            match LayerSurface::new(hwnd, (work.left, work.top), work.width(), work.height()) {
                Ok(surface) => surface,
                Err(error) => {
                    win32::destroy_window(hwnd);
                    return Err(error);
                }
            };

        let mut overlay = Self {
            hwnd,
            monitor,
            zones,
            surface,
            visible: false,
        };
        overlay.paint(theme, None)?;
        Ok(overlay)
    }

    pub fn paint(&mut self, theme: &Theme, hover: Option<ZoneRef>) -> Result<()> {
        // The .NET controls express border thickness in DIPs; our surface is
        // in physical pixels, so scale it by the monitor DPI.
        let mut theme = theme.clone();
        theme.border_thickness *= self.monitor.dpi as f64 / 96.0;

        let pixmap = render::render(
            self.monitor.work.width() as u32,
            self.monitor.work.height() as u32,
            &theme,
            &self.zones,
            hover,
        );
        self.surface.present(&pixmap)
    }

    pub fn show(&mut self) {
        win32::show_overlay(self.hwnd);
        self.visible = true;
    }

    pub fn hide(&mut self) {
        win32::hide_overlay(self.hwnd);
        self.visible = false;
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// `x`/`y` are screen coordinates.
    pub fn hit_test(&self, x: i32, y: i32, current_overlay: Option<usize>) -> Option<ZoneRef> {
        let local_x = x - self.monitor.work.left;
        let local_y = y - self.monitor.work.top;
        if local_x < 0
            || local_y < 0
            || local_x >= self.monitor.work.width()
            || local_y >= self.monitor.work.height()
        {
            return None;
        }
        self.zones.hit_test(local_x, local_y, current_overlay)
    }

    /// Converts a local zone target to screen coordinates.
    pub fn target(&self, zone: ZoneRef) -> Option<Rect> {
        self.zones
            .target(zone)
            .map(|rect| rect.offset(self.monitor.work.left, self.monitor.work.top))
    }
}

impl Drop for Overlay {
    fn drop(&mut self) {
        win32::destroy_window(self.hwnd);
    }
}
