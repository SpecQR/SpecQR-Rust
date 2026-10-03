//! Dependency-free QR Code Model 2 encoding in safe Rust.
#![forbid(unsafe_code)]
pub mod error;
pub use error::{Error, ErrorCode, Result};
mod api;
pub mod core;
pub mod gs1;
pub mod json;
pub mod kanji;
pub mod render;
pub mod segment;
pub mod structured_append;
pub mod tables;
pub use api::*;
pub use segment::{Mode, Segment};
pub use tables::Ecc;
/// This edition's package version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
