use std::path::Path;
use tracing::Level;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

pub fn init(log_dir: &Path) -> WorkerGuard {
    let _ = std::fs::create_dir_all(log_dir);
    let appender = tracing_appender::rolling::daily(log_dir, "desktop.log");
    let (writer, guard) = tracing_appender::non_blocking(appender);

    let env = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));

    let json_layer = fmt::layer()
        .json()
        .with_current_span(false)
        .with_writer(writer);

    let stderr_layer = fmt::layer()
        .with_target(false)
        .with_writer(std::io::stderr)
        .with_filter(EnvFilter::new("warn"));

    tracing_subscriber::registry()
        .with(env)
        .with(json_layer)
        .with(stderr_layer)
        .init();

    tracing::event!(Level::INFO, "logging initialized");
    guard
}
