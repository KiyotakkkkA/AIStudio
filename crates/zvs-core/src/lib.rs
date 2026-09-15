#![doc = include_str!("../README.md")]

pub mod chunk;
pub mod embed;
pub mod error;
pub mod hash;
pub mod index;
pub mod ocr;
pub mod pdf;

pub use error::{Error, Result};
pub use tokio_util::sync::CancellationToken;
