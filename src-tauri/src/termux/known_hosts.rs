//! Trust-on-first-use host key pinning, keyed by stable device id (not the changing serial).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use crate::error::{AppError, AppResult};

#[derive(Debug, PartialEq, Eq)]
pub enum Trust {
    New,
    Match,
    Mismatch { expected: String },
}

pub struct KnownHosts {
    path: PathBuf,
    map: Mutex<BTreeMap<String, String>>,
}

impl KnownHosts {
    pub fn load(data_dir: &Path) -> Self {
        let path = data_dir.join("ssh").join("known_hosts.json");
        let map = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        Self {
            path,
            map: Mutex::new(map),
        }
    }

    fn lock_map(&self) -> AppResult<MutexGuard<'_, BTreeMap<String, String>>> {
        self.map.lock().map_err(|_| {
            AppError::Io(
                "SSH host-key state is unavailable after a previous failure. Restart the app before reconnecting.".into(),
            )
        })
    }

    pub fn expected(&self, device_id: &str) -> AppResult<Option<String>> {
        Ok(self.lock_map()?.get(device_id).cloned())
    }

    pub fn check(&self, device_id: &str, fingerprint: &str) -> AppResult<Trust> {
        Ok(match self.expected(device_id)? {
            None => Trust::New,
            Some(e) if e == fingerprint => Trust::Match,
            Some(expected) => Trust::Mismatch { expected },
        })
    }

    pub fn pin(&self, device_id: &str, fingerprint: &str) -> AppResult<()> {
        let mut map = self.lock_map()?;
        map.insert(device_id.to_string(), fingerprint.to_string());
        self.save(&map)
    }

    pub fn forget(&self, device_id: &str) -> AppResult<()> {
        let mut map = self.lock_map()?;
        map.remove(device_id);
        self.save(&map)
    }

    fn save(&self, map: &BTreeMap<String, String>) -> AppResult<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_string_pretty(map).map_err(|e| AppError::Io(e.to_string()))?;
        std::fs::write(&self.path, json)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tofu_lifecycle_persists() {
        let dir = std::env::temp_dir().join(format!("hacc-kh-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let kh = KnownHosts::load(&dir);
        assert_eq!(kh.check("DEV1", "SHA256:a").unwrap(), Trust::New);
        kh.pin("DEV1", "SHA256:a").unwrap();
        let kh = KnownHosts::load(&dir);
        assert_eq!(kh.check("DEV1", "SHA256:a").unwrap(), Trust::Match);
        assert_eq!(
            kh.check("DEV1", "SHA256:b").unwrap(),
            Trust::Mismatch {
                expected: "SHA256:a".into()
            }
        );
        assert_eq!(kh.check("DEV2", "SHA256:a").unwrap(), Trust::New);
        kh.forget("DEV1").unwrap();
        assert_eq!(
            KnownHosts::load(&dir).check("DEV1", "SHA256:b").unwrap(),
            Trust::New
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn poisoned_host_key_state_fails_closed() {
        let dir = std::env::temp_dir().join(format!("hacc-kh-poison-{}", std::process::id()));
        let known_hosts = std::sync::Arc::new(KnownHosts::load(&dir));
        let poison = known_hosts.clone();
        let _ = std::thread::spawn(move || {
            let _guard = poison.map.lock().unwrap();
            panic!("poison host-key map");
        })
        .join();

        assert!(known_hosts.expected("device").is_err());
        assert!(known_hosts.check("device", "SHA256:new").is_err());
        assert!(known_hosts.pin("device", "SHA256:new").is_err());
        assert!(known_hosts.forget("device").is_err());
    }
}
