pub mod adb;
pub mod chat;
pub mod control_api;
pub mod device;
pub mod diagnostics;
pub mod hermes;
pub mod logs;
pub mod provision;
pub mod sessions;
pub mod settings;
pub mod terminal;
pub mod termux;

pub(crate) fn write_export_file(
    path: &std::path::Path,
    content: &[u8],
) -> Result<String, crate::error::AppError> {
    if !path.is_absolute() || path.file_name().is_none() {
        return Err(crate::error::AppError::Config(
            "Choose an absolute file path for export.".into(),
        ));
    }
    std::fs::write(path, content).map_err(|error| crate::error::AppError::Io(error.to_string()))?;
    Ok(path.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::AppError;
    use std::path::Path;

    #[test]
    fn export_writer_rejects_relative_empty_and_root_paths() {
        for path in [
            "",
            "logs.txt",
            "../logs.txt",
            "~/logs.txt",
            "exports/logs.txt",
        ] {
            assert!(matches!(
                write_export_file(Path::new(path), b"data"),
                Err(AppError::Config(_))
            ));
        }
        let temporary = std::env::temp_dir();
        let root = temporary.ancestors().last().unwrap();
        assert!(matches!(
            write_export_file(root, b"data"),
            Err(AppError::Config(_))
        ));
    }

    #[test]
    fn export_writer_preserves_bytes_and_propagates_write_errors() {
        let directory = std::env::temp_dir().join(format!(
            "hacc-export-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("output.txt");
        let bytes = b"stdout\nstderr\n";
        assert_eq!(
            write_export_file(&path, bytes).unwrap(),
            path.to_string_lossy()
        );
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert!(matches!(
            write_export_file(&directory.join("missing/output.txt"), bytes),
            Err(AppError::Io(_)),
        ));
        std::fs::remove_dir_all(directory).unwrap();
    }
}
