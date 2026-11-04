//! File logging: readable lines into a daily-rolling file under ~/.warp/logs.
//! Best effort — an unwritable home keeps stderr logging.

use warp_station_core::paths::AppPaths;

pub fn initialize() -> Option<tracing_appender::non_blocking::WorkerGuard> {
    warpui_kit_platform::logging::init(&AppPaths::logs_dir(), "warp-station-")
}
