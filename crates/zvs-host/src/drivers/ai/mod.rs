pub mod adapters;
pub mod capabilities;
pub mod errors;
pub mod http;
pub mod injected_tools;
pub mod ports;
pub mod registry;
pub mod sse;
pub mod stream_to_events;
pub mod transport;

#[cfg(test)]
mod live;
#[cfg(test)]
pub(crate) mod mock;

pub use capabilities::{AdapterCapabilities, Honours};
pub use http::{HttpClient, HttpClientOptions, ProxyMode};
pub use injected_tools::with_injected_tools;
pub use ports::{
    AiDriver, ChatMessage, ChatRole, DeltaStream, DiscoveredModel, EmbeddingDriver, FinishReason,
    GenerateRequest, GenerateResult, ImageDriver, TextDelta, TextGenerationDriver, TokenUsage,
    ToolCall, ToolSpec,
};
pub use registry::{
    AccountPort, AccountSnapshot, DatabaseProviders, ProviderRegistry, ProviderRegistryOptions,
    ProviderSource, UnlinkedAccounts,
};
pub use stream_to_events::{StreamToEventsResult, stream_to_events};
pub use transport::{
    ApiKey, ApiTransport, ApiTransportOptions, DEFAULT_TIMEOUT_SECONDS, RequestSpec,
    SecretResolver, SharedSecretResolver, SharedTransport, Transport, TransportResponse,
};
