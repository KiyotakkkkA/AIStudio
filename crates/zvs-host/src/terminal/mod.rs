pub mod pty;
pub mod service;
pub mod shells;

#[cfg(test)]
mod tests;

pub use pty::{NativePty, PtyChild, PtyControl, PtyParts, PtySpawner, PtySpec};
pub use service::{
    NO_SHELL, SESSION_EXITED, SESSION_MISSING, SESSIONS_LIMIT, SHELL_FAILED, TerminalService,
    TerminalServiceOptions,
};
pub use shells::{SHELL_ORDER, ShellCommand, ShellLocator, SystemShells, shell_title};
