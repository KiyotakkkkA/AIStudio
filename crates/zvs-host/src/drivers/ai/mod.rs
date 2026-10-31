pub mod adapters;
pub mod capabilities;
pub mod copilot;
pub mod errors;
pub mod http;
pub mod identity;
pub mod ports;
pub mod registry;
pub mod sse;
pub mod stream_to_events;
pub mod tool_dialect;
pub mod transport;

#[cfg(test)]
mod live;
#[cfg(test)]
pub(crate) mod mock;

pub use capabilities::{AdapterCapabilities, Honours};
pub use http::{HttpClient, HttpClientOptions, ProxyMode};
pub use ports::{
    AiDriver, ChatMessage, ChatRole, DeltaStream, DiscoveredModel, EmbeddingDriver, FinishReason,
    GenerateRequest, GenerateResult, ImageDriver, TextDelta, TextGenerationDriver, TokenUsage,
    ToolCall, ToolSpec, TurnUsage,
};
pub use registry::{
    AccountPort, AccountSnapshot, DatabaseAccounts, DatabaseProviders, ProviderRegistry,
    ProviderRegistryOptions, ProviderSource, SessionFactory, UnlinkedAccounts,
};
pub use stream_to_events::{StreamToEventsResult, stream_to_events};
pub use tool_dialect::{DefaultDialect, SharedDialect, ToolDialect, with_tool_dialect};
pub use transport::{
    AccountCredentials, AccountToken, AccountTransport, AccountTransportOptions, ApiKey,
    ApiTransport, ApiTransportOptions, ByteStream, DEFAULT_TIMEOUT_SECONDS, RequestSpec,
    SecretResolver, SessionGateway, SessionRequest, SessionResponse, SharedSecretResolver,
    SharedSessionGateway, SharedTransport, Transport, TransportResponse,
};
pub mod openai;
