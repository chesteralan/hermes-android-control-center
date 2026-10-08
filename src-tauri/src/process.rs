//! The only place in the app that spawns OS processes.

use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use process_wrap::tokio::{CommandWrap, KillOnDrop};
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
}

impl RawOutput {
    pub fn ok(stdout: impl Into<String>) -> Self {
        Self {
            stdout: stdout.into(),
            stderr: String::new(),
            exit_code: Some(0),
            duration_ms: 0,
        }
    }

    pub fn success(&self) -> bool {
        self.exit_code == Some(0)
    }

    /// stdout + stderr, for parsers that must inspect both (adb mixes them).
    pub fn combined(&self) -> String {
        if self.stderr.is_empty() {
            self.stdout.clone()
        } else if self.stdout.is_empty() {
            self.stderr.clone()
        } else {
            format!("{}\n{}", self.stdout, self.stderr)
        }
    }
}

pub(crate) async fn cancellable<T>(
    cancel: &CancellationToken,
    operation: impl Future<Output = AppResult<T>>,
) -> AppResult<T> {
    tokio::select! {
        _ = cancel.cancelled() => Err(AppError::Cancelled),
        result = operation => result,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamChunk {
    Stdout(Vec<u8>),
    Stderr(Vec<u8>),
    Exit(Option<i32>),
}

#[async_trait]
pub trait ProcessRunner: Send + Sync {
    async fn run(&self, program: &Path, args: &[String], timeout: Duration)
        -> AppResult<RawOutput>;

    /// Long-running process; the child is killed when `cancel` fires or the receiver is dropped.
    async fn spawn_stream(
        &self,
        program: &Path,
        args: &[String],
        cancel: CancellationToken,
    ) -> AppResult<mpsc::Receiver<StreamChunk>>;
}

fn command(program: &Path, args: &[String]) -> CommandWrap {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut wrapped = CommandWrap::from(cmd);
    wrapped.wrap(KillOnDrop);
    crate::platform::configure_command(&mut wrapped);
    wrapped
}

fn spawn_error(program: &Path, e: std::io::Error) -> AppError {
    if e.kind() == std::io::ErrorKind::NotFound {
        AppError::AdbNotFound {
            searched: vec![program.display().to_string()],
        }
    } else {
        AppError::Io(format!("failed to start {}: {e}", program.display()))
    }
}

pub struct TokioRunner;

#[async_trait]
impl ProcessRunner for TokioRunner {
    async fn run(
        &self,
        program: &Path,
        args: &[String],
        timeout: Duration,
    ) -> AppResult<RawOutput> {
        let started = Instant::now();
        crate::platform::prepare_process(program, args).await?;
        let mut child = command(program, args)
            .spawn()
            .map_err(|e| spawn_error(program, e))?;
        let mut stdout = child.stdout().take().expect("piped stdout");
        let mut stderr = child.stderr().take().expect("piped stderr");
        let mut output = Vec::new();
        let mut errors = Vec::new();
        let collect = async {
            let (_, _, status) = tokio::try_join!(
                stdout.read_to_end(&mut output),
                stderr.read_to_end(&mut errors),
                child.wait(),
            )?;
            Ok::<_, std::io::Error>(status)
        };
        let status = match tokio::time::timeout(timeout, collect).await {
            Ok(Ok(status)) => status,
            Ok(Err(error)) => {
                let _ = Box::into_pin(child.kill()).await;
                let _ = child.wait().await;
                return Err(error.into());
            }
            Err(_) => {
                let _ = Box::into_pin(child.kill()).await;
                let _ = child.wait().await;
                return Err(AppError::Timeout {
                    operation: describe(program, args),
                    after_ms: timeout.as_millis() as u64,
                });
            }
        };
        Ok(RawOutput {
            stdout: String::from_utf8_lossy(&output).replace("\r\n", "\n"),
            stderr: String::from_utf8_lossy(&errors).replace("\r\n", "\n"),
            exit_code: status.code(),
            duration_ms: started.elapsed().as_millis() as u64,
        })
    }

    async fn spawn_stream(
        &self,
        program: &Path,
        args: &[String],
        cancel: CancellationToken,
    ) -> AppResult<mpsc::Receiver<StreamChunk>> {
        crate::platform::prepare_process(program, args).await?;
        let mut child = command(program, args)
            .spawn()
            .map_err(|e| spawn_error(program, e))?;
        let mut stdout = child.stdout().take().expect("piped stdout");
        let mut stderr = child.stderr().take().expect("piped stderr");
        let (tx, rx) = mpsc::channel(256);

        let err_tx = tx.clone();
        tokio::spawn(async move {
            let mut buf = vec![0u8; 8192];
            while let Ok(n) = stderr.read(&mut buf).await {
                if n == 0
                    || err_tx
                        .send(StreamChunk::Stderr(buf[..n].to_vec()))
                        .await
                        .is_err()
                {
                    break;
                }
            }
        });

        tokio::spawn(async move {
            let mut buf = vec![0u8; 8192];
            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    _ = tx.closed() => break,
                    r = stdout.read(&mut buf) => match r {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            let sent = tokio::select! {
                                _ = cancel.cancelled() => false,
                                result = tx.send(StreamChunk::Stdout(buf[..n].to_vec())) => result.is_ok(),
                            };
                            if !sent {
                                break;
                            }
                        }
                    }
                }
            }
            if cancel.is_cancelled() || tx.is_closed() {
                let _ = Box::into_pin(child.kill()).await;
            }
            let code = tokio::select! {
                result = child.wait() => result.ok().and_then(|result| result.code()),
                _ = cancel.cancelled() => {
                    let _ = Box::into_pin(child.kill()).await;
                    child.wait().await.ok().and_then(|result| result.code())
                }
                _ = tx.closed() => {
                    let _ = Box::into_pin(child.kill()).await;
                    child.wait().await.ok().and_then(|result| result.code())
                }
            };
            let _ = tx.send(StreamChunk::Exit(code)).await;
        });

        Ok(rx)
    }
}

fn describe(program: &Path, args: &[String]) -> String {
    let name = program
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let first = args
        .iter()
        .filter(|a| !a.starts_with('-'))
        .take(2)
        .cloned()
        .collect::<Vec<_>>();
    format!("{name} {}", first.join(" ")).trim().to_string()
}

/// Test double: returns scripted outputs keyed by the joined argv.
#[derive(Default, Clone)]
pub struct FakeRunner {
    responses: Arc<Mutex<HashMap<String, Vec<AppResult<RawOutput>>>>>,
    streams: Arc<Mutex<HashMap<String, Vec<StreamChunk>>>>,
    pub calls: Arc<Mutex<Vec<Vec<String>>>>,
}

impl FakeRunner {
    pub fn new() -> Self {
        Self::default()
    }

    /// Queue a response for argv (joined by spaces). The last queued response repeats.
    pub fn on(&self, argv: &str, response: AppResult<RawOutput>) -> &Self {
        self.responses
            .lock()
            .unwrap()
            .entry(argv.to_string())
            .or_default()
            .push(response);
        self
    }

    pub fn on_stream(&self, argv: &str, chunks: Vec<StreamChunk>) -> &Self {
        self.streams
            .lock()
            .unwrap()
            .insert(argv.to_string(), chunks);
        self
    }

    pub fn calls(&self) -> Vec<Vec<String>> {
        self.calls.lock().unwrap().clone()
    }
}

#[async_trait]
impl ProcessRunner for FakeRunner {
    async fn run(
        &self,
        _program: &Path,
        args: &[String],
        _timeout: Duration,
    ) -> AppResult<RawOutput> {
        self.calls.lock().unwrap().push(args.to_vec());
        let key = args.join(" ");
        let mut map = self.responses.lock().unwrap();
        match map.get_mut(&key) {
            Some(queue) if queue.len() > 1 => queue.remove(0),
            Some(queue) if queue.len() == 1 => queue[0].clone(),
            _ => Err(AppError::Io(format!("FakeRunner: no response for `{key}`"))),
        }
    }

    async fn spawn_stream(
        &self,
        _program: &Path,
        args: &[String],
        _cancel: CancellationToken,
    ) -> AppResult<mpsc::Receiver<StreamChunk>> {
        self.calls.lock().unwrap().push(args.to_vec());
        let key = args.join(" ");
        let chunks = self
            .streams
            .lock()
            .unwrap()
            .get(&key)
            .cloned()
            .ok_or_else(|| AppError::Io(format!("FakeRunner: no stream for `{key}`")))?;
        let (tx, rx) = mpsc::channel(chunks.len().max(1));
        tokio::spawn(async move {
            for c in chunks {
                if tx.send(c).await.is_err() {
                    break;
                }
            }
        });
        Ok(rx)
    }
}

pub fn program_path(p: &str) -> PathBuf {
    PathBuf::from(p)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn sh(script: &str) -> Vec<String> {
        vec!["-c".into(), script.into()]
    }

    #[tokio::test]
    async fn captures_stdout_stderr_and_exit_code() {
        let out = TokioRunner
            .run(
                Path::new("/bin/sh"),
                &sh("echo hi; echo err >&2; exit 3"),
                Duration::from_secs(5),
            )
            .await
            .unwrap();
        assert_eq!(out.stdout.trim(), "hi");
        assert_eq!(out.stderr.trim(), "err");
        assert_eq!(out.exit_code, Some(3));
        assert!(!out.success());
    }

    #[tokio::test]
    async fn times_out() {
        let err = TokioRunner
            .run(
                Path::new("/bin/sh"),
                &sh("sleep 5"),
                Duration::from_millis(100),
            )
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::Timeout { .. }));
    }

    #[tokio::test]
    async fn timeout_kills_descendant_processes() {
        let path =
            std::env::temp_dir().join(format!("hacc-timeout-descendant-{}", std::process::id()));
        let mut args = sh("sleep 30 & echo $! > \"$1\"; wait");
        args.push("hacc-test".into());
        args.push(path.to_string_lossy().into_owned());
        let result = TokioRunner
            .run(Path::new("/bin/sh"), &args, Duration::from_millis(300))
            .await;
        assert!(matches!(result, Err(AppError::Timeout { .. })));
        let descendant = std::fs::read_to_string(&path)
            .unwrap()
            .trim()
            .parse::<u32>()
            .unwrap();
        std::fs::remove_file(path).unwrap();
        let state = TokioRunner
            .run(
                Path::new("/bin/sh"),
                &sh(&format!("ps -p {descendant} -o stat=")),
                Duration::from_secs(5),
            )
            .await
            .unwrap();
        assert!(
            !state.success() || state.stdout.trim().starts_with('Z'),
            "Descendant remained running after timeout"
        );
    }

    #[tokio::test]
    async fn missing_binary_is_adb_not_found() {
        let err = TokioRunner
            .run(Path::new("/nonexistent/adb"), &[], Duration::from_secs(1))
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::AdbNotFound { .. }));
    }

    #[tokio::test]
    async fn cancelling_a_run_kills_the_child_process() {
        let marker = std::env::temp_dir().join(format!("hacc-cancel-run-{}", std::process::id()));
        let cancel = CancellationToken::new();
        let task_cancel = cancel.clone();
        let marker_for_task = marker.clone();
        let task = tokio::spawn(async move {
            let mut args = sh("echo $$ > \"$1\"; sleep 30");
            args.push("hacc-cancel-test".into());
            args.push(marker_for_task.to_string_lossy().into_owned());
            cancellable(
                &task_cancel,
                TokioRunner.run(Path::new("/bin/sh"), &args, Duration::from_secs(60)),
            )
            .await
        });

        tokio::time::timeout(Duration::from_secs(5), async {
            while !marker.exists() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        cancel.cancel();
        let result = tokio::time::timeout(Duration::from_secs(5), task)
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(result, Err(AppError::Cancelled)));

        let child_pid = std::fs::read_to_string(&marker).unwrap().trim().to_string();
        std::fs::remove_file(marker).unwrap();
        let child_state = TokioRunner
            .run(
                Path::new("/bin/sh"),
                &sh(&format!("ps -p {child_pid} -o stat=")),
                Duration::from_secs(5),
            )
            .await
            .unwrap();
        assert!(
            !child_state.success() || child_state.stdout.trim().starts_with('Z'),
            "Child process remained active after cancellation"
        );
    }

    #[tokio::test]
    async fn stream_delivers_output_then_exit() {
        let mut rx = TokioRunner
            .spawn_stream(
                Path::new("/bin/sh"),
                &sh("echo a; echo b"),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        let mut out = Vec::new();
        let mut exit = None;
        while let Some(c) = rx.recv().await {
            match c {
                StreamChunk::Stdout(b) => out.extend(b),
                StreamChunk::Exit(code) => {
                    exit = Some(code);
                    break;
                }
                StreamChunk::Stderr(_) => {}
            }
        }
        assert_eq!(String::from_utf8(out).unwrap(), "a\nb\n");
        assert_eq!(exit, Some(Some(0)));
    }

    #[tokio::test]
    async fn cancellation_after_stdout_closes_still_kills_tree() {
        let cancel = CancellationToken::new();
        let mut receiver = TokioRunner
            .spawn_stream(
                Path::new("/bin/sh"),
                &sh("echo ready; exec 1>&-; sleep 30"),
                cancel.clone(),
            )
            .await
            .unwrap();
        assert!(matches!(
            receiver.recv().await,
            Some(StreamChunk::Stdout(_))
        ));
        cancel.cancel();
        tokio::time::timeout(Duration::from_secs(3), async {
            while let Some(chunk) = receiver.recv().await {
                if matches!(chunk, StreamChunk::Exit(_)) {
                    return;
                }
            }
            panic!("Missing process exit after cancellation");
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn stream_cancel_kills_child() {
        let cancel = CancellationToken::new();
        let mut rx = TokioRunner
            .spawn_stream(Path::new("/bin/sh"), &sh("sleep 30"), cancel.clone())
            .await
            .unwrap();
        cancel.cancel();
        let res = tokio::time::timeout(Duration::from_secs(3), async {
            while let Some(c) = rx.recv().await {
                if let StreamChunk::Exit(_) = c {
                    return true;
                }
            }
            false
        })
        .await;
        assert_eq!(res, Ok(true));
    }
}

#[cfg(all(test, windows))]
mod windows_tests {
    use super::*;

    fn powershell(script: &str) -> Vec<String> {
        vec![
            "-NoLogo".into(),
            "-NoProfile".into(),
            "-NonInteractive".into(),
            "-Command".into(),
            script.into(),
        ]
    }

    #[tokio::test]
    async fn normalizes_windows_crlf_and_times_out() {
        let runner = TokioRunner;
        let output = runner
            .run(
                Path::new("powershell.exe"),
                &powershell("[Console]::Write(\"a`r`nb`r`n\")"),
                Duration::from_secs(15),
            )
            .await
            .unwrap();
        assert_eq!(output.stdout, "a\nb\n");
        assert!(matches!(
            runner
                .run(
                    Path::new("powershell.exe"),
                    &powershell("Start-Sleep -Seconds 30"),
                    Duration::from_millis(100)
                )
                .await,
            Err(AppError::Timeout { .. })
        ));
    }

    #[tokio::test]
    async fn cancellation_leaves_no_descendant_process() {
        let cancel = CancellationToken::new();
        let mut receiver = TokioRunner.spawn_stream(
            Path::new("powershell.exe"),
            &powershell("$child=Start-Process powershell.exe -ArgumentList '-NoProfile','-NonInteractive','-Command','Start-Sleep -Seconds 60' -PassThru; [Console]::WriteLine($child.Id); Start-Sleep -Seconds 60"),
            cancel.clone(),
        ).await.unwrap();
        let descendant = tokio::time::timeout(Duration::from_secs(20), async {
            let mut output = String::new();
            loop {
                if let Some(StreamChunk::Stdout(bytes)) = receiver.recv().await {
                    output.push_str(&String::from_utf8_lossy(&bytes));
                    if let Some(line) = output.lines().next().filter(|_| output.contains('\n')) {
                        break line.trim().parse::<u32>().unwrap();
                    }
                } else {
                    panic!("No descendant PID received");
                }
            }
        })
        .await
        .unwrap();
        cancel.cancel();
        tokio::time::timeout(Duration::from_secs(15), async {
            while let Some(chunk) = receiver.recv().await {
                if matches!(chunk, StreamChunk::Exit(_)) {
                    return;
                }
            }
            panic!("Missing Windows job exit");
        })
        .await
        .unwrap();
        let check = TokioRunner
            .run(
                Path::new("powershell.exe"),
                &powershell(&format!(
                    "if (Get-Process -Id {descendant} -ErrorAction SilentlyContinue) {{ exit 1 }}"
                )),
                Duration::from_secs(15),
            )
            .await
            .unwrap();
        assert!(check.success(), "Descendant survived job cancellation");
    }
}
