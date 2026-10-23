pub mod definitions;
pub mod launch;
pub mod msi;
pub mod probe;
pub mod process;
pub mod releases;
pub mod service;

pub use definitions::{
    TOOLCHAIN_DEFINITIONS, ToolchainCategory, ToolchainCommand, ToolchainDefinition, Version,
    definition,
};
pub use launch::{LaunchSpec, prepend_path};
pub use msi::{MsiExtractor, MsiexecExtractor, SharedMsiExtractor};
pub use probe::{Probed, ProcessProbe, SharedProbe, VersionProbe, probe};
pub use process::{ProcessLimits, ProcessRun, prepare_scratch, run_process};
pub use releases::{ToolchainBuild, ToolchainResolver, ToolchainResolverOptions};
pub use service::{
    CONNECTED_KEY, CommandLauncher, ConnectedRuntime, ToolchainService, ToolchainServiceOptions,
    parse_toolchain_ref, runtime_of_catalogue_ref, toolchain_archive_ref, toolchain_catalogue_ref,
};

#[cfg(test)]
mod tests;
