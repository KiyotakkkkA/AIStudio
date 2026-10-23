pub mod copy;
pub mod service;

#[cfg(test)]
mod tests;

pub use service::{
    ACTIVATION_REPEAT_MS, COALESCE_WINDOW_MS, NotificationService, NotificationServiceOptions,
    RunSubject,
};
