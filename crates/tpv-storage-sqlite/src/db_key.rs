use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

use uuid::Uuid;

use crate::error::{StorageError, StorageResult};

const KEY_BYTES: usize = 32;
const KEY_HEX_LEN: usize = KEY_BYTES * 2;

/// Raw SQLCipher key for the local database.
///
/// The key is stored as lowercase hexadecimal and rendered as a SQLCipher blob
/// literal (`"x'...'"`). Debug output is redacted so it cannot leak into logs.
#[derive(Clone, PartialEq, Eq)]
pub struct DatabaseKey(String);

impl DatabaseKey {
    #[must_use]
    pub fn generate() -> Self {
        let mut bytes = [0_u8; KEY_BYTES];
        bytes[..16].copy_from_slice(Uuid::new_v4().as_bytes());
        bytes[16..].copy_from_slice(Uuid::new_v4().as_bytes());
        Self(hex::encode(bytes))
    }

    pub fn from_hex(value: &str) -> StorageResult<Self> {
        let value = value.trim().to_lowercase();
        if value.len() != KEY_HEX_LEN || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(StorageError::InvalidData(
                "the database key must be 64 hexadecimal characters".into(),
            ));
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_hex(&self) -> &str {
        &self.0
    }

    /// Value for sqlx's `pragma("key", ...)` in SQLCipher blob-literal form.
    #[must_use]
    pub fn pragma_value(&self) -> String {
        format!("\"x'{}'\"", self.0)
    }

    /// Loads the key from `path`, creating a new 256-bit key with restrictive
    /// permissions when the file does not exist yet.
    pub fn load_or_create(path: impl AsRef<Path>) -> StorageResult<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                StorageError::InvalidData(format!(
                    "cannot create key directory {}: {error}",
                    parent.display()
                ))
            })?;
        }
        match fs::read_to_string(path) {
            Ok(contents) => Self::from_hex(&contents),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let key = Self::generate();
                match Self::write_new(path, &key.0) {
                    Ok(()) => Ok(key),
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                        let contents = fs::read_to_string(path)?;
                        Self::from_hex(&contents)
                    }
                    Err(error) => Err(error.into()),
                }
            }
            Err(error) => Err(error.into()),
        }
    }

    fn write_new(path: &Path, hex_key: &str) -> std::io::Result<()> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(path)?;
        file.write_all(hex_key.as_bytes())?;
        file.write_all(b"\n")?;
        file.sync_all()
    }
}

impl fmt::Debug for DatabaseKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseKey(redacted)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_canonical_hex_keys() {
        let key = DatabaseKey::from_hex(&"AB".repeat(32)).unwrap();
        assert_eq!(key.as_hex(), "ab".repeat(32));
        assert!(DatabaseKey::from_hex("short").is_err());
        assert!(DatabaseKey::from_hex(&"zz".repeat(32)).is_err());
    }

    #[test]
    fn generated_keys_are_unique_and_redacted() {
        let first = DatabaseKey::generate();
        let second = DatabaseKey::generate();
        assert_ne!(first.as_hex(), second.as_hex());
        assert_eq!(format!("{first:?}"), "DatabaseKey(redacted)");
    }

    #[test]
    fn key_file_is_stable_and_private() {
        let dir = std::env::temp_dir().join(format!("tpv-key-{}", Uuid::new_v4()));
        let path = dir.join("tpv.key");
        let first = DatabaseKey::load_or_create(&path).unwrap();
        let second = DatabaseKey::load_or_create(&path).unwrap();
        assert_eq!(first, second);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        let _ = std::fs::remove_dir_all(dir);
    }
}
