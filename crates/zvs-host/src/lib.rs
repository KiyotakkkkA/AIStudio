#![deny(clippy::await_holding_lock)]

pub mod artifacts;
pub mod browser;
pub mod commands;
pub mod data;
pub mod documents;
pub mod downloads;
pub mod drivers;
pub mod files;
pub mod indexing;
pub mod ipc;
pub mod kernel;
pub mod notifications;
pub mod platform;
pub mod runtimes;
pub mod services;
pub mod skills;
pub mod terminal;
pub mod toolchain;
pub mod tools;
pub mod workspace;

#[cfg(any(test, feature = "testing"))]
pub mod testing;

pub use ipc::{Coverage, IpcServer};
pub use services::Services;
