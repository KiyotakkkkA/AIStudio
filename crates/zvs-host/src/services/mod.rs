pub mod accounts;
pub mod approvals;
pub mod crypto;
pub mod graph;
pub mod health_check;
pub mod permissions;
pub mod probe_outcome;
pub mod provider_status;
pub mod providers;
pub mod rerank;
pub mod retention;
pub mod retrieval_merge;
pub mod retrieval_metrics;
pub mod retrieval_query;
pub mod runs;
pub mod secret_fields;
pub mod secret_policy;
pub mod secrets;
pub mod settings;
pub mod stored;
pub mod system;
pub mod vector_eval;
pub mod vector_health;
pub mod vector_sources;
pub mod vector_stores;

pub use accounts::{AccountBrowser, AccountService, AccountServiceOptions, SharedAccountBrowser};
pub use approvals::ApprovalService;
pub use crypto::{
    CryptoBackend, CryptoService, Plaintext, SharedCryptoBackend, UnavailableBackend,
};
pub use graph::{GraphService, GraphServiceOptions};
pub use health_check::{HealthCheckOptions, HealthCheckService};
pub use permissions::{Admission, PermissionContext, PermissionService};
pub use providers::{
    DriverSource, ProbeInput, ProbeResult, ProviderDraftInput, ProviderService,
    ProviderServiceOptions,
};
pub use retention::{RetentionService, RetentionServiceOptions};
pub use runs::{RunService, RunServiceOptions};
pub use secrets::{SecretCreateInput, SecretService, SecretSummary, SecretUpdateInput};
pub use settings::{SettingDto, SettingService};
pub use system::SystemService;
pub use vector_eval::{EvalSearch, VectorEvalService, VectorEvalServiceOptions};
pub use vector_sources::{VectorSourceService, VectorSourceServiceOptions};
pub use vector_stores::{VectorSearchOptions, VectorStoreService, VectorStoreServiceOptions};

use std::sync::Arc;

use crate::downloads::DownloadService;
use crate::indexing::IndexingService;
use crate::platform::picker::SharedPicker;
use crate::platform::theme::SharedThemePort;
use crate::runtimes::RuntimeService;

pub struct Services {
    pub system: SystemService,
    pub accounts: AccountService,
    pub settings: SettingService,
    pub secrets: Arc<SecretService>,
    pub providers: ProviderService,
    pub health: HealthCheckService,
    pub theme: SharedThemePort,
    pub vector_stores: VectorStoreService,
    pub vector_sources: VectorSourceService,
    pub vector_eval: VectorEvalService,
    pub indexing: IndexingService,
    pub graph: GraphService,
    pub runs: RunService,
    pub downloads: DownloadService,
    pub runtimes: RuntimeService,
    pub picker: Option<SharedPicker>,
}
