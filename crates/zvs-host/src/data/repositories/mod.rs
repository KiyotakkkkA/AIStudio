pub mod account;
pub mod agentic;
pub mod browser_grant;
pub mod chat;
pub mod document;
pub mod download;
pub mod folder_grant;
pub mod graph_edge;
pub mod graph_relation;
pub mod mcp_server;
pub mod memory;
pub mod model;
pub mod permission;
pub mod provider;
pub mod run;
pub mod scenario;
pub mod secret;
pub mod setting;
pub mod skill;
pub mod tool;
pub mod vector_document;
pub mod vector_eval_case;
pub mod vector_source;
pub mod vector_store;

use rusqlite::Connection;

pub use account::{
    AccountDraft, AccountPatch, AccountRemoval, AccountRepository, NO_ACCOUNT_LINKED,
};
pub use agentic::AgenticSessionRepository;
pub use browser_grant::BrowserGrantRepository;
pub use chat::{ChatRepository, ConversationPatch, MessagePatch};
pub use document::DocumentRepository;
pub use download::{DownloadPatch, DownloadRepository};
pub use folder_grant::FolderGrantRepository;
pub use graph_edge::{GraphEdgeDraft, GraphEdgeLookup, GraphEdgeRepository};
pub use graph_relation::GraphRelationRepository;
pub use mcp_server::{CapabilityCounts, McpServerPatch, McpServerRepository};
pub use memory::MemoryRepository;
pub use model::{ModelDraft, ModelRepository};
pub use permission::PermissionRepository;
pub use provider::{ProviderDraft, ProviderFilter, ProviderPatch, ProviderRepository};
pub use run::{
    RunCounts, RunCursor, RunDraft, RunPageQuery, RunPatch, RunRepository, RunningNode,
    decode_run_cursor, encode_run_cursor,
};
pub use scenario::ScenarioRepository;
pub use secret::{
    SecretDraft, SecretFilter, SecretPatch, SecretRepository, SecretUsageDraft, secret_in_use,
};
pub use setting::SettingRepository;
pub use skill::SkillRepository;
pub use tool::{ToolPatch, ToolRepository};
pub use vector_document::{VectorDocumentDraft, VectorDocumentRepository};
pub use vector_eval_case::{VectorEvalCaseDraft, VectorEvalCaseRepository};
pub use vector_source::{VectorSourceDraft, VectorSourceRepository};
pub use vector_store::{VectorStoreDraft, VectorStorePatch, VectorStoreRepository};

pub struct Repositories<'c> {
    pub settings: SettingRepository<'c>,
    pub secrets: SecretRepository<'c>,
    pub providers: ProviderRepository<'c>,
    pub models: ModelRepository<'c>,
    pub accounts: AccountRepository<'c>,
    pub vector_stores: VectorStoreRepository<'c>,
    pub vector_sources: VectorSourceRepository<'c>,
    pub vector_documents: VectorDocumentRepository<'c>,
    pub vector_eval_cases: VectorEvalCaseRepository<'c>,
    pub graph_relations: GraphRelationRepository<'c>,
    pub graph_edges: GraphEdgeRepository<'c>,
    pub runs: RunRepository<'c>,
    pub permissions: PermissionRepository<'c>,
    pub downloads: DownloadRepository<'c>,
    pub tools: ToolRepository<'c>,
    pub mcp_servers: McpServerRepository<'c>,
    pub chat: ChatRepository<'c>,
    pub skills: SkillRepository<'c>,
    pub folder_grants: FolderGrantRepository<'c>,
    pub documents: DocumentRepository<'c>,
    pub scenarios: ScenarioRepository<'c>,
    pub memory: MemoryRepository<'c>,
    pub agentic_sessions: AgenticSessionRepository<'c>,
    pub browser_grants: BrowserGrantRepository<'c>,
}

impl<'c> Repositories<'c> {
    pub const fn new(connection: &'c Connection) -> Self {
        Self {
            settings: SettingRepository::new(connection),
            secrets: SecretRepository::new(connection),
            providers: ProviderRepository::new(connection),
            models: ModelRepository::new(connection),
            accounts: AccountRepository::new(connection),
            vector_stores: VectorStoreRepository::new(connection),
            vector_sources: VectorSourceRepository::new(connection),
            vector_documents: VectorDocumentRepository::new(connection),
            vector_eval_cases: VectorEvalCaseRepository::new(connection),
            graph_relations: GraphRelationRepository::new(connection),
            graph_edges: GraphEdgeRepository::new(connection),
            runs: RunRepository::new(connection),
            permissions: PermissionRepository::new(connection),
            downloads: DownloadRepository::new(connection),
            tools: ToolRepository::new(connection),
            mcp_servers: McpServerRepository::new(connection),
            chat: ChatRepository::new(connection),
            skills: SkillRepository::new(connection),
            folder_grants: FolderGrantRepository::new(connection),
            documents: DocumentRepository::new(connection),
            scenarios: ScenarioRepository::new(connection),
            memory: MemoryRepository::new(connection),
            agentic_sessions: AgenticSessionRepository::new(connection),
            browser_grants: BrowserGrantRepository::new(connection),
        }
    }
}
