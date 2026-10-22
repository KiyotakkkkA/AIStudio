#![deny(clippy::await_holding_lock)]

pub mod browser;
pub mod data;
pub mod downloads;
pub mod drivers;
pub mod indexing;
pub mod ipc;
pub mod kernel;
pub mod platform;
pub mod runtimes;
pub mod services;
pub mod skills;
pub mod toolchain;
pub mod tools;
pub mod workspace;

#[cfg(any(test, feature = "testing"))]
pub mod testing;

pub use ipc::{Coverage, IpcServer};
pub use services::Services;
