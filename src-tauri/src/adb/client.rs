use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::address::validate_address;
use super::args;
use super::parse::{self, ConnectOutcome};
use super::types::*;
use crate::error::{AppError, AppResult};
use crate::process::{ProcessRunner, RawOutput, StreamChunk};

const QUICK: Duration = Duration::from_secs(10);
const CONNECT: Duration = Duration::from_secs(20);
const INFO: Duration = Duration::from_secs(15);

/// Single entry point for every adb invocation.
#[derive(Clone)]
pub struct AdbClient {
    path: PathBuf,
    runner: Arc<dyn ProcessRunner>,
}

impl AdbClient {
    pub fn new(path: impl Into<PathBuf>, runner: Arc<dyn ProcessRunner>) -> Self {
        Self {
            path: path.into(),
            runner,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    async fn exec(&self, argv: Vec<String>, timeout: Duration) -> AppResult<RawOutput> {
        tracing::debug!(args = ?argv.first(), "adb");
        self.runner.run(&self.path, &argv, timeout).await
    }

    /// Escape hatch for argv built by `args::*` in other modules.
    pub async fn raw(&self, argv: &[String], timeout: Duration) -> AppResult<RawOutput> {
        self.exec(argv.to_vec(), timeout).await
    }

    pub async fn version(&self) -> AppResult<parse::VersionInfo> {
        let out = self.exec(args::version(), QUICK).await?;
        parse::parse_version(&out.stdout).ok_or_else(|| AppError::AdbFailed {
            message: "unexpected `adb version` output".into(),
            stderr: out.combined(),
            exit_code: out.exit_code,
        })
    }

    pub async fn devices(&self) -> AppResult<Vec<AndroidDevice>> {
        let out = self.exec(args::devices_l(), QUICK).await?;
        if !out.success() {
            return Err(AppError::AdbFailed {
                message: "could not list devices".into(),
                stderr: parse::strip_daemon_noise(&out.stderr),
                exit_code: out.exit_code,
            });
        }
        Ok(parse::parse_devices(&out.stdout))
    }

    pub async fn restart_server(&self) -> AppResult<()> {
        for argv in [
            args::disconnect_all(),
            args::kill_server(),
            args::start_server(),
        ] {
            let out = self.exec(argv.clone(), QUICK).await?;
            if !out.success() {
                return Err(AppError::AdbFailed {
                    message: format!("could not run adb {}", argv[0]),
                    stderr: out.combined(),
                    exit_code: out.exit_code,
                });
            }
        }
        Ok(())
    }

    pub async fn connect(&self, address: &str) -> AppResult<ConnectOutcome> {
        validate_address(address)?;
        let out = self.exec(args::connect(address), CONNECT).await?;
        parse::parse_connect(address, &out.combined())
    }

    pub async fn disconnect(&self, target: &str) -> AppResult<()> {
        let out = self.exec(args::disconnect(target), QUICK).await?;
        parse::parse_disconnect(target, &out.combined())
    }

    pub async fn install_apk(&self, serial: &str, apk_path: &Path) -> AppResult<RawOutput> {
        let out = self
            .exec(args::install(serial, apk_path), Duration::from_secs(180))
            .await?;
        if !out.success() {
            return Err(AppError::CommandFailed {
                command: "Install Android package".into(),
                exit_code: out.exit_code,
                stderr: if out.stderr.is_empty() {
                    out.stdout.clone()
                } else {
                    out.stderr.clone()
                },
            });
        }
        Ok(out)
    }

    pub async fn push_file(
        &self,
        serial: &str,
        local_path: &Path,
        remote_path: &str,
    ) -> AppResult<RawOutput> {
        let out = self
            .exec(
                args::push(serial, local_path, remote_path),
                Duration::from_secs(60),
            )
            .await?;
        if !out.success() {
            return Err(AppError::CommandFailed {
                command: "Copy bootstrap files to Android".into(),
                exit_code: out.exit_code,
                stderr: if out.stderr.is_empty() {
                    out.stdout.clone()
                } else {
                    out.stderr.clone()
                },
            });
        }
        Ok(out)
    }

    pub async fn pair(&self, address: &str, code: &str) -> AppResult<()> {
        validate_address(address)?;
        if code.is_empty() || code.len() > 64 || !code.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(AppError::Config(
                "Pairing code must be letters and digits only.".into(),
            ));
        }
        let out = self.exec(args::pair(address, code), CONNECT).await?;
        parse::parse_pair(&out.combined())
    }

    pub async fn mdns_check(&self) -> AppResult<()> {
        let out = self.exec(args::mdns_check(), QUICK).await?;
        parse::parse_mdns_check(&out.combined())
    }

    pub async fn mdns_services(&self) -> AppResult<Vec<MdnsService>> {
        let out = self.exec(args::mdns_services(), QUICK).await?;
        Ok(parse::parse_mdns_services(&out.stdout))
    }

    /// Runs `command` in the Android shell (uid `shell`) of `serial`.
    pub async fn shell(
        &self,
        serial: &str,
        command: &str,
        timeout: Duration,
    ) -> AppResult<RawOutput> {
        let out = self.exec(args::shell(serial, command), timeout).await?;
        let stderr = out.stderr.to_lowercase();
        if out.exit_code != Some(0)
            && (stderr.contains("device offline")
                || stderr.contains("unauthorized")
                || stderr.contains("not found")
                || stderr.contains("no devices"))
        {
            return Err(parse::map_device_error(serial, &out.stderr, out.exit_code));
        }
        Ok(out)
    }

    pub async fn device_info(&self, serial: &str) -> AppResult<DeviceInfo> {
        let out = self.shell(serial, parse::DEVICE_INFO_SCRIPT, INFO).await?;
        if !out.stdout.contains("@@end") {
            return Err(parse::map_device_error(
                serial,
                &out.combined(),
                out.exit_code,
            ));
        }
        Ok(parse::parse_device_info(serial, &out.stdout))
    }

    pub async fn track_devices(
        &self,
        cancel: CancellationToken,
    ) -> AppResult<mpsc::Receiver<StreamChunk>> {
        self.runner
            .spawn_stream(&self.path, &args::track_devices(), cancel)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::FakeRunner;

    fn client(f: &FakeRunner) -> AdbClient {
        AdbClient::new("/fake/adb", Arc::new(f.clone()))
    }

    #[tokio::test]
    async fn restart_server_disconnects_then_kills_then_starts() {
        let runner = FakeRunner::new();
        runner.on("disconnect", Ok(RawOutput::ok("disconnected everything")));
        runner.on("kill-server", Ok(RawOutput::ok("")));
        runner.on("start-server", Ok(RawOutput::ok("")));
        client(&runner).restart_server().await.unwrap();
        assert_eq!(
            runner.calls(),
            vec![
                args::disconnect_all(),
                args::kill_server(),
                args::start_server()
            ]
        );
    }

    #[tokio::test]
    async fn restart_server_stops_on_disconnect_failure() {
        let runner = FakeRunner::new();
        runner.on(
            "disconnect",
            Ok(RawOutput {
                stdout: String::new(),
                stderr: "disconnect failed".into(),
                exit_code: Some(1),
                duration_ms: 1,
            }),
        );
        let error = client(&runner).restart_server().await.unwrap_err();
        assert!(
            matches!(error, AppError::AdbFailed { exit_code: Some(1), stderr, .. } if stderr == "disconnect failed")
        );
        assert_eq!(runner.calls(), vec![args::disconnect_all()]);
    }

    #[tokio::test]
    async fn restart_server_stops_on_kill_failure() {
        let runner = FakeRunner::new();
        runner.on("disconnect", Ok(RawOutput::ok("disconnected everything")));
        runner.on(
            "kill-server",
            Ok(RawOutput {
                stdout: String::new(),
                stderr: "cannot reach server".into(),
                exit_code: Some(1),
                duration_ms: 1,
            }),
        );
        let error = client(&runner).restart_server().await.unwrap_err();
        assert!(
            matches!(error, AppError::AdbFailed { exit_code: Some(1), stderr, .. } if stderr == "cannot reach server")
        );
        assert_eq!(
            runner.calls(),
            vec![args::disconnect_all(), args::kill_server()]
        );
    }

    #[tokio::test]
    async fn restart_server_reports_start_failure() {
        let runner = FakeRunner::new();
        runner.on("disconnect", Ok(RawOutput::ok("disconnected everything")));
        runner.on("kill-server", Ok(RawOutput::ok("")));
        runner.on(
            "start-server",
            Ok(RawOutput {
                stdout: String::new(),
                stderr: "cannot bind port".into(),
                exit_code: Some(1),
                duration_ms: 1,
            }),
        );
        let error = client(&runner).restart_server().await.unwrap_err();
        assert!(
            matches!(error, AppError::AdbFailed { exit_code: Some(1), stderr, .. } if stderr == "cannot bind port")
        );
        assert_eq!(
            runner.calls(),
            vec![
                args::disconnect_all(),
                args::kill_server(),
                args::start_server()
            ]
        );
    }

    #[tokio::test]
    async fn connect_validates_before_spawning() {
        let f = FakeRunner::new();
        let err = client(&f).connect("not-an-address").await.unwrap_err();
        assert!(matches!(err, AppError::Config(_)));
        assert!(f.calls().is_empty());
    }

    #[tokio::test]
    async fn connect_parses_output_even_on_exit_zero() {
        let f = FakeRunner::new();
        f.on(
            "connect 192.0.2.10:5555",
            Ok(RawOutput::ok(
                "failed to connect to '192.0.2.10:5555': Connection refused",
            )),
        );
        let err = client(&f).connect("192.0.2.10:5555").await.unwrap_err();
        assert!(matches!(err, AppError::ConnectionRefused { .. }));
    }

    #[tokio::test]
    async fn pair_rejects_bad_codes() {
        let f = FakeRunner::new();
        assert!(client(&f).pair("192.0.2.10:40000", "12 34").await.is_err());
        assert!(client(&f).pair("192.0.2.10:40000", "").await.is_err());
        assert!(f.calls().is_empty());
    }

    #[tokio::test]
    async fn install_apk_uses_device_scoped_argv_and_propagates_failure() {
        let f = FakeRunner::new();
        f.on(
            "-s S install -r /tmp/termux.apk",
            Ok(RawOutput::ok("Success")),
        );
        let result = client(&f)
            .install_apk("S", Path::new("/tmp/termux.apk"))
            .await
            .unwrap();
        assert_eq!(result.stdout, "Success");
        let expected = vec![
            "-s".to_string(),
            "S".to_string(),
            "install".to_string(),
            "-r".to_string(),
            "/tmp/termux.apk".to_string(),
        ];
        assert_eq!(f.calls(), vec![expected]);

        let f = FakeRunner::new();
        f.on(
            "-s S install -r /tmp/termux.apk",
            Ok(RawOutput {
                stdout: String::new(),
                stderr: "INSTALL_FAILED_UPDATE_INCOMPATIBLE".into(),
                exit_code: Some(1),
                duration_ms: 10,
            }),
        );
        let error = client(&f)
            .install_apk("S", Path::new("/tmp/termux.apk"))
            .await
            .unwrap_err();
        assert!(matches!(error, AppError::CommandFailed { .. }));
    }

    #[tokio::test]
    async fn shell_maps_offline() {
        let f = FakeRunner::new();
        f.on(
            "-s S shell echo",
            Ok(RawOutput {
                stdout: String::new(),
                stderr: "error: device offline".into(),
                exit_code: Some(1),
                duration_ms: 1,
            }),
        );
        let err = client(&f).shell("S", "echo", QUICK).await.unwrap_err();
        assert!(matches!(err, AppError::DeviceOffline { .. }));
    }

    #[tokio::test]
    async fn device_info_from_fixture() {
        let f = FakeRunner::new();
        let serial = "adb-TESTSERIAL0001-a00nZY._adb-tls-connect._tcp";
        f.on(
            &format!("-s {serial} shell {}", parse::DEVICE_INFO_SCRIPT),
            Ok(RawOutput::ok(include_str!(
                "../../tests/fixtures/adb/device_info_sectioned.txt"
            ))),
        );
        let info = client(&f).device_info(serial).await.unwrap();
        assert_eq!(info.model.as_deref(), Some("CPH2239"));
    }
}
