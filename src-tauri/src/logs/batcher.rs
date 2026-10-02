use std::time::Duration;

use tokio::sync::mpsc;

use super::LogLine;

pub const MAX_BATCH: usize = 500;
pub const FLUSH_EVERY: Duration = Duration::from_millis(50);

/// Groups lines so the UI gets at most ~20 IPC messages per second, regardless of log volume.
pub async fn run(
    mut rx: mpsc::Receiver<LogLine>,
    max: usize,
    every: Duration,
    mut send: impl FnMut(Vec<LogLine>) -> bool,
) {
    let mut batch: Vec<LogLine> = Vec::with_capacity(max);
    let mut tick = tokio::time::interval(every);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            line = rx.recv() => match line {
                Some(l) => {
                    batch.push(l);
                    if batch.len() >= max && !send(std::mem::take(&mut batch)) {
                        return;
                    }
                }
                None => {
                    if !batch.is_empty() {
                        send(batch);
                    }
                    return;
                }
            },
            _ = tick.tick() => {
                if !batch.is_empty() && !send(std::mem::take(&mut batch)) {
                    return;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    fn line(seq: u64) -> LogLine {
        LogLine {
            seq,
            received_at: 0,
            timestamp: None,
            level: None,
            tag: None,
            message: String::new(),
            raw: String::new(),
        }
    }

    #[tokio::test(start_paused = true)]
    async fn flushes_by_size_and_time() {
        let (tx, rx) = mpsc::channel(100);
        let sizes: Arc<Mutex<Vec<usize>>> = Arc::default();
        let s = sizes.clone();
        let task = tokio::spawn(run(rx, 3, Duration::from_millis(50), move |b| {
            s.lock().unwrap().push(b.len());
            true
        }));
        for i in 0..4 {
            tx.send(line(i)).await.unwrap();
        }
        tokio::time::sleep(Duration::from_millis(60)).await;
        tx.send(line(4)).await.unwrap();
        drop(tx);
        task.await.unwrap();
        assert_eq!(*sizes.lock().unwrap(), vec![3, 1, 1]);
    }

    #[tokio::test(start_paused = true)]
    async fn stops_when_receiver_side_is_gone() {
        let (tx, rx) = mpsc::channel(10);
        let task = tokio::spawn(run(rx, 1, Duration::from_millis(50), |_| false));
        tx.send(line(0)).await.unwrap();
        task.await.unwrap();
    }
}
