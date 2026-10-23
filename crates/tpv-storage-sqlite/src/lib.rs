//! SQLite adapters for the local-first TPV.

#![forbid(unsafe_code)]

mod audit;
mod backup;
mod cash;
mod db_key;
mod error;
mod helpers;
mod identity;
mod master_data;
mod persistence;
mod prints;
mod sales;
mod seed;
mod store;
mod sync_outbox;
mod users;

pub use db_key::DatabaseKey;
pub use error::{StorageError, StorageResult};
pub use seed::DemoData;
pub use store::SqliteStore;
