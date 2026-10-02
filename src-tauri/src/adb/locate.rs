use std::path::PathBuf;
use std::sync::Arc;

use super::client::AdbClient;
use super::types::AdbInfo;
use crate::error::{AppError, AppResult};
use crate::platform::{self, Env};
use crate::process::ProcessRunner;

/// Ordered list of paths to try: configured → PATH → well-known locations.
pub fn candidate_paths(configured: Option<&PathBuf>, env: &dyn Env) -> Vec<PathBuf> {
    let os = platform::current_os();
    let mut out = Vec::new();
    if let Some(p) = configured.filter(|p| !p.as_os_str().is_empty()) {
        out.push(p.clone());
        // A configured path is authoritative: don't silently fall back to another adb.
        return out;
    }
    if let Some(p) = platform::find_on_path(platform::adb_binary_name(os), env) {
        out.push(p);
    }
    for p in platform::adb_candidates(os, env) {
        if !out.contains(&p) {
            out.push(p);
        }
    }
    out
}

pub async fn detect(
    configured: Option<&PathBuf>,
    runner: Arc<dyn ProcessRunner>,
    env: &dyn Env,
    exists: impl Fn(&PathBuf) -> bool,
) -> AppResult<AdbInfo> {
    let candidates = candidate_paths(configured, env);
    for path in candidates.iter().filter(|p| exists(p)) {
        let client = AdbClient::new(path.clone(), runner.clone());
        match client.version().await {
            Ok(v) => {
                return Ok(AdbInfo {
                    path: path.display().to_string(),
                    version: v.version,
                    revision: v.revision,
                })
            }
            Err(e) => tracing::warn!(path = %path.display(), error = %e, "adb candidate failed"),
        }
    }
    Err(AppError::AdbNotFound {
        searched: candidates.iter().map(|p| p.display().to_string()).collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::{FakeRunner, RawOutput};

    struct NoEnv;
    impl Env for NoEnv {
        fn var(&self, _: &str) -> Option<String> {
            None
        }
        fn home(&self) -> Option<PathBuf> {
            None
        }
    }

    #[tokio::test]
    async fn configured_path_is_authoritative() {
        let f = FakeRunner::new();
        let cfg = PathBuf::from("/custom/adb");
        let err = detect(Some(&cfg), Arc::new(f), &NoEnv, |_| false)
            .await
            .unwrap_err();
        match err {
            AppError::AdbNotFound { searched } => assert_eq!(searched, vec!["/custom/adb"]),
            other => panic!("{other:?}"),
        }
    }

    #[tokio::test]
    async fn detects_first_working_candidate() {
        let f = FakeRunner::new();
        f.on(
            "version",
            Ok(RawOutput::ok(include_str!(
                "../../tests/fixtures/adb/version.txt"
            ))),
        );
        let info = detect(None, Arc::new(f), &NoEnv, |p| p.ends_with("adb"))
            .await
            .unwrap();
        assert_eq!(info.version, "1.0.41");
        assert!(!info.path.is_empty());
    }

    #[tokio::test]
    async fn not_found_lists_searched_paths() {
        let err = detect(None, Arc::new(FakeRunner::new()), &NoEnv, |_| false)
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::AdbNotFound { searched } if !searched.is_empty()));
    }
}
