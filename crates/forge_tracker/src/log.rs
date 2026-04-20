use std::path::PathBuf;

use tracing::debug;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::prelude::*;
use tracing_subscriber::{self, Layer};

pub fn init_tracing(log_path: PathBuf) -> anyhow::Result<Guard> {
    debug!(path = %log_path.display(), "Initializing logging system in JSON format");

    let append = tracing_appender::rolling::daily(log_path, "forge.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(append);

    let fmt_layer = tracing_subscriber::fmt::layer()
        .json()
        .with_timer(tracing_subscriber::fmt::time::uptime())
        .with_thread_ids(false)
        .with_target(false)
        .with_file(true)
        .with_line_number(true)
        .with_writer(non_blocking)
        .with_filter(
            tracing_subscriber::EnvFilter::try_from_env("FORGE_LOG")
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("forge=debug")),
        );

    tracing_subscriber::registry().with(fmt_layer).init();

    Ok(Guard(guard))
}

pub struct Guard(#[allow(dead_code)] WorkerGuard);
