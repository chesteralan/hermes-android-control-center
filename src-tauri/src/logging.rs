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

fn open_log_file(
    log_dir: &Path,
) -> std::io::Result<tracing_appender::rolling::RollingFileAppender> {
    std::fs::create_dir_all(log_dir)?;
    tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("hacc")
        .filename_suffix("log")
        .max_log_files(7)
        .build(log_dir)
        .map_err(|error| std::io::Error::other(error.to_string()))
}

/// Structured logs to stdout and a daily-rolling file. Returns a runtime level setter.
pub fn init(log_dir: &Path, level: LogLevelSetting) -> LogLevelSetter {
    let (filter, handle) = reload::Layer::new(EnvFilter::new(directive(level)));
    let file_layer = match open_log_file(log_dir) {
        Ok(appender) => {
            let (writer, guard) = tracing_appender::non_blocking(appender);
            let _ = GUARD.set(guard);
            Some(fmt::layer().with_writer(writer).with_ansi(false))
        }
        Err(error) => {
            eprintln!("File logging unavailable: {error}. Continuing with console logging.");
            None
        }
    };
    if let Err(error) = Registry::default()
        .with(filter)
        .with(fmt::layer())
        .with(file_layer)
        .try_init()
    {
        eprintln!("Could not install application logging subscriber: {error}");
    }
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        tracing::error!(panic = %info, "panic");
        default_hook(info);
    }));
    Box::new(move |lvl| {
        if let Err(error) = handle.modify(|f| *f = EnvFilter::new(directive(lvl))) {
            eprintln!("Could not change application log level: {error}");
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temporary_path() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "hacc-logging-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ))
    }

    #[test]
    fn file_logger_creates_directory_and_writes_expected_daily_file() {
        let directory = temporary_path();
        let mut appender = open_log_file(&directory).unwrap();
        appender.write_all(b"synthetic log entry\n").unwrap();
        appender.flush().unwrap();
        drop(appender);
        let files = std::fs::read_dir(&directory)
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(files.len(), 1);
        let name = files[0].file_name().to_string_lossy().into_owned();
        assert!(name.starts_with("hacc."));
        assert!(name.ends_with(".log"));
        assert_eq!(
            std::fs::read(files[0].path()).unwrap(),
            b"synthetic log entry\n"
        );
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn file_logger_returns_setup_errors_without_panicking() {
        let path = temporary_path();
        std::fs::write(&path, b"existing file").unwrap();
        let result = std::panic::catch_unwind(|| open_log_file(&path));
        assert!(result.is_ok());
        assert!(result.unwrap().is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"existing file");
        std::fs::remove_file(path).unwrap();
    }
}
