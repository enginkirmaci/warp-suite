//! Shared file logging: devtools-style readable lines into a daily-rolling
//! file under an app's logs dir, plus stderr. Best effort — an unwritable
//! dir keeps stderr-only logging.

use std::path::Path;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

/// Log to `<dir>/<prefix>.log.<date>` and stderr, filtered by `RUST_LOG` or
/// `info`. Returns the worker guard the app must hold for the life of the
/// process, or None when the dir was not creatable.
pub fn init(dir: &Path, file_prefix: &str) -> Option<tracing_appender::non_blocking::WorkerGuard> {
    let file_layer = std::fs::create_dir_all(dir).ok().map(|_| {
        let appender = tracing_appender::rolling::daily(dir, format!("{file_prefix}.log"));
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
