use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use sha2::{Digest, Sha256};

use crate::db_key::DatabaseKey;
use crate::error::StorageResult;

/// Service name used for all database-key entries in the OS keychain.
pub const KEYCHAIN_SERVICE: &str = "general-tpv";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyOrigin {
    Keychain,
    FileFallback,
}

impl KeyOrigin {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Keychain => "keychain del sistema",
            Self::FileFallback => "fichero local (keychain no disponible)",
        }
    }
}

#[derive(Clone)]
pub struct LoadedDatabaseKey {
    pub key: DatabaseKey,
    pub origin: KeyOrigin,
}

impl fmt::Debug for LoadedDatabaseKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LoadedDatabaseKey")
            .field("key", &"redacted")
            .field("origin", &self.origin)
            .finish()
    }
}

/// Minimal secret backend so the key policy can be tested without touching the
/// operating-system keychain.
pub trait SecretStore: Send + Sync {
    fn get(&self, service: &str, account: &str) -> Result<Option<String>, String>;
    fn set(&self, service: &str, account: &str, secret: &str) -> Result<(), String>;
    fn delete(&self, service: &str, account: &str) -> Result<(), String>;
}

#[derive(Debug, Default, Clone)]
pub struct SystemSecretStore;

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
impl SecretStore for SystemSecretStore {
    fn get(&self, service: &str, account: &str) -> Result<Option<String>, String> {
        let entry = keyring::Entry::new(service, account).map_err(|error| error.to_string())?;
        match entry.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(error.to_string()),
        }
    }

    fn set(&self, service: &str, account: &str, secret: &str) -> Result<(), String> {
        keyring::Entry::new(service, account)
            .map_err(|error| error.to_string())?
            .set_password(secret)
            .map_err(|error| error.to_string())
    }

    fn delete(&self, service: &str, account: &str) -> Result<(), String> {
        let entry = keyring::Entry::new(service, account).map_err(|error| error.to_string())?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(error.to_string()),
        }
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
impl SecretStore for SystemSecretStore {
    fn get(&self, _service: &str, _account: &str) -> Result<Option<String>, String> {
        Err("system keychain is not supported on this platform".into())
    }

    fn set(&self, _service: &str, _account: &str, _secret: &str) -> Result<(), String> {
        Err("system keychain is not supported on this platform".into())
    }

    fn delete(&self, _service: &str, _account: &str) -> Result<(), String> {
        Err("system keychain is not supported on this platform".into())
    }
}

/// Loads the SQLCipher key from the OS keychain, migrating an existing
/// `0600` key file on first use. If the keychain is unavailable it falls back
/// to the key file so headless installs keep working.
#[derive(Clone)]
pub struct KeyStore {
    service: String,
    account: String,
    fallback_path: PathBuf,
    database_path: Option<PathBuf>,
    secrets: Arc<dyn SecretStore>,
}

impl KeyStore {
    #[must_use]
    pub fn system(account: impl Into<String>, fallback_path: impl Into<PathBuf>) -> Self {
        Self::with_secret_store(
            KEYCHAIN_SERVICE,
            account.into(),
            fallback_path.into(),
            Arc::new(SystemSecretStore),
        )
    }

    #[must_use]
    pub fn for_database(database: &Path, fallback_path: impl Into<PathBuf>) -> Self {
        let mut store = Self::system(Self::account_for_database(database), fallback_path);
        store.database_path = Some(database.to_path_buf());
        store
    }

    #[must_use]
    pub fn with_secret_store(
        service: impl Into<String>,
        account: impl Into<String>,
        fallback_path: impl Into<PathBuf>,
        secrets: Arc<dyn SecretStore>,
    ) -> Self {
        Self {
            service: service.into(),
            account: account.into(),
            fallback_path: fallback_path.into(),
            database_path: None,
            secrets,
        }
    }

    /// Writes an externally supplied key to the keychain, replacing any
    /// previous entry for this database.
    pub fn import_key(&self, key: &DatabaseKey) -> StorageResult<()> {
        self.secrets
            .set(&self.service, &self.account, key.as_hex())
            .map_err(crate::error::StorageError::InvalidData)
    }

    /// Stable keychain account for a database path. Hashing avoids leaking
    /// filesystem paths into the credential manager.
    #[must_use]
    pub fn account_for_database(database: &Path) -> String {
        let absolute = std::path::absolute(database).unwrap_or_else(|_| database.to_path_buf());
        let digest = Sha256::digest(absolute.to_string_lossy().as_bytes());
        let encoded = hex::encode(digest);
        format!("db-key:{}", &encoded[..32])
    }

    pub fn load_or_create(&self) -> StorageResult<LoadedDatabaseKey> {
        match self.secrets.get(&self.service, &self.account) {
            Ok(Some(secret)) => {
                return Ok(LoadedDatabaseKey {
                    key: DatabaseKey::from_hex(&secret)?,
                    origin: KeyOrigin::Keychain,
                });
            }
            Ok(None) => {}
            Err(_) => {
                if let Some(key) = DatabaseKey::load(&self.fallback_path)? {
                    return Ok(LoadedDatabaseKey {
                        key,
                        origin: KeyOrigin::FileFallback,
                    });
                }
                self.ensure_can_create()?;
                let key = DatabaseKey::generate();
                key.save_new(&self.fallback_path)?;
                return Ok(LoadedDatabaseKey {
                    key,
                    origin: KeyOrigin::FileFallback,
                });
            }
        }

        if let Some(key) = DatabaseKey::load(&self.fallback_path)? {
            if self
                .secrets
                .set(&self.service, &self.account, key.as_hex())
                .is_ok()
            {
                let _ = fs::remove_file(&self.fallback_path);
                return Ok(LoadedDatabaseKey {
                    key,
                    origin: KeyOrigin::Keychain,
                });
            }
            return Ok(LoadedDatabaseKey {
                key,
                origin: KeyOrigin::FileFallback,
            });
        }

        self.ensure_can_create()?;

        let key = DatabaseKey::generate();
        if self
            .secrets
            .set(&self.service, &self.account, key.as_hex())
            .is_ok()
        {
            return Ok(LoadedDatabaseKey {
                key,
                origin: KeyOrigin::Keychain,
            });
        }
        key.save_new(&self.fallback_path)?;
        Ok(LoadedDatabaseKey {
            key,
            origin: KeyOrigin::FileFallback,
        })
    }

    fn ensure_can_create(&self) -> StorageResult<()> {
        if let Some(database) = &self.database_path {
            if fs::metadata(database).is_ok_and(|metadata| metadata.len() > 0) {
                return Err(crate::error::StorageError::InvalidData(format!(
                    "database {} exists but no key was found in the keychain or {}; use --key-file or import the key",
                    database.display(),
                    self.fallback_path.display()
                )));
            }
        }
        Ok(())
    }

    pub fn delete_keychain_entry(&self) -> StorageResult<()> {
        self.secrets
            .delete(&self.service, &self.account)
            .map_err(crate::error::StorageError::InvalidData)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Clone, Default)]
    struct MemorySecretStore {
        entries: Arc<Mutex<HashMap<(String, String), String>>>,
    }

    impl SecretStore for MemorySecretStore {
        fn get(&self, service: &str, account: &str) -> Result<Option<String>, String> {
            Ok(self
                .entries
                .lock()
                .unwrap()
                .get(&(service.to_string(), account.to_string()))
                .cloned())
        }

        fn set(&self, service: &str, account: &str, secret: &str) -> Result<(), String> {
            self.entries.lock().unwrap().insert(
                (service.to_string(), account.to_string()),
                secret.to_string(),
            );
            Ok(())
        }

        fn delete(&self, service: &str, account: &str) -> Result<(), String> {
            self.entries
                .lock()
                .unwrap()
                .remove(&(service.to_string(), account.to_string()));
            Ok(())
        }
    }

    struct UnavailableSecretStore;

    impl SecretStore for UnavailableSecretStore {
        fn get(&self, _service: &str, _account: &str) -> Result<Option<String>, String> {
            Err("no keychain".into())
        }

        fn set(&self, _service: &str, _account: &str, _secret: &str) -> Result<(), String> {
            Err("no keychain".into())
        }

        fn delete(&self, _service: &str, _account: &str) -> Result<(), String> {
            Err("no keychain".into())
        }
    }

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!("tpv-keychain-{}", uuid::Uuid::new_v4()))
            .join(name)
    }

    #[test]
    fn creates_and_reuses_a_keychain_key() {
        let fallback = temp_path("tpv.key");
        let secrets = Arc::new(MemorySecretStore::default());
        let store = KeyStore::with_secret_store("test", "account", &fallback, secrets.clone());
        let first = store.load_or_create().unwrap();
        assert_eq!(first.origin, KeyOrigin::Keychain);
        assert!(!fallback.exists());
        let second = store.load_or_create().unwrap();
        assert_eq!(second.origin, KeyOrigin::Keychain);
        assert_eq!(first.key, second.key);
    }

    #[test]
    fn migrates_an_existing_key_file_to_the_keychain() {
        let fallback = temp_path("tpv.key");
        let file_key = DatabaseKey::load_or_create(&fallback).unwrap();
        assert!(fallback.exists());

        let secrets = Arc::new(MemorySecretStore::default());
        let store = KeyStore::with_secret_store("test", "account", &fallback, secrets.clone());
        let loaded = store.load_or_create().unwrap();
        assert_eq!(loaded.origin, KeyOrigin::Keychain);
        assert_eq!(loaded.key, file_key);
        assert!(!fallback.exists());
        assert_eq!(
            secrets.get("test", "account").unwrap().as_deref(),
            Some(file_key.as_hex())
        );
    }

    #[test]
    fn falls_back_to_a_private_file_without_keychain() {
        let fallback = temp_path("tpv.key");
        let store = KeyStore::with_secret_store(
            "test",
            "account",
            &fallback,
            Arc::new(UnavailableSecretStore),
        );
        let first = store.load_or_create().unwrap();
        assert_eq!(first.origin, KeyOrigin::FileFallback);
        assert!(fallback.exists());
        let second = store.load_or_create().unwrap();
        assert_eq!(second.origin, KeyOrigin::FileFallback);
        assert_eq!(first.key, second.key);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&fallback).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        let _ = fs::remove_dir_all(fallback.parent().unwrap());
    }

    #[test]
    fn refuses_to_create_a_new_key_for_an_existing_database() {
        let directory = temp_path("db").parent().unwrap().to_path_buf();
        fs::create_dir_all(&directory).unwrap();
        let database = directory.join("tpv.db");
        fs::write(&database, b"existing encrypted database").unwrap();
        let fallback = directory.join("tpv.key");
        let mut store = KeyStore::with_secret_store(
            "test",
            "account",
            &fallback,
            Arc::new(MemorySecretStore::default()),
        );
        store.database_path = Some(database);
        let error = store.load_or_create().unwrap_err().to_string();
        assert!(error.contains("no key was found"));
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn unavailable_keychain_does_not_invent_a_key_for_an_existing_database() {
        let directory = temp_path("db").parent().unwrap().to_path_buf();
        fs::create_dir_all(&directory).unwrap();
        let database = directory.join("tpv.db");
        fs::write(&database, b"existing encrypted database").unwrap();
        let fallback = directory.join("tpv.key");
        let mut store = KeyStore::with_secret_store(
            "test",
            "account",
            &fallback,
            Arc::new(UnavailableSecretStore),
        );
        store.database_path = Some(database);
        let error = store.load_or_create().unwrap_err().to_string();
        assert!(error.contains("no key was found"));
        assert!(!fallback.exists());
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn imported_key_is_used_from_the_keychain() {
        let fallback = temp_path("tpv.key");
        let store = KeyStore::with_secret_store(
            "test",
            "account",
            &fallback,
            Arc::new(MemorySecretStore::default()),
        );
        let key = DatabaseKey::generate();
        store.import_key(&key).unwrap();
        let loaded = store.load_or_create().unwrap();
        assert_eq!(loaded.origin, KeyOrigin::Keychain);
        assert_eq!(loaded.key, key);
        assert!(!fallback.exists());
    }

    #[test]
    fn database_paths_map_to_distinct_accounts() {
        let first = KeyStore::account_for_database(Path::new("/tmp/a/tpv.db"));
        let second = KeyStore::account_for_database(Path::new("/tmp/b/tpv.db"));
        assert_ne!(first, second);
        assert!(first.starts_with("db-key:"));
    }
}
