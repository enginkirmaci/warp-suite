//! The Warp Station app crate as a library: the binary in main.rs is a thin shell,
//! so headless UI probes can exercise the real views.

pub mod assets;
pub mod ipc;
pub mod logging;
pub mod net;
pub mod notices;
pub mod store;
pub mod theme;
pub mod ui;
