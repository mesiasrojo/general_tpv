//! Application use cases and ports.
//!
//! This crate depends on the pure domain and on abstract ports only. Concrete
//! storage, hardware and transport adapters live in other crates.

#![forbid(unsafe_code)]

pub mod dto;
pub mod error;
pub mod import;
pub mod ports;
pub mod services;

pub use dto::*;
pub use error::{ApplicationError, Result};
pub use import::*;
pub use ports::*;
pub use services::*;
