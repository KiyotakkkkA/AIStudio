use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use zvs_contract::{
    AccountDto, AccountLinkInput, AccountLinkResult, AccountRef, AccountsCancelLinkOutput,
    AccountsUnlinkOutput, AdapterCapabilitiesDto, AdapterDescriptorDto, AddGraphEdgeInput,
    AddVectorEvalCaseInput, AddVectorSourceInput, AppErrorCode, BrowserBounds, BrowserCommand,
    CreateProviderInput, CreateSecretInput, CreateVectorStoreInput, DeviceGpuDto, DeviceProfileDto,
    DeviceTier, DiscoveredModelDto, GraphEdgeDto, GraphNeighborDto, GraphNeighborsInput,
    GraphPathDto, GraphReconcileDto, GraphRelationTypeDto, GraphRemoveEdgeOutput,
    GraphShortestPathInput, GraphSubgraphDto, GraphSubgraphInput, HostEvent, Json, ModelDto, Page,
    PageRequest, PickVectorSourceInput, PickVectorSourceResult, ProbeOutcomeDto, ProbeRequest,
    ProbeResultDto, ProviderConnectionInput, ProviderDto, ProviderId, ProviderListFilter,
    ProviderRef, ProviderSettingsDto, ProviderSummaryDto, ProvidersGetInput, ProvidersRemoveOutput,
    RemoveGraphEdgeInput, ResourceSampleDto, ResultEnvelope, RunOutcomeDto, SecretDto,
    SecretFilter, SecretRef, SecretSummaryDto, SecretTypeSchema, SecretsRemoveOutput,
    SecretsTypesOutput, SetDefaultModelInput, SettingsGetInput, SettingsGetOutput,
    SettingsSetInput, SettingsSetOutput, Sort, SortDirection, StreamId, SystemDemoJobInput,
    SystemDemoStreamInput, SystemDemoStreamOutput, SystemDeviceInput, SystemNativePingInput,
    SystemNativePingOutput, SystemPingInput, SystemPingOutput, UpdateProviderInput,
    UpdateSecretInput, UpdateVectorStoreInput, Validate, VectorChunkDto, VectorChunksInput,
    VectorDocumentDto, VectorDocumentsRemoveInput, VectorDocumentsRemoveOutput, VectorEvalCaseDto,
    VectorEvalCaseRef, VectorEvalRemoveOutput, VectorEvalRunDto, VectorEvalRunInput,
    VectorEvalSetDto, VectorOcrConfig, VectorRerankConfig, VectorSearchHitDto, VectorSearchInput,
    VectorSearchResultDto, VectorSourceDto, VectorSourceRef, VectorSourcesRemoveOutput,
    VectorStoreDto, VectorStoreRef, VectorStoresRemoveOutput, secret_type_registry,
};

type Check = fn(Value) -> Result<Value, String>;

const TABLE: &[(&str, Check)] = &[
    ("primitives/ProviderId", round_trip::<ProviderId>),
    ("primitives/StreamId", round_trip::<StreamId>),
    ("primitives/PageRequest", round_trip::<PageRequest>),
    ("primitives/Page", round_trip::<Page<Json>>),
    ("primitives/Sort", round_trip::<Sort<String>>),
    ("primitives/SortDirection", round_trip::<SortDirection>),
    ("errors/AppErrorCode", round_trip::<AppErrorCode>),
    ("errors/ResultEnvelope", round_trip::<ResultEnvelope<Json>>),
    ("events/RunOutcomeDto", round_trip::<RunOutcomeDto>),
    ("events/HostEvent", round_trip::<HostEvent>),
    ("runs/RunStatus", round_trip::<zvs_contract::RunStatus>),
    ("runs/RunKind", round_trip::<zvs_contract::RunKind>),
    ("runs/StepStatus", round_trip::<zvs_contract::StepStatus>),
    ("runs/GraphNode", round_trip::<zvs_contract::GraphNode>),
    ("runs/RunGraph", round_trip::<zvs_contract::RunGraph>),
    (
        "runs/StartRunInput",
        round_trip::<zvs_contract::StartRunInput>,
    ),
    ("runs/RunIdInput", round_trip::<zvs_contract::RunIdInput>),
    (
        "runs/RunHandleDto",
        round_trip::<zvs_contract::RunHandleDto>,
    ),
    (
        "runs/RunsApproveInput",
        round_trip::<zvs_contract::RunsApproveInput>,
    ),
    (
        "runs/RunsDenyInput",
        round_trip::<zvs_contract::RunsDenyInput>,
    ),
    ("runs/RunDto", round_trip::<zvs_contract::RunDto>),
    ("runs/StepDto", round_trip::<zvs_contract::StepDto>),
    (
        "runs/RunListFilter",
        round_trip::<zvs_contract::RunListFilter>,
    ),
    (
        "runs/RunCountsDto",
        round_trip::<zvs_contract::RunCountsDto>,
    ),
    (
        "runs/RunSummaryDto",
        round_trip::<zvs_contract::RunSummaryDto>,
    ),
    ("runs/RunPageDto", round_trip::<zvs_contract::RunPageDto>),
    (
        "runs/RunDetailDto",
        round_trip::<zvs_contract::RunDetailDto>,
    ),
    (
        "runs/RunsClearFinishedOutput",
        round_trip::<zvs_contract::RunsClearFinishedOutput>,
    ),
    ("tools/ToolTier", round_trip::<zvs_contract::ToolTier>),
    ("tools/ToolSource", round_trip::<zvs_contract::ToolSource>),
    (
        "tools/ToolReferenceKind",
        round_trip::<zvs_contract::ToolReferenceKind>,
    ),
    (
        "tools/ToolDefaultTier",
        round_trip::<zvs_contract::ToolDefaultTier>,
    ),
    (
        "tools/ToolReferenceDto",
        round_trip::<zvs_contract::ToolReferenceDto>,
    ),
    ("tools/ToolDto", round_trip::<zvs_contract::ToolDto>),
    (
        "tools/ToolDetailDto",
        round_trip::<zvs_contract::ToolDetailDto>,
    ),
    (
        "tools/ToolUsageDto",
        round_trip::<zvs_contract::ToolUsageDto>,
    ),
    ("tools/ToolRef", round_trip::<zvs_contract::ToolRef>),
    (
        "tools/ToolListFilter",
        round_trip::<zvs_contract::ToolListFilter>,
    ),
    (
        "tools/SetToolTierInput",
        round_trip::<zvs_contract::SetToolTierInput>,
    ),
    (
        "tools/SetToolEnabledInput",
        round_trip::<zvs_contract::SetToolEnabledInput>,
    ),
    (
        "tools/ToolDefaultsDto",
        round_trip::<zvs_contract::ToolDefaultsDto>,
    ),
    (
        "tools/ToolInvokeInput",
        round_trip::<zvs_contract::ToolInvokeInput>,
    ),
    (
        "tools/ToolInvokeResultDto",
        round_trip::<zvs_contract::ToolInvokeResultDto>,
    ),
    ("mcp/McpTransport", round_trip::<zvs_contract::McpTransport>),
    ("mcp/McpStartup", round_trip::<zvs_contract::McpStartup>),
    (
        "mcp/McpServerStatus",
        round_trip::<zvs_contract::McpServerStatus>,
    ),
    (
        "mcp/McpOAuthState",
        round_trip::<zvs_contract::McpOAuthState>,
    ),
    (
        "mcp/McpCapabilityKind",
        round_trip::<zvs_contract::McpCapabilityKind>,
    ),
    ("mcp/McpValue", round_trip::<zvs_contract::McpValue>),
    ("mcp/McpServerDto", round_trip::<zvs_contract::McpServerDto>),
    (
        "mcp/McpCapabilityDto",
        round_trip::<zvs_contract::McpCapabilityDto>,
    ),
    (
        "mcp/McpServerDetailDto",
        round_trip::<zvs_contract::McpServerDetailDto>,
    ),
    (
        "mcp/McpTestResultDto",
        round_trip::<zvs_contract::McpTestResultDto>,
    ),
    (
        "mcp/McpLogLineDto",
        round_trip::<zvs_contract::McpLogLineDto>,
    ),
    ("mcp/McpHealthDto", round_trip::<zvs_contract::McpHealthDto>),
    ("mcp/McpServerRef", round_trip::<zvs_contract::McpServerRef>),
    (
        "mcp/CreateMcpServerInput",
        round_trip::<zvs_contract::CreateMcpServerInput>,
    ),
    (
        "mcp/UpdateMcpServerInput",
        round_trip::<zvs_contract::UpdateMcpServerInput>,
    ),
    (
        "mcp/McpContentDto",
        round_trip::<zvs_contract::McpContentDto>,
    ),
    (
        "mcp/McpCallResultDto",
        round_trip::<zvs_contract::McpCallResultDto>,
    ),
    (
        "mcp/McpRemoveOutput",
        round_trip::<zvs_contract::McpRemoveOutput>,
    ),
    ("system/DeviceTier", round_trip::<DeviceTier>),
    ("system/DeviceGpuDto", round_trip::<DeviceGpuDto>),
    ("system/DeviceProfileDto", round_trip::<DeviceProfileDto>),
    ("system/ResourceSampleDto", round_trip::<ResourceSampleDto>),
    ("system/SystemPingInput", round_trip::<SystemPingInput>),
    ("system/SystemPingOutput", round_trip::<SystemPingOutput>),
    (
        "system/SystemDemoStreamInput",
        round_trip::<SystemDemoStreamInput>,
    ),
    (
        "system/SystemDemoStreamOutput",
        round_trip::<SystemDemoStreamOutput>,
    ),
    (
        "system/SystemDemoJobInput",
        round_trip::<SystemDemoJobInput>,
    ),
    ("system/SystemDeviceInput", round_trip::<SystemDeviceInput>),
    (
        "system/SystemNativePingInput",
        round_trip::<SystemNativePingInput>,
    ),
    (
        "system/SystemNativePingOutput",
        round_trip::<SystemNativePingOutput>,
    ),
    ("settings/SettingsGetInput", round_trip::<SettingsGetInput>),
    (
        "settings/SettingsGetOutput",
        round_trip::<SettingsGetOutput>,
    ),
    ("settings/SettingsSetInput", round_trip::<SettingsSetInput>),
    (
        "settings/SettingsSetOutput",
        round_trip::<SettingsSetOutput>,
    ),
    ("secrets/SecretTypeSchema", round_trip::<SecretTypeSchema>),
    (
        "secrets/SecretsTypesOutput",
        round_trip::<SecretsTypesOutput>,
    ),
    ("secrets/SecretFilter", round_trip::<SecretFilter>),
    ("secrets/SecretRef", round_trip::<SecretRef>),
    ("secrets/CreateSecretInput", round_trip::<CreateSecretInput>),
    ("secrets/UpdateSecretInput", round_trip::<UpdateSecretInput>),
    ("secrets/SecretSummaryDto", round_trip::<SecretSummaryDto>),
    ("secrets/SecretDto", round_trip::<SecretDto>),
    (
        "secrets/SecretsRemoveOutput",
        round_trip::<SecretsRemoveOutput>,
    ),
    (
        "ai/AdapterCapabilitiesDto",
        round_trip::<AdapterCapabilitiesDto>,
    ),
    (
        "ai/AdapterDescriptorDto",
        round_trip::<AdapterDescriptorDto>,
    ),
    (
        "providers/ProviderSettingsDto",
        round_trip::<ProviderSettingsDto>,
    ),
    (
        "providers/DiscoveredModelDto",
        round_trip::<DiscoveredModelDto>,
    ),
    ("providers/ModelDto", round_trip::<ModelDto>),
    (
        "providers/ProviderSummaryDto",
        round_trip::<ProviderSummaryDto>,
    ),
    ("providers/ProviderDto", round_trip::<ProviderDto>),
    (
        "providers/ProviderConnectionInput",
        round_trip::<ProviderConnectionInput>,
    ),
    (
        "providers/CreateProviderInput",
        round_trip::<CreateProviderInput>,
    ),
    (
        "providers/UpdateProviderInput",
        round_trip::<UpdateProviderInput>,
    ),
    (
        "providers/ProviderListFilter",
        round_trip::<ProviderListFilter>,
    ),
    ("providers/ProviderRef", round_trip::<ProviderRef>),
    (
        "providers/ProvidersGetInput",
        round_trip::<ProvidersGetInput>,
    ),
    (
        "providers/SetDefaultModelInput",
        round_trip::<SetDefaultModelInput>,
    ),
    (
        "providers/ProvidersRemoveOutput",
        round_trip::<ProvidersRemoveOutput>,
    ),
    ("providers/ProbeOutcomeDto", round_trip::<ProbeOutcomeDto>),
    ("providers/ProbeRequest", round_trip::<ProbeRequest>),
    ("providers/ProbeResultDto", round_trip::<ProbeResultDto>),
    ("browser/BrowserBounds", round_trip::<BrowserBounds>),
    ("browser/BrowserCommand", round_trip::<BrowserCommand>),
    ("accounts/AccountDto", round_trip::<AccountDto>),
    ("accounts/AccountLinkInput", round_trip::<AccountLinkInput>),
    ("accounts/AccountRef", round_trip::<AccountRef>),
    (
        "accounts/AccountLinkResult",
        round_trip::<AccountLinkResult>,
    ),
    (
        "accounts/AccountsCancelLinkOutput",
        round_trip::<AccountsCancelLinkOutput>,
    ),
    (
        "accounts/AccountsUnlinkOutput",
        round_trip::<AccountsUnlinkOutput>,
    ),
    (
        "vectorStores/VectorRerankConfig",
        round_trip::<VectorRerankConfig>,
    ),
    (
        "vectorStores/VectorOcrConfig",
        round_trip::<VectorOcrConfig>,
    ),
    ("vectorStores/VectorStoreDto", round_trip::<VectorStoreDto>),
    ("vectorStores/VectorStoreRef", round_trip::<VectorStoreRef>),
    (
        "vectorStores/CreateVectorStoreInput",
        round_trip::<CreateVectorStoreInput>,
    ),
    (
        "vectorStores/UpdateVectorStoreInput",
        round_trip::<UpdateVectorStoreInput>,
    ),
    (
        "vectorStores/VectorStoresRemoveOutput",
        round_trip::<VectorStoresRemoveOutput>,
    ),
    (
        "vectorStores/VectorSearchInput",
        round_trip::<VectorSearchInput>,
    ),
    (
        "vectorStores/VectorSearchHitDto",
        round_trip::<VectorSearchHitDto>,
    ),
    (
        "vectorStores/VectorSearchResultDto",
        round_trip::<VectorSearchResultDto>,
    ),
    (
        "vectorStores/VectorChunksInput",
        round_trip::<VectorChunksInput>,
    ),
    ("vectorStores/VectorChunkDto", round_trip::<VectorChunkDto>),
    (
        "vectorStores/VectorSourceDto",
        round_trip::<VectorSourceDto>,
    ),
    (
        "vectorStores/AddVectorSourceInput",
        round_trip::<AddVectorSourceInput>,
    ),
    (
        "vectorStores/VectorSourceRef",
        round_trip::<VectorSourceRef>,
    ),
    (
        "vectorStores/VectorSourcesRemoveOutput",
        round_trip::<VectorSourcesRemoveOutput>,
    ),
    (
        "vectorStores/PickVectorSourceInput",
        round_trip::<PickVectorSourceInput>,
    ),
    (
        "vectorStores/PickVectorSourceResult",
        round_trip::<PickVectorSourceResult>,
    ),
    (
        "vectorStores/VectorIndexInput",
        round_trip::<zvs_contract::VectorIndexInput>,
    ),
    (
        "vectorStores/VectorIndexReportDto",
        round_trip::<zvs_contract::VectorIndexReportDto>,
    ),
    (
        "vectorStores/VectorDocumentDto",
        round_trip::<VectorDocumentDto>,
    ),
    (
        "vectorStores/VectorDocumentsRemoveInput",
        round_trip::<VectorDocumentsRemoveInput>,
    ),
    (
        "vectorStores/VectorDocumentsRemoveOutput",
        round_trip::<VectorDocumentsRemoveOutput>,
    ),
    (
        "vectorStores/VectorEvalCaseDto",
        round_trip::<VectorEvalCaseDto>,
    ),
    (
        "vectorStores/AddVectorEvalCaseInput",
        round_trip::<AddVectorEvalCaseInput>,
    ),
    (
        "vectorStores/VectorEvalCaseRef",
        round_trip::<VectorEvalCaseRef>,
    ),
    (
        "vectorStores/VectorEvalRemoveOutput",
        round_trip::<VectorEvalRemoveOutput>,
    ),
    (
        "vectorStores/VectorEvalRunInput",
        round_trip::<VectorEvalRunInput>,
    ),
    (
        "vectorStores/VectorEvalRunDto",
        round_trip::<VectorEvalRunDto>,
    ),
    (
        "vectorStores/VectorEvalSetDto",
        round_trip::<VectorEvalSetDto>,
    ),
    (
        "graph/GraphRelationTypeDto",
        round_trip::<GraphRelationTypeDto>,
    ),
    ("graph/GraphEdgeDto", round_trip::<GraphEdgeDto>),
    ("graph/AddGraphEdgeInput", round_trip::<AddGraphEdgeInput>),
    (
        "graph/RemoveGraphEdgeInput",
        round_trip::<RemoveGraphEdgeInput>,
    ),
    (
        "graph/GraphRemoveEdgeOutput",
        round_trip::<GraphRemoveEdgeOutput>,
    ),
    ("graph/GraphNeighborDto", round_trip::<GraphNeighborDto>),
    ("graph/GraphPathDto", round_trip::<GraphPathDto>),
    (
        "graph/GraphShortestPathOutput",
        round_trip::<Option<GraphPathDto>>,
    ),
    ("graph/GraphSubgraphDto", round_trip::<GraphSubgraphDto>),
    (
        "graph/GraphNeighborsInput",
        round_trip::<GraphNeighborsInput>,
    ),
    (
        "graph/GraphShortestPathInput",
        round_trip::<GraphShortestPathInput>,
    ),
    ("graph/GraphSubgraphInput", round_trip::<GraphSubgraphInput>),
    ("graph/GraphReconcileDto", round_trip::<GraphReconcileDto>),
    (
        "downloads/DownloadItemKind",
        round_trip::<zvs_contract::DownloadItemKind>,
    ),
    (
        "downloads/DownloadStatus",
        round_trip::<zvs_contract::DownloadStatus>,
    ),
    (
        "downloads/CatalogueItemState",
        round_trip::<zvs_contract::CatalogueItemState>,
    ),
    (
        "downloads/DiskCategory",
        round_trip::<zvs_contract::DiskCategory>,
    ),
    (
        "downloads/ChecksumDto",
        round_trip::<zvs_contract::ChecksumDto>,
    ),
    (
        "downloads/CatalogueItemDto",
        round_trip::<zvs_contract::CatalogueItemDto>,
    ),
    (
        "downloads/DownloadDto",
        round_trip::<zvs_contract::DownloadDto>,
    ),
    (
        "downloads/DiskUsageDto",
        round_trip::<zvs_contract::DiskUsageDto>,
    ),
    (
        "downloads/DownloadRef",
        round_trip::<zvs_contract::DownloadRef>,
    ),
    (
        "downloads/PrioritiseDownloadInput",
        round_trip::<zvs_contract::PrioritiseDownloadInput>,
    ),
    (
        "downloads/StartDownloadInput",
        round_trip::<zvs_contract::StartDownloadInput>,
    ),
    (
        "downloads/DownloadListFilter",
        round_trip::<zvs_contract::DownloadListFilter>,
    ),
    (
        "downloads/CatalogueFilter",
        round_trip::<zvs_contract::CatalogueFilter>,
    ),
    (
        "downloads/DownloadsDiskInput",
        round_trip::<zvs_contract::DownloadsDiskInput>,
    ),
    (
        "downloads/DownloadsRemoveOutput",
        round_trip::<zvs_contract::DownloadsRemoveOutput>,
    ),
    (
        "downloads/DownloadFetchInput",
        round_trip::<zvs_contract::DownloadFetchInput>,
    ),
    (
        "downloads/DownloadReportDto",
        round_trip::<zvs_contract::DownloadReportDto>,
    ),
    (
        "runtimes/ModelFormat",
        round_trip::<zvs_contract::ModelFormat>,
    ),
    (
        "runtimes/Accelerator",
        round_trip::<zvs_contract::Accelerator>,
    ),
    (
        "runtimes/RuntimeEngine",
        round_trip::<zvs_contract::RuntimeEngine>,
    ),
    (
        "runtimes/RuntimeState",
        round_trip::<zvs_contract::RuntimeState>,
    ),
    (
        "runtimes/RuntimeDto",
        round_trip::<zvs_contract::RuntimeDto>,
    ),
    (
        "runtimes/RuntimePlanDto",
        round_trip::<zvs_contract::RuntimePlanDto>,
    ),
    (
        "runtimes/RuntimeOverviewDto",
        round_trip::<zvs_contract::RuntimeOverviewDto>,
    ),
    (
        "runtimes/RuntimeRef",
        round_trip::<zvs_contract::RuntimeRef>,
    ),
    (
        "runtimes/InstallRuntimeInput",
        round_trip::<zvs_contract::InstallRuntimeInput>,
    ),
    (
        "runtimes/RuntimesStopOutput",
        round_trip::<zvs_contract::RuntimesStopOutput>,
    ),
    (
        "runtimes/RuntimesRemoveOutput",
        round_trip::<zvs_contract::RuntimesRemoveOutput>,
    ),
    (
        "chat/ChatSettings",
        round_trip::<zvs_contract::ChatSettings>,
    ),
    (
        "chat/ChatCitationDto",
        round_trip::<zvs_contract::ChatCitationDto>,
    ),
    (
        "chat/ChatToolCallStatus",
        round_trip::<zvs_contract::ChatToolCallStatus>,
    ),
    (
        "chat/ChatToolCallDto",
        round_trip::<zvs_contract::ChatToolCallDto>,
    ),
    (
        "chat/CreateConversationInput",
        round_trip::<zvs_contract::CreateConversationInput>,
    ),
    (
        "chat/ConversationRef",
        round_trip::<zvs_contract::ConversationRef>,
    ),
    (
        "chat/ConversationDto",
        round_trip::<zvs_contract::ConversationDto>,
    ),
    ("chat/MessageDto", round_trip::<zvs_contract::MessageDto>),
    (
        "chat/ConversationDetailDto",
        round_trip::<zvs_contract::ConversationDetailDto>,
    ),
    (
        "chat/ChatSendInput",
        round_trip::<zvs_contract::ChatSendInput>,
    ),
    (
        "chat/ChatContextInput",
        round_trip::<zvs_contract::ChatContextInput>,
    ),
    (
        "chat/ChatContextDto",
        round_trip::<zvs_contract::ChatContextDto>,
    ),
    (
        "chat/TruncateConversationInput",
        round_trip::<zvs_contract::TruncateConversationInput>,
    ),
    (
        "chat/RenameConversationInput",
        round_trip::<zvs_contract::RenameConversationInput>,
    ),
    (
        "chat/UpdateConversationStoresInput",
        round_trip::<zvs_contract::UpdateConversationStoresInput>,
    ),
    (
        "chat/UpdateConversationRelationsInput",
        round_trip::<zvs_contract::UpdateConversationRelationsInput>,
    ),
    (
        "chat/UpdateConversationToolsInput",
        round_trip::<zvs_contract::UpdateConversationToolsInput>,
    ),
    (
        "files/FileChangeKind",
        round_trip::<zvs_contract::files::FileChangeKind>,
    ),
    (
        "files/FileDiffDto",
        round_trip::<zvs_contract::files::FileDiffDto>,
    ),
    (
        "files/FileChangesDto",
        round_trip::<zvs_contract::files::FileChangesDto>,
    ),
    (
        "files/FilesApprovalDto",
        round_trip::<zvs_contract::files::FilesApprovalDto>,
    ),
    (
        "process/CodeLanguage",
        round_trip::<zvs_contract::process::CodeLanguage>,
    ),
    (
        "process/ProcessPlanDto",
        round_trip::<zvs_contract::process::ProcessPlanDto>,
    ),
    (
        "process/ProcessResultDto",
        round_trip::<zvs_contract::process::ProcessResultDto>,
    ),
    (
        "process/ProcessApprovalDto",
        round_trip::<zvs_contract::process::ProcessApprovalDto>,
    ),
    (
        "explorer/ExplorerEntryKind",
        round_trip::<zvs_contract::explorer::ExplorerEntryKind>,
    ),
    (
        "explorer/ExplorerChangeKind",
        round_trip::<zvs_contract::explorer::ExplorerChangeKind>,
    ),
    (
        "explorer/ExplorerEntryDto",
        round_trip::<zvs_contract::explorer::ExplorerEntryDto>,
    ),
    (
        "explorer/ExplorerListInput",
        round_trip::<zvs_contract::explorer::ExplorerListInput>,
    ),
    (
        "explorer/ExplorerListingDto",
        round_trip::<zvs_contract::explorer::ExplorerListingDto>,
    ),
    (
        "explorer/ExplorerReadInput",
        round_trip::<zvs_contract::explorer::ExplorerReadInput>,
    ),
    (
        "explorer/ExplorerFileDto",
        round_trip::<zvs_contract::explorer::ExplorerFileDto>,
    ),
    (
        "explorer/ExplorerChangeDto",
        round_trip::<zvs_contract::explorer::ExplorerChangeDto>,
    ),
    (
        "explorer/ExplorerChangesDto",
        round_trip::<zvs_contract::explorer::ExplorerChangesDto>,
    ),
    (
        "explorer/ExplorerWatchDto",
        round_trip::<zvs_contract::explorer::ExplorerWatchDto>,
    ),
    (
        "artifacts/ArtifactDto",
        round_trip::<zvs_contract::artifacts::ArtifactDto>,
    ),
    (
        "artifacts/ArtifactRef",
        round_trip::<zvs_contract::artifacts::ArtifactRef>,
    ),
    (
        "artifacts/ArtifactSaveResult",
        round_trip::<zvs_contract::artifacts::ArtifactSaveResult>,
    ),
    (
        "artifacts/ArtifactApplicationDto",
        round_trip::<zvs_contract::artifacts::ArtifactApplicationDto>,
    ),
    (
        "chat/DocumentSelectionDto",
        round_trip::<zvs_contract::chat::DocumentSelectionDto>,
    ),
    (
        "documents/DocumentSaveResult",
        round_trip::<zvs_contract::documents::DocumentSaveResult>,
    ),
    (
        "documents/DocumentKind",
        round_trip::<zvs_contract::documents::DocumentKind>,
    ),
    (
        "documents/DocumentOfficeFormat",
        round_trip::<zvs_contract::documents::DocumentOfficeFormat>,
    ),
    (
        "documents/DocumentTextEncoding",
        round_trip::<zvs_contract::documents::DocumentTextEncoding>,
    ),
    (
        "documents/DocumentRef",
        round_trip::<zvs_contract::documents::DocumentRef>,
    ),
    (
        "documents/DocumentDto",
        round_trip::<zvs_contract::documents::DocumentDto>,
    ),
    (
        "documents/DocumentHandle",
        round_trip::<zvs_contract::documents::DocumentHandle>,
    ),
    (
        "documents/DocumentPageInput",
        round_trip::<zvs_contract::documents::DocumentPageInput>,
    ),
    (
        "documents/DocumentPageDto",
        round_trip::<zvs_contract::documents::DocumentPageDto>,
    ),
    (
        "folders/FolderGrantScope",
        round_trip::<zvs_contract::folders::FolderGrantScope>,
    ),
    (
        "folders/WorkingFolderAccess",
        round_trip::<zvs_contract::folders::WorkingFolderAccess>,
    ),
    (
        "folders/FolderAccessAnswer",
        round_trip::<zvs_contract::folders::FolderAccessAnswer>,
    ),
    (
        "folders/FolderGrantDto",
        round_trip::<zvs_contract::folders::FolderGrantDto>,
    ),
    (
        "folders/FolderGrantRef",
        round_trip::<zvs_contract::folders::FolderGrantRef>,
    ),
    (
        "folders/FolderRequestEntryDto",
        round_trip::<zvs_contract::folders::FolderRequestEntryDto>,
    ),
    (
        "folders/FolderApprovalDto",
        round_trip::<zvs_contract::folders::FolderApprovalDto>,
    ),
    (
        "folders/AnswerFolderRequestInput",
        round_trip::<zvs_contract::folders::AnswerFolderRequestInput>,
    ),
    (
        "folders/BindFolderInput",
        round_trip::<zvs_contract::folders::BindFolderInput>,
    ),
    (
        "folders/BindFolderResultDto",
        round_trip::<zvs_contract::folders::BindFolderResultDto>,
    ),
    (
        "folders/PickFolderResultDto",
        round_trip::<zvs_contract::folders::PickFolderResultDto>,
    ),
    (
        "skills/SkillSource",
        round_trip::<zvs_contract::skills::SkillSource>,
    ),
    (
        "skills/SkillStatus",
        round_trip::<zvs_contract::skills::SkillStatus>,
    ),
    (
        "skills/SkillMatchMethod",
        round_trip::<zvs_contract::skills::SkillMatchMethod>,
    ),
    (
        "skills/SkillFileEncoding",
        round_trip::<zvs_contract::skills::SkillFileEncoding>,
    ),
    (
        "skills/SkillIssueRule",
        round_trip::<zvs_contract::skills::SkillIssueRule>,
    ),
    (
        "skills/SkillModelDto",
        round_trip::<zvs_contract::skills::SkillModelDto>,
    ),
    (
        "skills/SkillIssueDto",
        round_trip::<zvs_contract::skills::SkillIssueDto>,
    ),
    (
        "skills/SkillFileDto",
        round_trip::<zvs_contract::skills::SkillFileDto>,
    ),
    (
        "skills/SkillDto",
        round_trip::<zvs_contract::skills::SkillDto>,
    ),
    (
        "skills/SkillDetailDto",
        round_trip::<zvs_contract::skills::SkillDetailDto>,
    ),
    (
        "skills/SkillRef",
        round_trip::<zvs_contract::skills::SkillRef>,
    ),
    (
        "skills/SkillFileRef",
        round_trip::<zvs_contract::skills::SkillFileRef>,
    ),
    (
        "skills/SkillFileContentDto",
        round_trip::<zvs_contract::skills::SkillFileContentDto>,
    ),
    (
        "skills/SetSkillEnabledInput",
        round_trip::<zvs_contract::skills::SetSkillEnabledInput>,
    ),
    (
        "skills/SkillFileInput",
        round_trip::<zvs_contract::skills::SkillFileInput>,
    ),
    (
        "skills/CreateSkillInput",
        round_trip::<zvs_contract::skills::CreateSkillInput>,
    ),
    (
        "skills/UpdateSkillInput",
        round_trip::<zvs_contract::skills::UpdateSkillInput>,
    ),
    (
        "skills/SkillTestInput",
        round_trip::<zvs_contract::skills::SkillTestInput>,
    ),
    (
        "skills/SkillMatchDto",
        round_trip::<zvs_contract::skills::SkillMatchDto>,
    ),
    (
        "skills/SkillTestResultDto",
        round_trip::<zvs_contract::skills::SkillTestResultDto>,
    ),
    (
        "skills/SkillRunInput",
        round_trip::<zvs_contract::skills::SkillRunInput>,
    ),
    (
        "skills/SkillImportInput",
        round_trip::<zvs_contract::skills::SkillImportInput>,
    ),
    (
        "skills/SkillImportResultDto",
        round_trip::<zvs_contract::skills::SkillImportResultDto>,
    ),
    (
        "skills/SkillExportInput",
        round_trip::<zvs_contract::skills::SkillExportInput>,
    ),
    (
        "skills/SkillExportResultDto",
        round_trip::<zvs_contract::skills::SkillExportResultDto>,
    ),
    (
        "skills/SkillRemoveOutput",
        round_trip::<zvs_contract::skills::SkillRemoveOutput>,
    ),
    (
        "skills/SkillTriggerSettingsDto",
        round_trip::<zvs_contract::skills::SkillTriggerSettingsDto>,
    ),
    (
        "toolchain/ToolchainRuntime",
        round_trip::<zvs_contract::ToolchainRuntime>,
    ),
    (
        "toolchain/ToolchainSource",
        round_trip::<zvs_contract::ToolchainSource>,
    ),
    (
        "toolchain/ToolchainStatus",
        round_trip::<zvs_contract::ToolchainStatus>,
    ),
    (
        "toolchain/ToolchainDto",
        round_trip::<zvs_contract::ToolchainDto>,
    ),
    (
        "toolchain/ToolchainListInput",
        round_trip::<zvs_contract::ToolchainListInput>,
    ),
    (
        "toolchain/ToolchainRef",
        round_trip::<zvs_contract::ToolchainRef>,
    ),
    (
        "toolchain/ToolchainSelectInput",
        round_trip::<zvs_contract::ToolchainSelectInput>,
    ),
];

const SECRET_OUTPUTS: &[&str] = &[
    "secrets/SecretsTypesOutput",
    "secrets/SecretTypeSchema",
    "secrets/SecretSummaryDto",
    "secrets/SecretDto",
    "secrets/SecretsRemoveOutput",
];

const PLAINTEXT_KEYS: &[&str] = &["value", "cipher", "plaintext", "secret", "apiKey"];

fn round_trip<T: DeserializeOwned + Serialize + Validate>(value: Value) -> Result<Value, String> {
    let parsed: T = serde_json::from_value(value).map_err(|error| error.to_string())?;
    parsed.validate().map_err(|error| error.to_string())?;
    serde_json::to_value(&parsed).map_err(|error| error.to_string())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Valid,
    Invalid,
}

struct Fixture {
    name: String,
    kind: Kind,
    path: PathBuf,
}

fn fixtures_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/shared/fixtures")
}

fn collect(dir: &Path, root: &Path, out: &mut Vec<Fixture>) {
    let entries = fs::read_dir(dir).unwrap_or_else(|error| panic!("{}: {error}", dir.display()));
    for entry in entries {
        let path = entry.expect("directory entry").path();
        if path.is_dir() {
            collect(&path, root, out);
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .expect("inside the fixtures root")
            .to_string_lossy()
            .replace('\\', "/");
        let (name, kind) = if let Some(name) = relative.strip_suffix(".valid.json") {
            (name.to_owned(), Kind::Valid)
        } else if let Some(name) = relative.strip_suffix(".invalid.json") {
            (name.to_owned(), Kind::Invalid)
        } else {
            panic!("{relative} is neither <Name>.valid.json nor <Name>.invalid.json");
        };
        out.push(Fixture { name, kind, path });
    }
}

fn all_fixtures() -> Vec<Fixture> {
    let root = fixtures_root();
    let mut out = Vec::new();
    collect(&root, &root, &mut out);
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

fn cases(path: &Path) -> Vec<(String, Value)> {
    let text =
        fs::read_to_string(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let parsed: Vec<Value> =
        serde_json::from_str(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    assert!(!parsed.is_empty(), "{} has no cases", path.display());
    parsed
        .into_iter()
        .map(|case| {
            let name = case["name"]
                .as_str()
                .unwrap_or_else(|| panic!("{}: a case without a name", path.display()))
                .to_owned();
            let value = case
                .get("value")
                .cloned()
                .unwrap_or_else(|| panic!("{}: case {name} has no value", path.display()));
            (name, value)
        })
        .collect()
}

fn check_for(name: &str) -> Option<Check> {
    TABLE
        .iter()
        .find(|(entry, _)| *entry == name)
        .map(|(_, check)| *check)
}

#[test]
fn every_fixture_has_a_rust_type_and_every_type_has_a_valid_fixture() {
    let fixtures = all_fixtures();
    let mut problems = Vec::new();
    for fixture in &fixtures {
        if check_for(&fixture.name).is_none() {
            problems.push(format!(
                "{} has no entry in the Rust fixture table",
                fixture.name
            ));
        }
    }
    let valid: BTreeSet<&str> = fixtures
        .iter()
        .filter(|fixture| fixture.kind == Kind::Valid)
        .map(|fixture| fixture.name.as_str())
        .collect();
    for (name, _) in TABLE {
        if !valid.contains(name) {
            problems.push(format!(
                "{name} is in the table but has no {name}.valid.json"
            ));
        }
    }
    assert!(problems.is_empty(), "\n{}", problems.join("\n"));
}

#[test]
fn valid_fixtures_deserialise_validate_and_round_trip_exactly() {
    let mut problems = Vec::new();
    for fixture in all_fixtures()
        .iter()
        .filter(|fixture| fixture.kind == Kind::Valid)
    {
        let Some(check) = check_for(&fixture.name) else {
            continue;
        };
        for (case, value) in cases(&fixture.path) {
            match check(value.clone()) {
                Ok(output) if output == value => {}
                Ok(output) => problems.push(format!(
                    "{} / {case}: re-serialised JSON differs\n  fixture: {value}\n  serde:   {output}",
                    fixture.name
                )),
                Err(error) => problems.push(format!("{} / {case}: rejected: {error}", fixture.name)),
            }
        }
    }
    assert!(problems.is_empty(), "\n{}", problems.join("\n"));
}

#[test]
fn invalid_fixtures_fail_deserialisation_or_validation() {
    let mut problems = Vec::new();
    for fixture in all_fixtures()
        .iter()
        .filter(|fixture| fixture.kind == Kind::Invalid)
    {
        let Some(check) = check_for(&fixture.name) else {
            continue;
        };
        for (case, value) in cases(&fixture.path) {
            if check(value).is_ok() {
                problems.push(format!(
                    "{} / {case}: accepted but must be rejected",
                    fixture.name
                ));
            }
        }
    }
    assert!(problems.is_empty(), "\n{}", problems.join("\n"));
}

#[test]
fn the_error_code_fixture_lists_every_rust_code() {
    let path = fixtures_root().join("errors/AppErrorCode.valid.json");
    let listed: Vec<Value> = cases(&path).into_iter().map(|(_, value)| value).collect();
    let ours: Vec<Value> = AppErrorCode::ALL
        .iter()
        .map(|code| Value::String(code.as_str().to_owned()))
        .collect();
    assert_eq!(listed, ours);
}

#[test]
fn every_host_event_variant_and_outcome_has_a_fixture() {
    let path = fixtures_root().join("events/HostEvent.valid.json");
    let values: Vec<Value> = cases(&path).into_iter().map(|(_, value)| value).collect();
    let types: BTreeSet<&str> = values
        .iter()
        .filter_map(|value| value["type"].as_str())
        .collect();
    for variant in ["token", "progress", "step", "log", "approval", "end"] {
        assert!(
            types.contains(variant),
            "no fixture for the {variant} event"
        );
    }
    let outcomes: BTreeSet<&str> = values
        .iter()
        .filter_map(|value| value["outcome"]["status"].as_str())
        .collect();
    for status in ["ok", "cancelled", "failed"] {
        assert!(
            outcomes.contains(status),
            "no end fixture with status {status}"
        );
    }
}

fn plaintext_keys(value: &Value, path: &str, found: &mut Vec<String>) {
    match value {
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                plaintext_keys(item, &format!("{path}.{index}"), found);
            }
        }
        Value::Object(entries) => {
            for (key, entry) in entries {
                if key == "options" {
                    continue;
                }
                if PLAINTEXT_KEYS.contains(&key.as_str()) {
                    found.push(format!("{path}.{key}"));
                }
                plaintext_keys(entry, &format!("{path}.{key}"), found);
            }
        }
        _ => {}
    }
}

#[test]
fn no_secrets_output_fixture_carries_a_value_field() {
    let mut problems = Vec::new();
    for fixture in all_fixtures()
        .iter()
        .filter(|fixture| fixture.kind == Kind::Valid)
        .filter(|fixture| fixture.name.starts_with("secrets/"))
    {
        let is_output = SECRET_OUTPUTS.contains(&fixture.name.as_str());
        let is_input = fixture.name.ends_with("Input")
            || matches!(
                fixture.name.as_str(),
                "secrets/SecretFilter" | "secrets/SecretRef"
            );
        assert!(
            is_output || is_input,
            "{} is neither a known secrets output nor an input",
            fixture.name
        );
        if !is_output {
            continue;
        }
        for (case, value) in cases(&fixture.path) {
            let mut found = Vec::new();
            plaintext_keys(&value, "", &mut found);
            if !found.is_empty() {
                problems.push(format!("{} / {case}: {found:?}", fixture.name));
            }
        }
    }
    assert!(
        problems.is_empty(),
        "
{}",
        problems.join(
            "
"
        )
    );
}

#[test]
fn the_rust_registry_matches_the_registry_fixture() {
    let path = fixtures_root().join("secrets/SecretsTypesOutput.valid.json");
    let (_, fixture) = cases(&path).remove(0);
    assert_eq!(
        serde_json::to_value(secret_type_registry()).expect("serialises"),
        fixture
    );
}
