import { readdirSync, readFileSync } from "node:fs";
import { join, relative, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { z } from "zod";
import {
  AccountDto,
  ChatCitationDto,
  ChatSettings,
  ChatToolCallDto,
  ChatToolCallStatus,
  ConversationDetailDto,
  ConversationDto,
  CreateConversationInput,
  MessageDto,
  AdapterCapabilitiesDto,
  AdapterDescriptorDto,
  AppErrorCode,
  BrowserBounds,
  BrowserCommand,
  contract,
  DeviceGpuDto,
  DeviceProfileDto,
  DeviceTier,
  DiscoveredModelDto,
  HostEvent,
  Json,
  ModelDto,
  Page,
  PageRequest,
  ProbeOutcomeDto,
  ProviderConnectionInput,
  ProviderDto,
  ProviderId,
  ProviderSettingsDto,
  ProviderSummaryDto,
  ResourceSampleDto,
  Result,
  RunCountsDto,
  RunKind,
  RunOutcomeDto,
  RunStatus,
  RunSummaryDto,
  GraphNode,
  RunGraph,
  StepDto,
  StepStatus,
  ToolTier,
  SECRET_TYPE_REGISTRY,
  SecretDto,
  SecretSummaryDto,
  SecretTypeSchema,
  Sort,
  SortDirection,
  StreamId,
  GraphEdgeDto,
  GraphNeighborDto,
  GraphPathDto,
  GraphRelationTypeDto,
  GraphSubgraphDto,
  VectorChunkDto,
  VectorDocumentDto,
  VectorIndexReportDto,
  VectorOcrConfig,
  VectorRerankConfig,
  VectorSearchHitDto,
  VectorSourceDto,
  VectorStoreDto,
  Accelerator,
  CatalogueItemDto,
  CatalogueItemState,
  ChecksumDto,
  DiskCategory,
  DiskUsageDto,
  DownloadDto,
  DownloadFetchInput,
  DownloadItemKind,
  DownloadReportDto,
  DownloadStatus,
  ModelFormat,
  RuntimeDto,
  RuntimeEngine,
  RuntimePlanDto,
  RuntimeState,
  ToolSource,
  ToolReferenceKind,
  ToolDefaultTier,
  ToolReferenceDto,
  ToolDto,
  ToolDetailDto,
  ToolUsageDto,
  ToolListFilter,
  ToolDefaultsDto,
  ToolInvokeInput,
  ToolInvokeResultDto,
  McpTransport,
  McpStartup,
  McpServerStatus,
  McpCapabilityKind,
  McpValue,
  McpServerDto,
  McpCapabilityDto,
  McpServerDetailDto,
  McpTestResultDto,
  McpLogLineDto,
  McpHealthDto,
  McpServerRef,
  CreateMcpServerInput,
  UpdateMcpServerInput,
  McpContentDto,
  McpCallResultDto,
} from "../src/index.js";

const schemas: Record<string, z.ZodType> = {
  "primitives/ProviderId": ProviderId,
  "primitives/StreamId": StreamId,
  "primitives/PageRequest": PageRequest,
  "primitives/Page": Page(Json),
  "primitives/Sort": Sort(z.string()),
  "primitives/SortDirection": SortDirection,
  "errors/AppErrorCode": z.enum(AppErrorCode),
  "errors/ResultEnvelope": Result(Json),
  "events/RunOutcomeDto": RunOutcomeDto,
  "runs/RunStatus": RunStatus,
  "runs/RunKind": RunKind,
  "runs/StepStatus": StepStatus,
  "runs/GraphNode": GraphNode,
  "runs/RunGraph": RunGraph,
  "runs/StartRunInput": contract["runs.start"].input,
  "runs/RunIdInput": contract["runs.get"].input,
  "runs/RunHandleDto": contract["runs.start"].output,
  "runs/RunsApproveInput": contract["runs.approve"].input,
  "runs/RunsDenyInput": contract["runs.deny"].input,
  "runs/RunDto": contract["runs.get"].output,
  "runs/StepDto": StepDto,
  "runs/RunListFilter": contract["runs.list"].input,
  "runs/RunCountsDto": RunCountsDto,
  "runs/RunSummaryDto": RunSummaryDto,
  "runs/RunPageDto": contract["runs.list"].output,
  "runs/RunDetailDto": contract["runs.detail"].output,
  "runs/RunsClearFinishedOutput": contract["runs.clearFinished"].output,
  "tools/ToolTier": ToolTier,
  "tools/ToolSource": ToolSource,
  "tools/ToolReferenceKind": ToolReferenceKind,
  "tools/ToolDefaultTier": ToolDefaultTier,
  "tools/ToolReferenceDto": ToolReferenceDto,
  "tools/ToolDto": ToolDto,
  "tools/ToolDetailDto": ToolDetailDto,
  "tools/ToolUsageDto": ToolUsageDto,
  "tools/ToolRef": contract["tools.get"].input,
  "tools/ToolListFilter": ToolListFilter,
  "tools/SetToolTierInput": contract["tools.setTier"].input,
  "tools/SetToolEnabledInput": contract["tools.setEnabled"].input,
  "tools/ToolDefaultsDto": ToolDefaultsDto,
  "tools/ToolInvokeInput": ToolInvokeInput,
  "tools/ToolInvokeResultDto": ToolInvokeResultDto,
  "mcp/McpTransport": McpTransport,
  "mcp/McpStartup": McpStartup,
  "mcp/McpServerStatus": McpServerStatus,
  "mcp/McpCapabilityKind": McpCapabilityKind,
  "mcp/McpValue": McpValue,
  "mcp/McpServerDto": McpServerDto,
  "mcp/McpCapabilityDto": McpCapabilityDto,
  "mcp/McpServerDetailDto": McpServerDetailDto,
  "mcp/McpTestResultDto": McpTestResultDto,
  "mcp/McpLogLineDto": McpLogLineDto,
  "mcp/McpHealthDto": McpHealthDto,
  "mcp/McpServerRef": McpServerRef,
  "mcp/CreateMcpServerInput": CreateMcpServerInput,
  "mcp/UpdateMcpServerInput": UpdateMcpServerInput,
  "mcp/McpContentDto": McpContentDto,
  "mcp/McpCallResultDto": McpCallResultDto,
  "mcp/McpRemoveOutput": contract["mcp.remove"].output,
  "events/HostEvent": HostEvent,
  "system/DeviceTier": DeviceTier,
  "system/DeviceGpuDto": DeviceGpuDto,
  "system/DeviceProfileDto": DeviceProfileDto,
  "system/ResourceSampleDto": ResourceSampleDto,
  "system/SystemPingInput": contract["system.ping"].input,
  "system/SystemPingOutput": contract["system.ping"].output,
  "system/SystemDemoStreamInput": contract["system.demoStream"].input,
  "system/SystemDemoStreamOutput": contract["system.demoStream"].output,
  "system/SystemDemoJobInput": contract["system.demoJob"].input,
  "system/SystemDeviceInput": contract["system.device"].input,
  "system/SystemNativePingInput": contract["system.nativePing"].input,
  "system/SystemNativePingOutput": contract["system.nativePing"].output,
  "settings/SettingsGetInput": contract["settings.get"].input,
  "settings/SettingsGetOutput": contract["settings.get"].output,
  "settings/SettingsSetInput": contract["settings.set"].input,
  "settings/SettingsSetOutput": contract["settings.set"].output,
  "secrets/SecretTypeSchema": SecretTypeSchema,
  "secrets/SecretsTypesOutput": contract["secrets.types"].output,
  "secrets/SecretFilter": contract["secrets.list"].input,
  "secrets/SecretRef": contract["secrets.get"].input,
  "secrets/CreateSecretInput": contract["secrets.create"].input,
  "secrets/UpdateSecretInput": contract["secrets.update"].input,
  "secrets/SecretSummaryDto": SecretSummaryDto,
  "secrets/SecretDto": SecretDto,
  "secrets/SecretsRemoveOutput": contract["secrets.remove"].output,
  "ai/AdapterCapabilitiesDto": AdapterCapabilitiesDto,
  "ai/AdapterDescriptorDto": AdapterDescriptorDto,
  "providers/ProviderSettingsDto": ProviderSettingsDto,
  "providers/DiscoveredModelDto": DiscoveredModelDto,
  "providers/ModelDto": ModelDto,
  "providers/ProviderSummaryDto": ProviderSummaryDto,
  "providers/ProviderDto": ProviderDto,
  "providers/ProviderConnectionInput": ProviderConnectionInput,
  "providers/CreateProviderInput": contract["providers.create"].input,
  "providers/UpdateProviderInput": contract["providers.update"].input,
  "providers/ProviderListFilter": contract["providers.list"].input,
  "providers/ProviderRef": contract["providers.remove"].input,
  "providers/ProvidersGetInput": contract["providers.get"].input,
  "providers/SetDefaultModelInput": contract["providers.setDefaultModel"].input,
  "providers/ProvidersRemoveOutput": contract["providers.remove"].output,
  "providers/ProbeOutcomeDto": ProbeOutcomeDto,
  "providers/ProbeRequest": contract["providers.probe"].input,
  "providers/ProbeResultDto": contract["providers.probe"].output,
  "browser/BrowserBounds": BrowserBounds,
  "browser/BrowserCommand": BrowserCommand,
  "accounts/AccountDto": AccountDto,
  "accounts/AccountLinkInput": contract["accounts.link"].input,
  "accounts/AccountRef": contract["accounts.refresh"].input,
  "accounts/AccountLinkResult": contract["accounts.link"].output,
  "accounts/AccountsCancelLinkOutput": contract["accounts.cancelLink"].output,
  "accounts/AccountsUnlinkOutput": contract["accounts.unlink"].output,
  "vectorStores/VectorRerankConfig": VectorRerankConfig,
  "vectorStores/VectorOcrConfig": VectorOcrConfig,
  "vectorStores/VectorStoreDto": VectorStoreDto,
  "vectorStores/VectorStoreRef": contract["vectorStores.get"].input,
  "vectorStores/CreateVectorStoreInput": contract["vectorStores.create"].input,
  "vectorStores/UpdateVectorStoreInput": contract["vectorStores.update"].input,
  "vectorStores/VectorStoresRemoveOutput": contract["vectorStores.remove"].output,
  "vectorStores/VectorSearchInput": contract["vectorStores.search"].input,
  "vectorStores/VectorSearchHitDto": VectorSearchHitDto,
  "vectorStores/VectorSearchResultDto": contract["vectorStores.searchTimed"].output,
  "vectorStores/VectorChunksInput": contract["vectorStores.chunks"].input,
  "vectorStores/VectorChunkDto": VectorChunkDto,
  "vectorStores/VectorSourceDto": VectorSourceDto,
  "vectorStores/AddVectorSourceInput": contract["vectorStores.sources.add"].input,
  "vectorStores/VectorSourceRef": contract["vectorStores.sources.remove"].input,
  "vectorStores/VectorSourcesRemoveOutput": contract["vectorStores.sources.remove"].output,
  "vectorStores/PickVectorSourceInput": contract["vectorStores.sources.pick"].input,
  "vectorStores/PickVectorSourceResult": contract["vectorStores.sources.pick"].output,
  "vectorStores/VectorIndexInput": contract["vectorStores.index"].input,
  "vectorStores/VectorIndexReportDto": VectorIndexReportDto,
  "vectorStores/VectorDocumentDto": VectorDocumentDto,
  "vectorStores/VectorDocumentsRemoveInput": contract["vectorStores.documents.remove"].input,
  "vectorStores/VectorDocumentsRemoveOutput": contract["vectorStores.documents.remove"].output,
  "vectorStores/VectorEvalCaseDto": contract["vectorStores.eval.add"].output,
  "vectorStores/AddVectorEvalCaseInput": contract["vectorStores.eval.add"].input,
  "vectorStores/VectorEvalCaseRef": contract["vectorStores.eval.remove"].input,
  "vectorStores/VectorEvalRemoveOutput": contract["vectorStores.eval.remove"].output,
  "vectorStores/VectorEvalRunInput": contract["vectorStores.eval.run"].input,
  "vectorStores/VectorEvalRunDto": contract["vectorStores.eval.run"].output,
  "vectorStores/VectorEvalSetDto": contract["vectorStores.eval.list"].output,
  "graph/GraphRelationTypeDto": GraphRelationTypeDto,
  "graph/GraphEdgeDto": GraphEdgeDto,
  "graph/AddGraphEdgeInput": contract["graphRelations.addEdge"].input,
  "graph/RemoveGraphEdgeInput": contract["graphRelations.removeEdge"].input,
  "graph/GraphRemoveEdgeOutput": contract["graphRelations.removeEdge"].output,
  "graph/GraphNeighborDto": GraphNeighborDto,
  "graph/GraphPathDto": GraphPathDto,
  "graph/GraphShortestPathOutput": contract["graphRelations.shortestPath"].output,
  "graph/GraphSubgraphDto": GraphSubgraphDto,
  "graph/GraphNeighborsInput": contract["graphRelations.neighbors"].input,
  "graph/GraphShortestPathInput": contract["graphRelations.shortestPath"].input,
  "graph/GraphSubgraphInput": contract["graphRelations.kHopSubgraph"].input,
  "graph/GraphReconcileDto": contract["graphRelations.reconcile"].output,
  "downloads/DownloadItemKind": DownloadItemKind,
  "downloads/DownloadStatus": DownloadStatus,
  "downloads/CatalogueItemState": CatalogueItemState,
  "downloads/DiskCategory": DiskCategory,
  "downloads/ChecksumDto": ChecksumDto,
  "downloads/CatalogueItemDto": CatalogueItemDto,
  "downloads/DownloadDto": DownloadDto,
  "downloads/DiskUsageDto": DiskUsageDto,
  "downloads/DownloadRef": contract["downloads.pause"].input,
  "downloads/PrioritiseDownloadInput": contract["downloads.prioritise"].input,
  "downloads/StartDownloadInput": contract["downloads.start"].input,
  "downloads/DownloadListFilter": contract["downloads.list"].input,
  "downloads/CatalogueFilter": contract["downloads.catalogue"].input,
  "downloads/DownloadsDiskInput": contract["downloads.disk"].input,
  "downloads/DownloadsRemoveOutput": contract["downloads.remove"].output,
  "downloads/DownloadFetchInput": DownloadFetchInput,
  "downloads/DownloadReportDto": DownloadReportDto,
  "runtimes/ModelFormat": ModelFormat,
  "runtimes/Accelerator": Accelerator,
  "runtimes/RuntimeEngine": RuntimeEngine,
  "runtimes/RuntimeState": RuntimeState,
  "runtimes/RuntimeDto": RuntimeDto,
  "runtimes/RuntimePlanDto": RuntimePlanDto,
  "runtimes/RuntimeOverviewDto": contract["runtimes.list"].output,
  "runtimes/RuntimeRef": contract["runtimes.stop"].input,
  "runtimes/InstallRuntimeInput": contract["runtimes.install"].input,
  "runtimes/RuntimesStopOutput": contract["runtimes.stop"].output,
  "runtimes/RuntimesRemoveOutput": contract["runtimes.remove"].output,
  "chat/ChatSettings": ChatSettings,
  "chat/ChatCitationDto": ChatCitationDto,
  "chat/ChatToolCallStatus": ChatToolCallStatus,
  "chat/ChatToolCallDto": ChatToolCallDto,
  "chat/CreateConversationInput": CreateConversationInput,
  "chat/ConversationRef": contract["chat.conversations.get"].input,
  "chat/ConversationDto": ConversationDto,
  "chat/MessageDto": MessageDto,
  "chat/ConversationDetailDto": ConversationDetailDto,
  "chat/ChatSendInput": contract["chat.send"].input,
  "chat/ChatContextInput": contract["chat.context"].input,
  "chat/ChatContextDto": contract["chat.context"].output,
  "chat/TruncateConversationInput": contract["chat.conversations.truncate"].input,
  "chat/RenameConversationInput": contract["chat.conversations.rename"].input,
  "chat/UpdateConversationStoresInput": contract["chat.conversations.updateStores"].input,
  "chat/UpdateConversationRelationsInput": contract["chat.conversations.updateRelations"].input,
  "chat/UpdateConversationToolsInput": contract["chat.conversations.updateTools"].input,
};

const root = fileURLToPath(new URL("../fixtures/", import.meta.url));

interface Fixture {
  name: string;
  kind: "valid" | "invalid";
  file: string;
}

const FixtureCases = z.array(z.object({ name: z.string(), value: z.unknown() })).min(1);

function collect(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) =>
    entry.isDirectory() ? collect(join(dir, entry.name)) : [join(dir, entry.name)],
  );
}

const fixtures: Fixture[] = collect(root)
  .sort()
  .map((file) => {
    const path = relative(root, file).split(sep).join("/");
    const match = /^(.+)\.(valid|invalid)\.json$/.exec(path);
    if (!match?.[1] || !match[2]) {
      throw new Error(`${path} is neither <Name>.valid.json nor <Name>.invalid.json`);
    }
    return { name: match[1], kind: match[2] as Fixture["kind"], file };
  });

function cases(file: string): { name: string; value: unknown }[] {
  return FixtureCases.parse(JSON.parse(readFileSync(file, "utf8")));
}

describe("golden fixtures", () => {
  it("map every fixture to a Zod schema and every schema to a valid fixture", () => {
    const problems: string[] = [];
    for (const fixture of fixtures) {
      if (!(fixture.name in schemas)) {
        problems.push(`${fixture.name} has no entry in the Zod fixture table`);
      }
    }
    const valid = new Set(fixtures.filter((f) => f.kind === "valid").map((f) => f.name));
    for (const name of Object.keys(schemas)) {
      if (!valid.has(name)) problems.push(`${name} is in the table but has no ${name}.valid.json`);
    }
    expect(problems).toEqual([]);
  });

  for (const fixture of fixtures) {
    const schema = schemas[fixture.name];
    if (!schema) continue;
    describe(`${fixture.name}.${fixture.kind}`, () => {
      for (const { name, value } of cases(fixture.file)) {
        if (fixture.kind === "valid") {
          it(`accepts ${name}`, () => {
            const parsed = schema.safeParse(value);
            expect(parsed.error).toBeUndefined();
          });
        } else {
          it(`rejects ${name}`, () => {
            expect(schema.safeParse(value).success).toBe(false);
          });
        }
      }
    });
  }

  it("lists every AppErrorCode in order", () => {
    const listed = cases(join(root, "errors/AppErrorCode.valid.json")).map((c) => c.value);
    expect(listed).toEqual(Object.values(AppErrorCode));
  });

  it("holds the secret type registry exactly as secrets.types returns it", () => {
    const [registry] = cases(join(root, "secrets/SecretsTypesOutput.valid.json"));
    expect(registry?.value).toEqual(SECRET_TYPE_REGISTRY);
  });
});
