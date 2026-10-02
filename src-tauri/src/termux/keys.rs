use std::path::{Path, PathBuf};

use rand::RngCore;
use russh::keys::ssh_key::private::Ed25519Keypair;
use russh::keys::ssh_key::LineEnding;
use russh::keys::PrivateKey;

use crate::error::{AppError, AppResult};
use crate::platform;

const FILE: &str = "id_ed25519";
const COMMENT: &str = "hermes-control-center";

pub fn key_path(data_dir: &Path) -> PathBuf {
    data_dir.join("ssh").join(FILE)
}

fn io(e: impl std::fmt::Display) -> AppError {
    AppError::Io(format!("SSH key: {e}"))
}

/// Loads the app's SSH key, creating an ed25519 key on first use. Key material is never logged.
pub fn load_or_create(data_dir: &Path) -> AppResult<PrivateKey> {
    let path = key_path(data_dir);
    if path.is_file() {
        let pem = std::fs::read_to_string(&path).map_err(io)?;
        return PrivateKey::from_openssh(pem).map_err(io);
    }
    let mut seed = [0u8; 32];
    rand::rng().fill_bytes(&mut seed);
    let mut key = PrivateKey::from(Ed25519Keypair::from_seed(&seed));
    key.set_comment(COMMENT);
    let pem = key.to_openssh(LineEnding::LF).map_err(io)?;
    std::fs::create_dir_all(path.parent().expect("key path has parent")).map_err(io)?;
    std::fs::write(&path, pem.as_bytes()).map_err(io)?;
    platform::restrict_file(&path)?;
    tracing::info!("generated new SSH key for the Termux bridge");
    Ok(key)
}

pub fn public_key_line(key: &PrivateKey) -> AppResult<String> {
    key.public_key().to_openssh().map_err(io)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_once_then_reloads_same_key() {
        let dir = std::env::temp_dir().join(format!("hacc-key-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let a = load_or_create(&dir).unwrap();
        let b = load_or_create(&dir).unwrap();
        assert_eq!(public_key_line(&a).unwrap(), public_key_line(&b).unwrap());
        let line = public_key_line(&a).unwrap();
        assert!(line.starts_with("ssh-ed25519 ") && line.ends_with(COMMENT));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(key_path(&dir))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
