pub mod client;
pub mod http;
pub mod protocol;
pub mod stdio;
pub mod toolchain_env;
pub mod transport;
pub mod windows_command;

pub use client::{McpClient, McpClientOptions, McpEndpoint, McpHandshake};
pub use protocol::{
    CLIENT_PROTOCOL_VERSION, McpDiscovery, McpPromptDescriptor, McpResourceDescriptor,
    McpToolDescriptor, SUPPORTED_PROTOCOL_VERSIONS, call_result, infer_destructive,
    infer_read_only,
};
pub use stdio::{LaunchSpec, SpawnServer, default_spawn};
pub use toolchain_env::{TOOLCHAIN_DIR, isolated_toolchain_env};
pub use transport::TransportHooks;
