use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::parse::parse_threadtime;
use super::{LogLine, LogSource};
use crate::adb::{args, AdbClient};
use crate::error::AppResult;
use crate::process::{ProcessRunner, StreamChunk};
use crate::transport::lines::LineSplitter;

/// Recent lines replayed when the stream starts, before following live output.
pub const BACKLOG: &str = "500";

pub struct LogcatSource {
    client: AdbClient,
    runner: Arc<dyn ProcessRunner>,
    serial: String,
    filter: Vec<String>,
}

impl LogcatSource {
    pub fn new(
        client: AdbClient,
        runner: Arc<dyn ProcessRunner>,
        serial: &str,
        filter: &str,
    ) -> Self {
        Self {
            client,
            runner,
            serial: serial.to_string(),
            filter: filter.split_whitespace().map(str::to_string).collect(),
        }
    }

    pub fn argv(&self) -> Vec<String> {
        let mut extra = vec!["-T".to_string(), BACKLOG.to_string()];
        extra.extend(self.filter.iter().cloned());
        args::logcat(&self.serial, &extra)
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[async_trait]
impl LogSource for LogcatSource {
    fn name(&self) -> String {
        format!("logcat {}", self.filter.join(" "))
    }

    async fn stream(&self, cancel: CancellationToken) -> AppResult<mpsc::Receiver<LogLine>> {
        let mut raw = self
            .runner
            .spawn_stream(self.client.path(), &self.argv(), cancel)
            .await?;
        let (tx, rx) = mpsc::channel(4096);
        tokio::spawn(async move {
            let mut split = LineSplitter::default();
            let mut seq = 0u64;
            while let Some(chunk) = raw.recv().await {
                let lines = match chunk {
                    StreamChunk::Stdout(b) | StreamChunk::Stderr(b) => split.push(&b),
                    StreamChunk::Exit(_) => split.finish().into_iter().collect(),
                };
                for l in lines.into_iter().filter(|l| !l.is_empty()) {
                    seq += 1;
                    if tx.send(parse_threadtime(seq, now_ms(), &l)).await.is_err() {
                        return;
                    }
                }
            }
        });
        Ok(rx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::FakeRunner;

    #[tokio::test]
    async fn streams_parsed_lines_with_backlog_and_filter() {
        let f = FakeRunner::new();
        f.on_stream(
            "-s S logcat -v threadtime -T 500 *:I",
            vec![
                StreamChunk::Stdout(
                    b"--------- beginning of main\n10-02 16:11:21.369  1  2 W Tag: hel".to_vec(),
                ),
                StreamChunk::Stdout(b"lo\n".to_vec()),
                StreamChunk::Exit(None),
            ],
        );
        let runner: Arc<dyn ProcessRunner> = Arc::new(f);
        let src = LogcatSource::new(AdbClient::new("/adb", runner.clone()), runner, "S", "*:I");
        let mut rx = src.stream(CancellationToken::new()).await.unwrap();
        let a = rx.recv().await.unwrap();
        let b = rx.recv().await.unwrap();
        assert_eq!((a.seq, a.level), (1, None));
        assert_eq!(b.seq, 2);
        assert_eq!(b.message, "hello");
        assert!(rx.recv().await.is_none());
    }
}
