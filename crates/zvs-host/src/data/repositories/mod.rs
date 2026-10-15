pub mod account;
pub mod graph_edge;
pub mod graph_relation;
pub mod model;
pub mod provider;
pub mod secret;
pub mod setting;
pub mod vector_document;
pub mod vector_eval_case;
pub mod vector_source;
pub mod vector_store;

use rusqlite::Connection;

pub use account::{
    AccountDraft, AccountPatch, AccountRemoval, AccountRepository, NO_ACCOUNT_LINKED,
};
pub use graph_edge::{GraphEdgeDraft, GraphEdgeLookup, GraphEdgeRepository};
pub use graph_relation::GraphRelationRepository;
pub use model::{ModelDraft, ModelRepository};
pub use provider::{ProviderDraft, ProviderFilter, ProviderPatch, ProviderRepository};
pub use secret::{
    SecretDraft, SecretFilter, SecretPatch, SecretRepository, SecretUsageDraft, secret_in_use,
};
pub use setting::SettingRepository;
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
        }
    }
}
