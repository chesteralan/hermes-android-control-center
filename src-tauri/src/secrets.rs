use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use argon2::Argon2;
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, Zeroizing};

use crate::error::{AppError, AppResult};

#[derive(Serialize, Deserialize)]
struct VaultFile {
    version: u32,
    salt: Vec<u8>,
    nonce: Vec<u8>,
    ciphertext: Vec<u8>,
}

struct UnlockedVault {
    key: Zeroizing<[u8; 32]>,
    salt: Vec<u8>,
    tokens: HashMap<String, String>,
}

impl Drop for UnlockedVault {
    fn drop(&mut self) {
        for token in self.tokens.values_mut() {
            token.zeroize();
        }
    }
}

pub struct EncryptedSecrets {
    path: PathBuf,
    unlocked: Mutex<Option<UnlockedVault>>,
}

impl EncryptedSecrets {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            path: data_dir.join("encrypted-secrets.json"),
            unlocked: Mutex::new(None),
        }
    }

    pub fn status(&self) -> &'static str {
        if self.unlocked.lock().unwrap().is_some() {
            "encryptedUnlocked"
        } else if self.path.exists() {
            "encryptedLocked"
        } else {
            "native"
        }
    }

    pub fn unlock(&self, passphrase: &str, opted_in: bool) -> AppResult<()> {
        if !opted_in {
            return Err(AppError::Config(
                "Encrypted file storage requires explicit opt-in.".into(),
            ));
        }
        if passphrase.chars().count() < 12 {
            return Err(AppError::Config(
                "Use a passphrase of at least 12 characters.".into(),
            ));
        }
        let mut unlocked = self.unlocked.lock().unwrap();
        let existing = match std::fs::metadata(&self.path) {
            Ok(metadata) => {
                if metadata.len() > 1024 * 1024 {
                    return Err(AppError::Config(
                        "Encrypted secret storage exceeds its size limit.".into(),
                    ));
                }
                let vault: VaultFile = serde_json::from_slice(&std::fs::read(&self.path)?)
                    .map_err(|_| AppError::Config("Invalid encrypted secret storage.".into()))?;
                if vault.version != 1 || vault.salt.len() != 16 || vault.nonce.len() != 12 {
                    return Err(AppError::Config(
                        "Unsupported encrypted secret storage format.".into(),
                    ));
                }
                Some(vault)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        let mut salt = existing
            .as_ref()
            .map(|vault| vault.salt.clone())
            .unwrap_or_else(|| vec![0; 16]);
        if existing.is_none() {
            rand::rng().fill_bytes(&mut salt);
        }
        let mut key = Zeroizing::new([0; 32]);
        Argon2::default()
            .hash_password_into(passphrase.as_bytes(), &salt, key.as_mut())
            .map_err(|_| AppError::Config("Could not derive the encrypted storage key.".into()))?;
        let tokens = if let Some(vault) = existing {
            let cipher = ChaCha20Poly1305::new_from_slice(key.as_ref())
                .map_err(|_| AppError::Config("Invalid encryption key.".into()))?;
            let plaintext = Zeroizing::new(
                cipher
                    .decrypt(Nonce::from_slice(&vault.nonce), vault.ciphertext.as_ref())
                    .map_err(|_| {
                        AppError::Config(
                            "Incorrect passphrase or damaged encrypted secret storage.".into(),
                        )
                    })?,
            );
            serde_json::from_slice(&plaintext)
                .map_err(|_| AppError::Config("Invalid encrypted secret entries.".into()))?
        } else {
            HashMap::new()
        };
        let vault = UnlockedVault { key, salt, tokens };
        self.save(&vault)?;
        *unlocked = Some(vault);
        Ok(())
    }

    pub fn lock(&self) {
        *self.unlocked.lock().unwrap() = None;
    }

    pub fn get(&self, device_id: &str) -> AppResult<Option<String>> {
        let unlocked = self.unlocked.lock().unwrap();
        let vault = unlocked.as_ref().ok_or_else(locked_error)?;
        Ok(vault.tokens.get(device_id).cloned())
    }

    pub fn set(&self, device_id: &str, token: &str) -> AppResult<()> {
        let mut unlocked = self.unlocked.lock().unwrap();
        let vault = unlocked.as_mut().ok_or_else(locked_error)?;
        let mut previous = vault.tokens.insert(device_id.into(), token.into());
        let result = self.save(vault);
        if result.is_err() {
            if let Some(mut rejected) = vault.tokens.remove(device_id) {
                rejected.zeroize();
            }
            if let Some(previous) = previous.take() {
                vault.tokens.insert(device_id.into(), previous);
            }
        }
        if let Some(mut previous) = previous {
            previous.zeroize();
        }
        result
    }

    fn save(&self, vault: &UnlockedVault) -> AppResult<()> {
        let cipher = ChaCha20Poly1305::new_from_slice(vault.key.as_ref())
            .map_err(|_| AppError::Config("Invalid encryption key.".into()))?;
        let mut nonce = vec![0; 12];
        rand::rng().fill_bytes(&mut nonce);
        let plaintext = Zeroizing::new(
            serde_json::to_vec(&vault.tokens)
                .map_err(|_| AppError::Config("Could not encode secrets.".into()))?,
        );
        let ciphertext = cipher
            .encrypt(Nonce::from_slice(&nonce), plaintext.as_ref())
            .map_err(|_| AppError::Config("Could not encrypt secrets.".into()))?;
        let file = VaultFile {
            version: 1,
            salt: vault.salt.clone(),
            nonce,
            ciphertext,
        };
        let parent = self
            .path
            .parent()
            .ok_or_else(|| AppError::Io("Invalid secret storage path.".into()))?;
        std::fs::create_dir_all(parent)?;
        let temporary = self.path.with_extension("tmp");
        let mut output = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        let result = (|| -> AppResult<()> {
            crate::platform::restrict_file(&temporary)?;
            std::io::Write::write_all(
                &mut output,
                &serde_json::to_vec(&file).map_err(|error| AppError::Io(error.to_string()))?,
            )?;
            output.sync_all()?;
            drop(output);
            std::fs::rename(&temporary, &self.path)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(temporary);
        }
        result
    }
}

fn locked_error() -> AppError {
    AppError::Config(
        "Unlock encrypted secret storage in Settings before using Control API tokens.".into(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_requires_consent_and_never_writes_plaintext() {
        let root =
            std::env::temp_dir().join(format!("hacc-encrypted-secrets-{}", std::process::id()));
        let store = EncryptedSecrets::new(&root);
        assert!(store.unlock("a long test passphrase", false).is_err());
        assert!(!store.path.exists());
        assert!(store.set("phone", "secret-token").is_err());
        store.unlock("a long test passphrase", true).unwrap();
        store.set("phone", "secret-token").unwrap();
        assert!(!std::fs::read_to_string(&store.path)
            .unwrap()
            .contains("secret-token"));
        assert_eq!(store.get("phone").unwrap().as_deref(), Some("secret-token"));
        assert_eq!(store.get("other-phone").unwrap(), None);
        store.lock();
        assert!(store.get("phone").is_err());
        assert!(store.unlock("incorrect passphrase", true).is_err());
        let reloaded = EncryptedSecrets::new(&root);
        assert_eq!(reloaded.status(), "encryptedLocked");
        reloaded.unlock("a long test passphrase", true).unwrap();
        assert_eq!(
            reloaded.get("phone").unwrap().as_deref(),
            Some("secret-token")
        );
        let mut file: VaultFile =
            serde_json::from_slice(&std::fs::read(&store.path).unwrap()).unwrap();
        file.ciphertext[0] ^= 1;
        std::fs::write(&store.path, serde_json::to_vec(&file).unwrap()).unwrap();
        reloaded.lock();
        assert!(reloaded.unlock("a long test passphrase", true).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
