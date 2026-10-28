#![deny(clippy::await_holding_lock)]

pub mod agentic;
pub mod artifact_pages;
pub mod artifacts;
pub mod browser;
pub mod chat_files;
pub mod commands;
pub mod conversations;
pub mod data;
pub mod documents;
pub mod downloads;
pub mod drivers;
pub mod files;
pub mod indexing;
pub mod integrations;
pub mod ipc;
pub mod kernel;
pub mod notifications;
pub mod platform;
pub mod runtimes;
pub mod scenarios;
pub mod services;
pub mod sites;
pub mod skills;
pub mod terminal;
pub mod toolchain;
pub mod tools;
pub mod workers;
pub mod workspace;

#[cfg(any(test, feature = "testing"))]
pub mod testing;

pub use ipc::{Coverage, IpcServer};
pub use services::Services;
