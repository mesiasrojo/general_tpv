//! SQLite adapters for the local-first TPV.

#![forbid(unsafe_code)]

mod audit;
mod cash;
mod error;
mod helpers;
mod persistence;
mod sales;
mod seed;
mod store;
mod sync_outbox;

pub use error::{StorageError, StorageResult};
pub use seed::DemoData;
pub use store::SqliteStore;
