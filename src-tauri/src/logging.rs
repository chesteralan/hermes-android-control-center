use std::path::Path;
use std::sync::OnceLock;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{fmt, reload, EnvFilter, Registry};

use crate::config::LogLevelSetting;
use crate::state::LogLevelSetter;

static GUARD: OnceLock<WorkerGuard> = OnceLock::new();

fn directive(level: LogLevelSetting) -> String {
    let l = match level {
        LogLevelSetting::Error => "error",
        LogLevelSetting::Warn => "warn",
        LogLevelSetting::Info => "info",
        LogLevelSetting::Debug => "debug",
    };
    // Keep third-party crates quieter than the app itself.
    format!("warn,hacc_lib={l}")
}

/// Structured logs to stdout and a daily-rolling file. Returns a runtime level setter.
pub fn init(log_dir: &Path, level: LogLevelSetting) -> LogLevelSetter {
    let (filter, handle) = reload::Layer::new(EnvFilter::new(directive(level)));
    let file_layer = std::fs::create_dir_all(log_dir).ok().map(|_| {
        let appender = tracing_appender::rolling::Builder::new()
            .rotation(tracing_appender::rolling::Rotation::DAILY)
            .filename_prefix("hacc")
            .filename_suffix("log")
            .max_log_files(7)
            .build(log_dir)
            .ok();
        appender.map(|a| {
            let (writer, guard) = tracing_appender::non_blocking(a);
            let _ = GUARD.set(guard);
            fmt::layer().with_writer(writer).with_ansi(false)
        })
    });
    let _ = Registry::default()
        .with(filter)
        .with(fmt::layer())
        .with(file_layer.flatten())
        .try_init();
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        tracing::error!(panic = %info, "panic");
        default_hook(info);
    }));
    Box::new(move |lvl| {
        let _ = handle.modify(|f| *f = EnvFilter::new(directive(lvl)));
    })
}
