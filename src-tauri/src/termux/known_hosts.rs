//! Trust-on-first-use host key pinning, keyed by stable device id (not the changing serial).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

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

    pub fn expected(&self, device_id: &str) -> Option<String> {
        self.map.lock().unwrap().get(device_id).cloned()
    }

    pub fn check(&self, device_id: &str, fingerprint: &str) -> Trust {
        match self.expected(device_id) {
            None => Trust::New,
            Some(e) if e == fingerprint => Trust::Match,
            Some(expected) => Trust::Mismatch { expected },
        }
    }

    pub fn pin(&self, device_id: &str, fingerprint: &str) -> AppResult<()> {
        let mut m = self.map.lock().unwrap();
        m.insert(device_id.to_string(), fingerprint.to_string());
        self.save(&m)
    }

    pub fn forget(&self, device_id: &str) -> AppResult<()> {
        let mut m = self.map.lock().unwrap();
        m.remove(device_id);
        self.save(&m)
    }

    fn save(&self, m: &BTreeMap<String, String>) -> AppResult<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_string_pretty(m).map_err(|e| AppError::Io(e.to_string()))?;
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
        assert_eq!(kh.check("DEV1", "SHA256:a"), Trust::New);
        kh.pin("DEV1", "SHA256:a").unwrap();
        let kh = KnownHosts::load(&dir);
        assert_eq!(kh.check("DEV1", "SHA256:a"), Trust::Match);
        assert_eq!(
            kh.check("DEV1", "SHA256:b"),
            Trust::Mismatch {
                expected: "SHA256:a".into()
            }
        );
        assert_eq!(kh.check("DEV2", "SHA256:a"), Trust::New);
        kh.forget("DEV1").unwrap();
        assert_eq!(KnownHosts::load(&dir).check("DEV1", "SHA256:b"), Trust::New);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
