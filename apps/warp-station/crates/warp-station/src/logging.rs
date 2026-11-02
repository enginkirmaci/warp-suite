//! File logging: readable lines into a daily-rolling file under ~/.warp/logs.
//! Best effort — an unwritable home keeps stderr logging.

use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use warp_station_core::paths::AppPaths;

pub fn initialize() -> Option<tracing_appender::non_blocking::WorkerGuard> {
    let file_layer = std::fs::create_dir_all(AppPaths::logs_dir()).ok().map(|_| {
        let appender = tracing_appender::rolling::daily(AppPaths::logs_dir(), "warp-station-.log");
        let (writer, guard) = tracing_appender::non_blocking(appender);
        (
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(writer),
            guard,
        )
    });

    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    let registry = tracing_subscriber::registry().with(filter).with(
        tracing_subscriber::fmt::layer()
            .with_ansi(false)
            .with_writer(std::io::stderr),
    );
    match file_layer {
        Some((layer, guard)) => {
            registry.with(layer).init();
            Some(guard)
        }
        None => {
            registry.init();
            None
        }
    }
}
