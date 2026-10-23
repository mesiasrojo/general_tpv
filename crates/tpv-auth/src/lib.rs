//! Argon2id password hashing adapter.

#![forbid(unsafe_code)]

use argon2::Argon2;
use argon2::PasswordVerifier;
use argon2::password_hash::{PasswordHash, PasswordHasher as _, SaltString, rand_core::OsRng};
use tpv_application::{ApplicationError, PasswordHasher, Result as AppResult};

#[derive(Debug, Clone, Default)]
pub struct Argon2PasswordHasher;

impl Argon2PasswordHasher {
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl PasswordHasher for Argon2PasswordHasher {
    fn hash(&self, password: &str) -> AppResult<String> {
        let salt = SaltString::generate(&mut OsRng);
        Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map(|hash| hash.to_string())
            .map_err(|error| ApplicationError::Internal(error.to_string()))
    }

    fn verify(&self, password: &str, hash: &str) -> AppResult<bool> {
        let parsed = PasswordHash::new(hash)
            .map_err(|error| ApplicationError::Internal(error.to_string()))?;
        Ok(Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_and_verifies_password() {
        let hasher = Argon2PasswordHasher;
        let hash = hasher.hash("admin123").unwrap();
        assert!(hash.starts_with("$argon2"));
        assert!(hasher.verify("admin123", &hash).unwrap());
        assert!(!hasher.verify("wrong", &hash).unwrap());
    }

    #[test]
    fn hashes_are_salted() {
        let hasher = Argon2PasswordHasher;
        assert_ne!(
            hasher.hash("admin123").unwrap(),
            hasher.hash("admin123").unwrap()
        );
    }
}
