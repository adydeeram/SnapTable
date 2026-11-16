//! Secret storage (SPEC §2.3).
//!
//! Secrets live only in the macOS Keychain, as generic-password items under the service
//! [`SERVICE`] with a deterministic account name derived from [`SecretKey`]. The
//! frontend can set/delete/check secrets but no IPC command ever returns one.

use std::collections::HashMap;
use std::sync::Mutex;

use secrecy::SecretString;
use snaptable_core::AppError;

pub const SERVICE: &str = "dev.snaptable.app";

/// Every secret slot the app knows about. The account string format is stable and
/// documented — changing it orphans users' stored secrets.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SecretKey {
    DbPassword { connection_id: String },
    SshPassword { connection_id: String },
    SshKeyPassphrase { connection_id: String },
    TlsKeyPassphrase { connection_id: String },
}

impl SecretKey {
    pub fn account(&self) -> String {
        match self {
            SecretKey::DbPassword { connection_id } => format!("conn:{connection_id}:db_password"),
            SecretKey::SshPassword { connection_id } => {
                format!("conn:{connection_id}:ssh_password")
            }
            SecretKey::SshKeyPassphrase { connection_id } => {
                format!("conn:{connection_id}:ssh_key_passphrase")
            }
            SecretKey::TlsKeyPassphrase { connection_id } => {
                format!("conn:{connection_id}:tls_key_passphrase")
            }
        }
    }

    /// All slots belonging to a connection (used when deleting it).
    pub fn all_for_connection(connection_id: &str) -> Vec<SecretKey> {
        let id = connection_id.to_owned();
        vec![
            SecretKey::DbPassword {
                connection_id: id.clone(),
            },
            SecretKey::SshPassword {
                connection_id: id.clone(),
            },
            SecretKey::SshKeyPassphrase {
                connection_id: id.clone(),
            },
            SecretKey::TlsKeyPassphrase { connection_id: id },
        ]
    }
}

pub trait SecretStore: Send + Sync {
    fn set(&self, key: &SecretKey, value: &str) -> Result<(), AppError>;
    fn get(&self, key: &SecretKey) -> Result<Option<SecretString>, AppError>;
    /// Deleting a missing secret is not an error.
    fn delete(&self, key: &SecretKey) -> Result<(), AppError>;
}

/// macOS Keychain-backed store.
pub struct KeychainStore;

impl KeychainStore {
    /// Register the Apple keychain as the default credential store. Call once at startup.
    #[cfg(target_os = "macos")]
    pub fn init() -> Result<Self, AppError> {
        let store = apple_native_keyring_store::keychain::Store::new().map_err(map_err)?;
        keyring_core::set_default_store(store);
        Ok(Self)
    }

    /// SnapTable targets macOS; elsewhere (e.g. Linux CI) only the memory store exists.
    #[cfg(not(target_os = "macos"))]
    pub fn init() -> Result<Self, AppError> {
        Err(AppError::Auth {
            message: "The system keychain is only supported on macOS; set SNAPTABLE_SECRETS=memory"
                .into(),
            keychain_denied: false,
        })
    }

    fn entry(key: &SecretKey) -> Result<keyring_core::Entry, AppError> {
        keyring_core::Entry::new(SERVICE, &key.account()).map_err(map_err)
    }
}

impl SecretStore for KeychainStore {
    fn set(&self, key: &SecretKey, value: &str) -> Result<(), AppError> {
        Self::entry(key)?.set_password(value).map_err(map_err)
    }

    fn get(&self, key: &SecretKey) -> Result<Option<SecretString>, AppError> {
        match Self::entry(key)?.get_password() {
            Ok(v) => Ok(Some(SecretString::from(v))),
            Err(keyring_core::Error::NoEntry) => Ok(None),
            Err(e) => Err(map_err(e)),
        }
    }

    fn delete(&self, key: &SecretKey) -> Result<(), AppError> {
        match Self::entry(key)?.delete_credential() {
            Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
            Err(e) => Err(map_err(e)),
        }
    }
}

fn map_err(e: keyring_core::Error) -> AppError {
    // Never include secret material in errors; keyring errors carry none.
    let denied = matches!(e, keyring_core::Error::NoStorageAccess(_));
    tracing::warn!(error = %e, "keychain operation failed");
    AppError::Auth {
        message: if denied {
            "Access to the macOS Keychain was denied".into()
        } else {
            format!("Keychain error: {e}")
        },
        keychain_denied: denied,
    }
}

/// In-memory store for tests and for `SNAPTABLE_SECRETS=memory` dev runs.
#[derive(Default)]
pub struct MemoryStore {
    inner: Mutex<HashMap<String, String>>,
}

impl SecretStore for MemoryStore {
    fn set(&self, key: &SecretKey, value: &str) -> Result<(), AppError> {
        self.inner
            .lock()
            .unwrap()
            .insert(key.account(), value.to_owned());
        Ok(())
    }
    fn get(&self, key: &SecretKey) -> Result<Option<SecretString>, AppError> {
        Ok(self
            .inner
            .lock()
            .unwrap()
            .get(&key.account())
            .cloned()
            .map(SecretString::from))
    }
    fn delete(&self, key: &SecretKey) -> Result<(), AppError> {
        self.inner.lock().unwrap().remove(&key.account());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use secrecy::ExposeSecret;

    #[test]
    fn account_names_are_stable() {
        let k = SecretKey::DbPassword {
            connection_id: "abc".into(),
        };
        assert_eq!(k.account(), "conn:abc:db_password");
        assert_eq!(SecretKey::all_for_connection("abc").len(), 4);
    }

    #[test]
    fn memory_store_roundtrip() {
        let s = MemoryStore::default();
        let k = SecretKey::DbPassword {
            connection_id: "x".into(),
        };
        assert!(s.get(&k).unwrap().is_none());
        s.set(&k, "hunter2").unwrap();
        assert_eq!(s.get(&k).unwrap().unwrap().expose_secret(), "hunter2");
        s.delete(&k).unwrap();
        s.delete(&k).unwrap();
        assert!(s.get(&k).unwrap().is_none());
    }
}
