#![deny(clippy::await_holding_lock)]

pub mod browser;
pub mod data;
pub mod drivers;
pub mod ipc;
pub mod kernel;
pub mod platform;
pub mod services;

#[cfg(any(test, feature = "testing"))]
pub mod testing;

pub use ipc::{Coverage, IpcServer};
pub use services::Services;
