//! Registry of running streams so the UI can cancel them and nothing outlives its device/window.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};

use tokio_util::sync::CancellationToken;

pub type StreamId = String;

struct Entry {
    serial: String,
    token: CancellationToken,
}

pub struct StreamRegistry {
    next: AtomicU64,
    entries: Mutex<HashMap<StreamId, Entry>>,
    root: CancellationToken,
}

impl StreamRegistry {
    pub fn new(root: CancellationToken) -> Self {
        Self {
            next: AtomicU64::new(1),
            entries: Mutex::default(),
            root,
        }
    }

    fn lock_entries(&self) -> MutexGuard<'_, HashMap<StreamId, Entry>> {
        self.entries.lock().unwrap_or_else(|poisoned| {
            tracing::error!("stream registry lock was poisoned; cancelling active streams");
            let mut entries = poisoned.into_inner();
            for entry in entries.values() {
                entry.token.cancel();
            }
            entries.clear();
            self.entries.clear_poison();
            entries
        })
    }

    pub fn register(&self, serial: &str, prefix: &str) -> (StreamId, CancellationToken) {
        let id = format!("{prefix}-{}", self.next.fetch_add(1, Ordering::Relaxed));
        let token = self.root.child_token();
        self.lock_entries().insert(
            id.clone(),
            Entry {
                serial: serial.to_string(),
                token: token.clone(),
            },
        );
        (id, token)
    }

    /// Call when a stream ends on its own.
    pub fn finish(&self, id: &str) {
        self.lock_entries().remove(id);
    }

    pub fn cancel(&self, id: &str) -> bool {
        match self.lock_entries().remove(id) {
            Some(e) => {
                e.token.cancel();
                true
            }
            None => false,
        }
    }

    pub fn cancel_device(&self, serial: &str) -> usize {
        let mut g = self.lock_entries();
        let ids: Vec<StreamId> = g
            .iter()
            .filter(|(_, e)| e.serial == serial)
            .map(|(k, _)| k.clone())
            .collect();
        for id in &ids {
            if let Some(e) = g.remove(id) {
                e.token.cancel();
            }
        }
        ids.len()
    }

    pub fn len(&self) -> usize {
        self.lock_entries().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancel_by_id_and_by_device() {
        let r = StreamRegistry::new(CancellationToken::new());
        let (a, ta) = r.register("dev1", "cmd");
        let (_b, tb) = r.register("dev1", "logs");
        let (_c, tc) = r.register("dev2", "logs");
        assert_ne!(a, _b);
        assert!(r.cancel(&a));
        assert!(ta.is_cancelled() && !tb.is_cancelled());
        assert!(!r.cancel(&a));
        assert_eq!(r.cancel_device("dev1"), 1);
        assert!(tb.is_cancelled() && !tc.is_cancelled());
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn root_cancel_reaches_all_streams() {
        let root = CancellationToken::new();
        let r = StreamRegistry::new(root.clone());
        let (_, t) = r.register("d", "x");
        root.cancel();
        assert!(t.is_cancelled());
    }

    #[test]
    fn poisoned_registry_cancels_active_streams_and_recovers() {
        let registry = std::sync::Arc::new(StreamRegistry::new(CancellationToken::new()));
        let (_, token) = registry.register("device", "terminal");
        let poison = registry.clone();
        let _ = std::thread::spawn(move || {
            let _guard = poison.entries.lock().unwrap();
            panic!("poison stream registry");
        })
        .join();

        assert_eq!(registry.len(), 0);
        assert!(token.is_cancelled());
        let (_, next) = registry.register("device", "terminal");
        assert!(!next.is_cancelled());
    }
}
