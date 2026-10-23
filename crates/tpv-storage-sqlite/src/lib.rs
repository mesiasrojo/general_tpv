//! SQLite adapters for the local-first TPV.

#![forbid(unsafe_code)]

mod audit;
mod backup;
mod cash;
mod db_key;
mod error;
mod helpers;
mod identity;
mod import;
mod key_store;
mod master_data;
mod persistence;
mod prints;
mod sales;
mod seed;
mod store;
mod sync_outbox;
mod time_clock;
mod users;

pub use db_key::DatabaseKey;
pub use error::{StorageError, StorageResult};
pub use key_store::{
    KEYCHAIN_SERVICE, KeyOrigin, KeyStore, LoadedDatabaseKey, SecretStore, SystemSecretStore,
};
pub use seed::DemoData;
pub use store::SqliteStore;
