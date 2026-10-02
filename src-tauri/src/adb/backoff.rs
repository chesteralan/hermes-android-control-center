use std::time::Duration;

/// Reconnect delay schedule: returns delays in order, then `None` (give up).
#[derive(Debug, Clone)]
pub struct Backoff {
    schedule: Vec<Duration>,
    attempt: usize,
}

impl Backoff {
    pub fn new(schedule_ms: &[u64]) -> Self {
        Self {
            schedule: schedule_ms
                .iter()
                .map(|ms| Duration::from_millis(*ms))
                .collect(),
            attempt: 0,
        }
    }

    pub fn next_delay(&mut self) -> Option<Duration> {
        let d = self.schedule.get(self.attempt).copied();
        if d.is_some() {
            self.attempt += 1;
        }
        d
    }

    /// 1-based number of the attempt about to run after the last `next_delay`.
    pub fn attempt(&self) -> usize {
        self.attempt
    }

    pub fn max_attempts(&self) -> usize {
        self.schedule.len()
    }

    pub fn reset(&mut self) {
        self.attempt = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn follows_schedule_then_gives_up() {
        let mut b = Backoff::new(&[1_000, 2_000, 5_000, 10_000, 30_000]);
        let got: Vec<u64> = std::iter::from_fn(|| b.next_delay())
            .map(|d| d.as_millis() as u64)
            .collect();
        assert_eq!(got, vec![1_000, 2_000, 5_000, 10_000, 30_000]);
        assert_eq!(b.next_delay(), None);
        assert_eq!(b.attempt(), 5);
        b.reset();
        assert_eq!(b.next_delay(), Some(Duration::from_secs(1)));
    }
}
