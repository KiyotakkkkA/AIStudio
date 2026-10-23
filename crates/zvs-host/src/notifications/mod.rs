pub mod copy;
pub mod service;

#[cfg(test)]
mod tests;

pub use service::{
    COALESCE_WINDOW_MS, NotificationService, NotificationServiceOptions, RunSubject,
};
