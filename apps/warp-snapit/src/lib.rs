//! SnapIt — a Windows zone snapper, ported from the .NET `Tools.SnapIt`
//! companion (vendored in Warp Station). The .NET app stays as-is; this crate is
//! a separate, standalone Rust implementation that reads the same settings
//! and layout files.
//!
//! The shared modules (geometry, settings, zone math, rendering) compile on
//! every platform so they can be unit tested on Linux. The Windows engine is
//! gated behind `cfg(windows)`.

pub mod cli;
pub mod geometry;
pub mod log;
pub mod render;
pub mod settings;
pub mod zones;

#[cfg(windows)]
mod app;
#[cfg(windows)]
mod cornering;
#[cfg(windows)]
mod engine;
#[cfg(windows)]
mod hooks;
#[cfg(windows)]
mod overlay;
#[cfg(windows)]
mod parent_watch;
#[cfg(windows)]
mod win32;

#[cfg(windows)]
pub use app::run;
