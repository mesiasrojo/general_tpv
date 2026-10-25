//! Pure domain model for the General TPV.
//!
//! This crate contains no I/O, async runtime, database access or UI code. All
//! business rules live here so they can be tested deterministically.

#![forbid(unsafe_code)]

pub mod audit;
pub mod cash;
pub mod catalog;
pub mod document;
pub mod error;
pub mod ids;
pub mod invoice;
pub mod master_data;
pub mod money;
pub mod notes;
pub mod people;
pub mod pricing;
pub mod sale;
pub mod stock;
pub mod sync;
pub mod tax;
pub mod timeclock;

pub use error::{DomainError, Result};
pub use ids::*;
pub use money::{Money, Quantity};
pub use notes::{Note, NoteRecipient};
pub use tax::{Tax, TaxBreakdown, TaxRate};
