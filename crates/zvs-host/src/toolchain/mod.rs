pub mod definitions;
pub mod launch;
pub mod probe;
pub mod releases;
pub mod service;

pub use definitions::{
    TOOLCHAIN_DEFINITIONS, ToolchainCommand, ToolchainDefinition, Version, definition,
};
pub use launch::{LaunchSpec, prepend_path};
pub use probe::{Probed, ProcessProbe, SharedProbe, VersionProbe, probe};
pub use releases::{ToolchainBuild, ToolchainResolver, ToolchainResolverOptions};
pub use service::{
    CONNECTED_KEY, CommandLauncher, ConnectedRuntime, ToolchainService, ToolchainServiceOptions,
    parse_toolchain_ref, runtime_of_catalogue_ref, toolchain_archive_ref, toolchain_catalogue_ref,
};

#[cfg(test)]
mod tests;
