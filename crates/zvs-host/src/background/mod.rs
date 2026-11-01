pub mod nodes;
pub mod service;

#[cfg(test)]
mod tests;

pub use nodes::{
    BACKGROUND_TOOLS, KILL_DESCRIPTION, NO_OWNER, OUTPUT_DESCRIPTION, SHELL_KILL, SHELL_OUTPUT,
    ShellToolOutput, register_background_nodes, status_line,
};
pub use service::{
    BackgroundShellOptions, BackgroundShells, ENDED_KEPT, EndedListener, EndedShell, OWNER_LIMIT,
    SHELL_MISSING, ShellFinish, ShellLimits, ShellStart, TOTAL_LIMIT,
};
