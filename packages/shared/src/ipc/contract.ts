import { z } from "zod";
import {
  AgenticInstructInput,
  AgenticSessionDto,
  AgenticSessionRef,
  CreateAgenticSessionInput,
  ExtendAgenticSessionInput,
  SetAgenticModeInput,
  UpdateAgenticPlanInput,
} from "../agentic.js";
import {
  ChatContextDto,
  ChatContextInput,
  ChatEnabledTools,
  ChatEvalInput,
  ChatEvalRunDto,
  ChatSendInput,
  ConversationDetailDto,
  TruncateConversationInput,
  ConversationDto,
  ConversationRef,
  RetrievalMode,
  CreateConversationInput,
  BindFolderInput,
  BindFolderResultDto,
  PickFolderResultDto,
} from "../chat.js";
import { AnswerFolderRequestInput, FolderGrantDto, FolderGrantRef } from "../folders.js";
import { ChatAnswerInput, ChatSkipInput } from "../questions.js";
import {
  ClearMemoryInput,
  CreateMemoryInput,
  MemoryDto,
  MemoryRef,
  UpdateMemoryInput,
} from "../memory.js";
import {
  ExplorerFileDto,
  ExplorerListInput,
  ExplorerListingDto,
  ExplorerReadInput,
  ExplorerWatchDto,
} from "../explorer.js";
import {
  TerminalAttachDto,
  TerminalCreateInput,
  TerminalRef,
  TerminalRenameInput,
  TerminalResizeInput,
  TerminalSessionDto,
  TerminalShell,
  TerminalWriteInput,
} from "../terminal.js";
import { NotificationTargetDto } from "../notifications.js";
import { ArtifactApplicationDto, ArtifactRef, ArtifactSaveResult } from "../artifacts.js";
import {
  DocumentDto,
  DocumentHandle,
  DocumentPageDto,
  DocumentPageInput,
  DocumentRef,
  DocumentSaveResult,
} from "../documents.js";
import { RunDto, RunHandleDto, RunIdInput, StartRunInput } from "../runs/RunDto.js";
import { RunListFilter, RunPageDto } from "../runs/RunListFilter.js";
import { RunDetailDto } from "../runs/RunDetailDto.js";
import { StepDto } from "../runs/StepDto.js";
import { StreamId } from "../primitives/branded.js";
import { Timestamp } from "../primitives/time.js";
import { Json } from "../primitives/json.js";
import {
  DocumentId,
  DownloadId,
  McpServerId,
  SecretId,
  VectorStoreId,
} from "../primitives/branded.js";
import {
  CatalogueFilter,
  CatalogueItemDto,
  DiskUsageDto,
  DownloadDto,
  DownloadListFilter,
  DownloadRef,
  PrioritiseDownloadInput,
  StartDownloadInput,
} from "../downloads/index.js";
import { CreateSecretInput } from "../secrets/CreateSecretInput.js";
import { UpdateSecretInput } from "../secrets/UpdateSecretInput.js";
import { SecretDto, SecretScope, SecretSummaryDto } from "../secrets/SecretDto.js";
import { SecretTypeKey } from "../secrets/registry.js";
import { SecretTypeSchema } from "../secrets/SecretTypeSchema.js";
import {
  CreateProviderInput,
  ProbeRequest,
  ProbeResultDto,
  ProviderDto,
  ProviderListFilter,
  ProviderRef,
  ProviderSummaryDto,
  SetDefaultModelInput,
  UpdateProviderInput,
} from "../providers/index.js";
import { ProviderId } from "../primitives/branded.js";
import { defineContract } from "./defineContract.js";
import {
  CreateSkillInput,
  SkillDetailDto,
  SkillDto,
  SkillExportInput,
  SkillExportResultDto,
  SkillImportInput,
  SkillImportResultDto,
  SkillRef,
  SetSkillEnabledInput,
  SkillFileContentDto,
  SkillFileRef,
  SkillRemoveOutput,
  UpdateSkillInput,
} from "../skills/index.js";
import {
  CreateScenarioInput,
  SaveScenarioDraftInput,
  ScenarioDetailDto,
  ScenarioDraftDto,
  ScenarioDto,
  ScenarioNodeTypeDto,
  ScenarioRef,
  ScenarioRemoveOutput,
  ScenarioRetryStepInput,
  ScenarioRunInput,
  ScenarioRunNodeInput,
  ScenarioValidateInput,
  ScenarioValidationDto,
  ScenarioVersionDto,
  SetScenarioActiveInput,
  UpdateScenarioInput,
} from "../scenarios.js";
import { AccountDto, AccountLinkInput, AccountLinkResult, AccountRef } from "../accounts.js";
import { AdapterDescriptorDto } from "../ai.js";
import { DeviceProfileDto, ResourceSampleDto } from "../system.js";
import { InstallRuntimeInput, RuntimeOverviewDto, RuntimeRef } from "../runtimes/index.js";
import {
  ToolchainDto,
  ToolchainListInput,
  ToolchainRef,
  ToolchainSelectInput,
} from "../toolchain/index.js";
import {
  AddGraphEdgeInput,
  GraphEdgeDto,
  GraphEdgeRef,
  GraphNeighborDto,
  GraphNeighborsInput,
  GraphPathDto,
  GraphReconcileDto,
  GraphRelationTypeDto,
  GraphShortestPathInput,
  GraphSubgraphDto,
  GraphSubgraphInput,
  RemoveGraphEdgeInput,
} from "../graph/index.js";
import {
  CreateMcpServerInput,
  McpHealthDto,
  McpServerDetailDto,
  McpServerDto,
  McpServerRef,
  McpTestResultDto,
  UpdateMcpServerInput,
} from "../mcp/index.js";
import {
  SetToolEnabledInput,
  SetToolTierInput,
  ToolDefaultsDto,
  ToolDetailDto,
  ToolDto,
  ToolListFilter,
  ToolRef,
  ToolUsageDto,
} from "../tools/index.js";
import {
  CreateVectorStoreInput,
  UpdateVectorStoreInput,
  VectorStoreDto,
  VectorStoreRef,
  VectorSearchInput,
  VectorSearchHitDto,
  VectorSearchResultDto,
  VectorChunksInput,
  VectorChunkDto,
  VectorDocumentDto,
  VectorSourceDto,
  VectorSourceRef,
  AddVectorSourceInput,
  PickVectorSourceInput,
  PickVectorSourceResult,
  VectorIndexInput,
  AddVectorEvalCaseInput,
  VectorEvalCaseDto,
  VectorEvalCaseRef,
  VectorEvalRunDto,
  VectorEvalRunInput,
  VectorEvalSetDto,
} from "../vectorStores/index.js";

export const SecretFilter = z.object({
  scope: SecretScope.optional(),
  type: SecretTypeKey.optional(),
  query: z.string().max(128).optional(),
});
export type SecretFilter = z.infer<typeof SecretFilter>;

export const SecretRef = z.object({ id: SecretId });
export type SecretRef = z.infer<typeof SecretRef>;

export const SettingKey = z
  .string()
  .min(1)
  .max(128)
  .regex(/^[a-z][a-z0-9]*(\.[a-z0-9]+)*$/);
export type SettingKey = z.infer<typeof SettingKey>;

export const contract = defineContract({
  "chat.conversations.list": { input: z.void(), output: z.array(ConversationDto) },
  "chat.conversations.get": { input: ConversationRef, output: ConversationDetailDto },
  "chat.conversations.create": { input: CreateConversationInput, output: ConversationDto },
  "chat.conversations.remove": { input: ConversationRef, output: z.void() },
  "chat.conversations.rename": {
    input: ConversationRef.extend({ title: z.string().trim().min(1).max(200) }),
    output: ConversationDto,
  },
  "chat.conversations.updateStores": {
    input: ConversationRef.extend({ attachedStoreIds: z.array(VectorStoreId).max(64) }),
    output: ConversationDto,
  },
  "chat.evaluate": { input: ChatEvalInput, output: ChatEvalRunDto },
  "chat.conversations.updateRetrieval": {
    input: ConversationRef.extend({ retrievalMode: RetrievalMode }),
    output: ConversationDto,
  },
  "chat.conversations.updateRelations": {
    input: ConversationRef.extend({ useRelations: z.boolean() }),
    output: ConversationDto,
  },
  "chat.conversations.updateTools": {
    input: ConversationRef.extend({ enabledTools: ChatEnabledTools }),
    output: ConversationDto,
  },
  "chat.conversations.truncate": {
    input: TruncateConversationInput,
    output: ConversationDetailDto,
  },
  "chat.context": { input: ChatContextInput, output: ChatContextDto },
  "chat.send": { input: ChatSendInput, output: RunHandleDto },
  "chat.cancel": { input: RunIdInput, output: z.void() },
  "chat.answer": { input: ChatAnswerInput, output: z.void() },
  "chat.skip": { input: ChatSkipInput, output: z.void() },
  "chat.pickFolder": { input: z.void(), output: PickFolderResultDto },
  "chat.bindFolder": { input: BindFolderInput, output: BindFolderResultDto },
  "chat.unbindFolder": { input: ConversationRef, output: ConversationDto },
  "folders.list": { input: z.void(), output: z.array(FolderGrantDto) },
  "folders.revoke": { input: FolderGrantRef, output: z.void() },
  "folders.answer": { input: AnswerFolderRequestInput, output: z.void() },
  "agentic.list": { input: z.void(), output: z.array(AgenticSessionDto) },
  "agentic.get": { input: AgenticSessionRef, output: AgenticSessionDto },
  "agentic.create": { input: CreateAgenticSessionInput, output: AgenticSessionDto },
  "agentic.updatePlan": { input: UpdateAgenticPlanInput, output: AgenticSessionDto },
  "agentic.setMode": { input: SetAgenticModeInput, output: AgenticSessionDto },
  "agentic.instruct": { input: AgenticInstructInput, output: AgenticSessionDto },
  "agentic.extend": { input: ExtendAgenticSessionInput, output: AgenticSessionDto },
  "agentic.cancel": { input: AgenticSessionRef, output: z.void() },
  "agentic.remove": { input: AgenticSessionRef, output: z.void() },
  "memory.list": { input: z.void(), output: z.array(MemoryDto) },
  "memory.create": { input: CreateMemoryInput, output: MemoryDto },
  "memory.update": { input: UpdateMemoryInput, output: MemoryDto },
  "memory.remove": { input: MemoryRef, output: z.void() },
  "memory.clear": { input: ClearMemoryInput, output: z.void() },
  "workspace.list": { input: ExplorerListInput, output: ExplorerListingDto },
  "workspace.read": { input: ExplorerReadInput, output: ExplorerFileDto },
  "workspace.watch": { input: ConversationRef, output: ExplorerWatchDto },
  "workspace.unwatch": { input: ConversationRef, output: z.void() },
  "terminal.shells": { input: z.void(), output: z.array(TerminalShell) },
  "terminal.list": { input: ConversationRef, output: z.array(TerminalSessionDto) },
  "terminal.create": { input: TerminalCreateInput, output: TerminalSessionDto },
  "terminal.attach": { input: TerminalRef, output: TerminalAttachDto },
  "terminal.write": { input: TerminalWriteInput, output: z.void() },
  "terminal.resize": { input: TerminalResizeInput, output: z.void() },
  "terminal.rename": { input: TerminalRenameInput, output: TerminalSessionDto },
  "terminal.restart": { input: TerminalRef, output: TerminalSessionDto },
  "terminal.clear": { input: TerminalRef, output: z.void() },
  "terminal.close": { input: TerminalRef, output: z.void() },
  "documents.open": { input: DocumentRef, output: DocumentDto },
  "documents.page": { input: DocumentPageInput, output: DocumentPageDto },
  "documents.close": { input: DocumentHandle, output: z.void() },
  "documents.list": { input: z.void(), output: z.array(DocumentDto) },
  "documents.reveal": { input: DocumentHandle, output: z.void() },
  "documents.save": { input: DocumentHandle, output: DocumentSaveResult },
  "artifacts.save": { input: ArtifactRef, output: ArtifactSaveResult },
  "artifacts.reveal": { input: ArtifactRef, output: z.void() },
  "artifacts.open": { input: ArtifactRef, output: z.void() },
  "artifacts.application": { input: ArtifactRef, output: ArtifactApplicationDto },
  "runs.approve": {
    input: RunIdInput.extend({ approvalId: z.string().uuid(), always: z.boolean().default(false) }),
    output: z.void(),
  },
  "runs.deny": { input: RunIdInput.extend({ approvalId: z.string().uuid() }), output: z.void() },
  "runs.start": { input: StartRunInput, output: RunHandleDto },
  "runs.cancel": { input: RunIdInput, output: z.void() },
  "runs.list": { input: RunListFilter, output: RunPageDto },
  "runs.get": { input: RunIdInput, output: RunDto },
  "runs.steps": { input: RunIdInput, output: z.array(StepDto) },
  "runs.detail": { input: RunIdInput, output: RunDetailDto },
  "runs.retry": { input: RunIdInput, output: RunHandleDto },
  "runs.clearFinished": {
    input: z.void(),
    output: z.object({ removed: z.number().int().nonnegative() }),
  },
  "vectorStores.list": { input: z.void(), output: z.array(VectorStoreDto) },
  "vectorStores.get": { input: VectorStoreRef, output: VectorStoreDto },
  "vectorStores.create": { input: CreateVectorStoreInput, output: VectorStoreDto },
  "vectorStores.update": { input: UpdateVectorStoreInput, output: VectorStoreDto },
  "vectorStores.remove": {
    input: VectorStoreRef,
    output: VectorStoreRef.extend({ removed: z.literal(true) }),
  },
  "vectorStores.search": { input: VectorSearchInput, output: z.array(VectorSearchHitDto) },
  "vectorStores.searchTimed": { input: VectorSearchInput, output: VectorSearchResultDto },
  "vectorStores.chunks": { input: VectorChunksInput, output: z.array(VectorChunkDto) },
  "vectorStores.reconcile": { input: VectorStoreRef, output: VectorStoreDto },
  "vectorStores.clear": { input: VectorStoreRef, output: VectorStoreDto },
  "vectorStores.sources.list": { input: VectorStoreRef, output: z.array(VectorSourceDto) },
  "vectorStores.sources.add": { input: AddVectorSourceInput, output: VectorSourceDto },
  "vectorStores.sources.remove": {
    input: VectorSourceRef,
    output: VectorSourceRef.extend({ removed: z.literal(true) }),
  },
  "vectorStores.sources.pick": { input: PickVectorSourceInput, output: PickVectorSourceResult },
  "vectorStores.documents.list": { input: VectorStoreRef, output: z.array(VectorDocumentDto) },
  "vectorStores.documents.remove": {
    input: z.object({ storeId: VectorStoreId, id: DocumentId }),
    output: z.object({ id: DocumentId, removed: z.literal(true) }),
  },
  "vectorStores.index": { input: VectorIndexInput, output: RunHandleDto },
  "vectorStores.eval.list": { input: VectorStoreRef, output: VectorEvalSetDto },
  "vectorStores.eval.add": { input: AddVectorEvalCaseInput, output: VectorEvalCaseDto },
  "vectorStores.eval.remove": {
    input: VectorEvalCaseRef,
    output: VectorEvalCaseRef.pick({ id: true }).extend({ removed: z.literal(true) }),
  },
  "vectorStores.eval.run": { input: VectorEvalRunInput, output: VectorEvalRunDto },
  "graphRelations.list": { input: z.void(), output: z.array(GraphRelationTypeDto) },
  "graphRelations.listEdges": { input: VectorStoreRef, output: z.array(GraphEdgeDto) },
  "graphRelations.addEdge": { input: AddGraphEdgeInput, output: GraphEdgeDto },
  "graphRelations.removeEdge": {
    input: RemoveGraphEdgeInput,
    output: GraphEdgeRef.extend({ removed: z.literal(true) }),
  },
  "graphRelations.neighbors": { input: GraphNeighborsInput, output: z.array(GraphNeighborDto) },
  "graphRelations.shortestPath": {
    input: GraphShortestPathInput,
    output: GraphPathDto.nullable(),
  },
  "graphRelations.kHopSubgraph": { input: GraphSubgraphInput, output: GraphSubgraphDto },
  "graphRelations.reconcile": { input: VectorStoreRef, output: GraphReconcileDto },
  "tools.list": { input: ToolListFilter, output: z.array(ToolDto) },
  "tools.get": { input: ToolRef, output: ToolDetailDto },
  "tools.setTier": { input: SetToolTierInput, output: ToolDto },
  "tools.setEnabled": { input: SetToolEnabledInput, output: ToolDto },
  "tools.usage": { input: ToolRef, output: ToolUsageDto },
  "tools.defaults": { input: z.void(), output: ToolDefaultsDto },
  "tools.setDefaults": { input: ToolDefaultsDto, output: ToolDefaultsDto },
  "mcp.list": { input: z.void(), output: z.array(McpServerDto) },
  "mcp.get": { input: McpServerRef, output: McpServerDetailDto },
  "mcp.create": { input: CreateMcpServerInput, output: McpServerDto },
  "mcp.update": { input: UpdateMcpServerInput, output: McpServerDto },
  "mcp.remove": {
    input: McpServerRef,
    output: z.object({ id: McpServerId, removed: z.literal(true) }),
  },
  "mcp.start": { input: McpServerRef, output: McpServerDto },
  "mcp.stop": { input: McpServerRef, output: McpServerDto },
  "mcp.test": { input: McpServerRef, output: McpTestResultDto },
  "mcp.health": { input: McpServerRef, output: McpHealthDto },
  "mcp.signIn": { input: McpServerRef, output: McpServerDto },
  "mcp.cancelSignIn": { input: McpServerRef, output: McpServerDto },
  "mcp.signOut": { input: McpServerRef, output: McpServerDto },
  "skills.list": { input: z.void(), output: z.array(SkillDto) },
  "skills.get": { input: SkillRef, output: SkillDetailDto },
  "skills.create": { input: CreateSkillInput, output: SkillDetailDto },
  "skills.update": { input: UpdateSkillInput, output: SkillDetailDto },
  "skills.remove": { input: SkillRef, output: SkillRemoveOutput },
  "skills.setEnabled": { input: SetSkillEnabledInput, output: SkillDto },
  "skills.duplicate": { input: SkillRef, output: SkillDetailDto },
  "skills.readFile": { input: SkillFileRef, output: SkillFileContentDto },
  "skills.import": { input: SkillImportInput, output: SkillImportResultDto },
  "skills.export": { input: SkillExportInput, output: SkillExportResultDto },
  "scenarios.list": { input: z.void(), output: z.array(ScenarioDto) },
  "scenarios.get": { input: ScenarioRef, output: ScenarioDetailDto },
  "scenarios.create": { input: CreateScenarioInput, output: ScenarioDetailDto },
  "scenarios.update": { input: UpdateScenarioInput, output: ScenarioDetailDto },
  "scenarios.remove": { input: ScenarioRef, output: ScenarioRemoveOutput },
  "scenarios.validate": { input: ScenarioValidateInput, output: ScenarioValidationDto },
  "scenarios.activate": { input: SetScenarioActiveInput, output: ScenarioDto },
  "scenarios.versions": { input: ScenarioRef, output: z.array(ScenarioVersionDto) },
  "scenarios.catalogue": { input: z.void(), output: z.array(ScenarioNodeTypeDto) },
  "scenarios.draft": { input: ScenarioRef, output: ScenarioDraftDto.nullable() },
  "scenarios.saveDraft": { input: SaveScenarioDraftInput, output: ScenarioDraftDto },
  "scenarios.discardDraft": { input: ScenarioRef, output: ScenarioRef },
  "scenarios.run": { input: ScenarioRunInput, output: RunHandleDto },
  "scenarios.runNode": { input: ScenarioRunNodeInput, output: RunHandleDto },
  "scenarios.retryStep": { input: ScenarioRetryStepInput, output: RunHandleDto },
  "downloads.list": { input: DownloadListFilter, output: z.array(DownloadDto) },
  "downloads.start": { input: StartDownloadInput, output: DownloadDto },
  "downloads.pause": { input: DownloadRef, output: DownloadDto },
  "downloads.resume": { input: DownloadRef, output: DownloadDto },
  "downloads.cancel": { input: DownloadRef, output: DownloadDto },
  "downloads.remove": {
    input: DownloadRef,
    output: z.object({ id: DownloadId, removed: z.literal(true) }),
  },
  "downloads.prioritise": { input: PrioritiseDownloadInput, output: DownloadDto },
  "downloads.catalogue": { input: CatalogueFilter, output: z.array(CatalogueItemDto) },
  "downloads.disk": {
    input: z.object({ refresh: z.boolean().default(false) }),
    output: DiskUsageDto,
  },
  "accounts.list": { input: z.void(), output: z.array(AccountDto) },
  "accounts.link": { input: AccountLinkInput, output: AccountLinkResult },
  "accounts.cancelLink": { input: AccountLinkInput, output: z.object({ cancelled: z.boolean() }) },
  "accounts.unlink": {
    input: AccountRef,
    output: AccountRef.extend({
      removed: z.literal(true),
      browserSessionPreserved: z.literal(true),
    }),
  },
  "accounts.refresh": { input: AccountRef, output: AccountDto },
  "system.ping": {
    input: z.object({ sentAt: Timestamp }),
    output: z.object({
      pong: z.literal(true),
      hostTime: Timestamp,
      roundTripHint: z.number(),
    }),
  },
  "system.demoStream": {
    input: z.object({ steps: z.number().int().min(1).max(100) }),
    output: z.object({ streamId: StreamId }),
  },
  "system.demoJob": {
    input: z.object({
      steps: z.number().int().min(1).max(1000),
      intervalMs: z.number().int().min(0).max(5_000).optional(),
    }),
    output: RunHandleDto,
  },
  "system.device": {
    input: z.object({ refresh: z.boolean().default(false) }),
    output: DeviceProfileDto,
  },
  "system.resources": { input: z.void(), output: ResourceSampleDto },
  "runtimes.list": { input: z.void(), output: RuntimeOverviewDto },
  "runtimes.install": { input: InstallRuntimeInput, output: z.array(DownloadDto) },
  "runtimes.stop": { input: RuntimeRef, output: z.object({ stopped: z.boolean() }) },
  "runtimes.remove": { input: RuntimeRef, output: RuntimeRef.extend({ removed: z.literal(true) }) },
  "toolchain.list": { input: ToolchainListInput, output: z.array(ToolchainDto) },
  "toolchain.install": { input: ToolchainRef, output: ToolchainDto },
  "toolchain.scan": { input: ToolchainRef, output: ToolchainDto },
  "toolchain.select": { input: ToolchainSelectInput, output: ToolchainDto },
  "toolchain.update": { input: ToolchainRef, output: ToolchainDto },
  "toolchain.remove": { input: ToolchainRef, output: ToolchainDto },
  "system.nativePing": {
    input: z.object({ text: z.string().max(1_000_000) }),
    output: z.object({ count: z.number().int().nonnegative() }),
  },
  "settings.get": {
    input: z.object({ key: SettingKey }),
    output: z.object({
      key: SettingKey,
      value: Json.optional(),
      updatedAt: Timestamp.optional(),
    }),
  },
  "notifications.clear": { input: NotificationTargetDto, output: z.void() },
  "notifications.pending": { input: z.void(), output: NotificationTargetDto.nullable() },
  "settings.set": {
    input: z.object({ key: SettingKey, value: Json }),
    output: z.object({ key: SettingKey, value: Json, updatedAt: Timestamp }),
  },
  "secrets.types": {
    input: z.void(),
    output: z.array(SecretTypeSchema),
  },
  "secrets.list": {
    input: SecretFilter,
    output: z.array(SecretSummaryDto),
  },
  "secrets.get": {
    input: SecretRef,
    output: SecretDto,
  },
  "secrets.create": {
    input: CreateSecretInput,
    output: SecretDto,
  },
  "secrets.update": {
    input: UpdateSecretInput,
    output: SecretDto,
  },
  "secrets.remove": {
    input: SecretRef,
    output: z.object({ id: SecretId, removed: z.literal(true) }),
  },
  "providers.adapters": {
    input: z.void(),
    output: z.array(AdapterDescriptorDto),
  },
  "providers.list": {
    input: ProviderListFilter,
    output: z.array(ProviderSummaryDto),
  },
  "providers.get": {
    input: ProviderRef.extend({ selectedOnly: z.boolean().optional() }),
    output: ProviderDto,
  },
  "providers.create": {
    input: CreateProviderInput,
    output: ProviderDto,
  },
  "providers.update": {
    input: UpdateProviderInput,
    output: ProviderDto,
  },
  "providers.remove": {
    input: ProviderRef,
    output: z.object({ id: ProviderId, removed: z.literal(true) }),
  },
  "providers.probe": {
    input: ProbeRequest,
    output: ProbeResultDto,
  },
  "providers.setDefaultModel": {
    input: SetDefaultModelInput,
    output: ProviderDto,
  },
  "providers.refreshAll": {
    input: z.void(),
    output: z.array(ProviderSummaryDto),
  },
});

export type Contract = typeof contract;
