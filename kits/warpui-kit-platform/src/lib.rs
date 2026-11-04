//! warpui-kit-platform: the platform plumbing the suite apps had each re-grown
//! by hand — atomic file replace, file logging, the notes store's shared read
//! side — plus, under the `assets` feature, the rust-embed-over-gpui-kit
//! `AssetSource`.
//!
//! Unlike the rest of the family this crate carries no design system; it
//! exists so the per-app copies (which had already drifted in their error
//! paths) collapse into one tested implementation. It builds without gpui
//! (`default-features = false`) so headless service crates can take the
//! io/logging halves.

pub mod io;
pub mod logging;
pub mod notes;

#[cfg(feature = "assets")]
pub mod assets;
