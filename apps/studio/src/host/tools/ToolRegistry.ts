import { z } from "zod";
import {
  AppError,
  AppErrorCode,
  Json,
  ToolDetailDto,
  ToolDto,
  ToolKey,
  ToolListFilter,
  type ToolListRequest,
  ToolReferenceDto,
  ToolReferenceKind,
  ToolSource,
  ToolTier,
  ToolUsageDto,
} from "@zvs/shared";
import type { UnitOfWork } from "../data/UnitOfWork.ts";
import type { Repositories } from "../data/repositories/index.ts";
import type { ToolEntity, ToolReferenceEntity } from "../data/schema/index.ts";
import type { Logger } from "../platform/logger.ts";
import { createId } from "../platform/ids.ts";

export const ToolDescriptor = z.object({
  key: ToolKey,
  source: ToolSource,
  sourceRef: z.string().trim().min(1).max(256).nullable().default(null),
  displayName: z.string().trim().min(1).max(200).optional(),
  description: z.string().trim().max(4000).default(""),
  inputSchema: Json.default(null),
  readOnly: z.boolean().default(false),
  destructive: z.boolean().default(false),
});
export type ToolDescriptor = z.infer<typeof ToolDescriptor>;
export type ToolDescriptorRequest = z.input<typeof ToolDescriptor>;

export const ToolReferenceSubject = z.object({
  kind: ToolReferenceKind,
  refId: z.string().trim().min(1).max(128),
  name: z.string().trim().min(1).max(200),
});
export type ToolReferenceSubject = z.infer<typeof ToolReferenceSubject>;

export interface ToolCeiling {
  readonly key: string;
  readonly tier: ToolTier;
  readonly enabled: boolean;
  readonly available: boolean;
}

export interface ToolRegistryOptions {
  data: UnitOfWork;
  logger?: Logger;
  clock?: () => number;
}

export function defaultToolTier(
  descriptor: Pick<ToolDescriptor, "readOnly" | "destructive">,
): ToolTier {
  if (descriptor.destructive) return ToolTier.enum.off;
  return descriptor.readOnly ? ToolTier.enum.auto : ToolTier.enum.ask;
}

export class ToolRegistry {
  constructor(private readonly options: ToolRegistryOptions) {}

  register(input: ToolDescriptorRequest): ToolDto {
    const descriptor = ToolDescriptor.parse(input);
    const now = this.now();
    const row = this.options.data.transaction((repositories) => {
      const existing = repositories.tools.findByKey(descriptor.key);
      if (!existing) {
        const tier = defaultToolTier(descriptor);
        const created = repositories.tools.insert({
          id: createId(),
          key: descriptor.key,
          source: descriptor.source,
          sourceRef: descriptor.sourceRef,
          displayName: descriptor.displayName ?? descriptor.key,
          description: descriptor.description,
          inputSchema: descriptor.inputSchema,
          readOnly: descriptor.readOnly,
          destructive: descriptor.destructive,
          tier,
          enabled: true,
          available: true,
          lastUsedAt: null,
          callCount: 0,
          registeredAt: now,
          updatedAt: now,
        });
        this.writeCeiling(repositories, created.key, tier, "registry", now);
        return created;
      }
      const tier =
        descriptor.destructive && !existing.destructive
          ? defaultToolTier(descriptor)
          : existing.tier;
      const updated = repositories.tools.update(existing.id, {
        source: descriptor.source,
        sourceRef: descriptor.sourceRef,
        displayName: descriptor.displayName ?? existing.displayName,
        description: descriptor.description,
        inputSchema: descriptor.inputSchema,
        readOnly: descriptor.readOnly,
        destructive: descriptor.destructive,
        tier,
        available: true,
        updatedAt: now,
      });
      this.writeCeiling(repositories, updated.key, tier, "registry", now);
      return updated;
    });
    this.options.logger?.log("info", "tools", "Registered a tool", {
      key: row.key,
      source: row.source,
      tier: row.tier,
    });
    return this.toDto(row);
  }

  deregister(key: string): ToolDto | undefined {
    const existing = this.repositories.tools.findByKey(ToolKey.parse(key));
    if (!existing) return undefined;
    return this.toDto(
      this.repositories.tools.update(existing.id, { available: false, updatedAt: this.now() }),
    );
  }

  deregisterSource(source: ToolSource, sourceRef: string | null = null): ToolDto[] {
    const now = this.now();
    return this.repositories.tools
      .listBySource(ToolSource.parse(source), sourceRef)
      .map((row) =>
        this.toDto(this.repositories.tools.update(row.id, { available: false, updatedAt: now })),
      );
  }

  list(filter: ToolListRequest = {}): ToolDto[] {
    const { source, tier, query, includeUnavailable } = ToolListFilter.parse(filter);
    const needle = query?.toLowerCase();
    return this.repositories.tools
      .list()
      .filter((row) => includeUnavailable || row.available)
      .filter((row) => source === undefined || row.source === source)
      .filter((row) => tier === undefined || row.tier === tier)
      .filter(
        (row) =>
          needle === undefined ||
          row.key.toLowerCase().includes(needle) ||
          row.displayName.toLowerCase().includes(needle) ||
          row.description.toLowerCase().includes(needle),
      )
      .map((row) => this.toDto(row));
  }

  get(id: string): ToolDetailDto {
    const row = this.require(id);
    return ToolDetailDto.parse({
      ...this.toDto(row),
      references: this.repositories.tools.references(row.id).map(toReference),
    });
  }

  usage(id: string): ToolUsageDto {
    const row = this.require(id);
    return ToolUsageDto.parse({
      id: row.id,
      key: row.key,
      callCount: row.callCount,
      lastUsedAt: row.lastUsedAt,
      references: this.repositories.tools.references(row.id).map(toReference),
    });
  }

  setTier(id: string, tier: ToolTier, actor = "user"): ToolDto {
    const next = ToolTier.parse(tier);
    const now = this.now();
    const row = this.require(id);
    return this.toDto(
      this.options.data.transaction((repositories) => {
        const updated = repositories.tools.update(row.id, { tier: next, updatedAt: now });
        this.writeCeiling(repositories, updated.key, next, actor, now);
        return updated;
      }),
    );
  }

  setEnabled(id: string, enabled: boolean): ToolDto {
    const row = this.require(id);
    return this.toDto(
      this.repositories.tools.update(row.id, {
        enabled: z.boolean().parse(enabled),
        updatedAt: this.now(),
      }),
    );
  }

  ceiling(key: string): ToolCeiling | undefined {
    const row = this.repositories.tools.findByKey(key);
    if (!row) return undefined;
    return { key: row.key, tier: row.tier, enabled: row.enabled, available: row.available };
  }

  recordUse(key: string, usedAt: number = this.now()): void {
    const row = this.repositories.tools.findByKey(key);
    if (!row) return;
    this.repositories.tools.recordUse(row.id, usedAt);
  }

  recordReferences(subject: ToolReferenceSubject, keys: readonly string[]): void {
    const reference = ToolReferenceSubject.parse(subject);
    const rows = keys
      .map((key) => this.repositories.tools.findByKey(key))
      .filter((row): row is ToolEntity => row !== undefined);
    this.options.data.transaction((repositories) => {
      repositories.tools.clearReferences(
        reference.kind,
        reference.refId,
        rows.map((row) => row.id),
      );
      for (const row of rows)
        repositories.tools.linkReference({
          id: createId(),
          toolId: row.id,
          kind: reference.kind,
          refId: reference.refId,
          name: reference.name,
        });
    });
  }

  references(id: string): ToolReferenceDto[] {
    return this.repositories.tools.references(this.require(id).id).map(toReference);
  }

  private require(id: string): ToolEntity {
    const row = this.repositories.tools.findById(z.uuid().parse(id));
    if (!row) throw new AppError(AppErrorCode.NOT_FOUND, "Инструмент не найден");
    return row;
  }

  private writeCeiling(
    repositories: Repositories,
    subject: string,
    tier: ToolTier,
    grantedBy: string,
    grantedAt: number,
  ): void {
    const existing = repositories.permissions.list(subject).find((row) => row.scope === "global");
    repositories.permissions.grant({
      id: existing?.id ?? createId(),
      subject,
      scope: "global",
      tier,
      grantedBy,
      grantedAt,
    });
  }

  private toDto(row: ToolEntity): ToolDto {
    return ToolDto.parse({
      id: row.id,
      key: row.key,
      source: row.source,
      sourceRef: row.sourceRef,
      displayName: row.displayName,
      description: row.description,
      inputSchema: row.inputSchema ?? null,
      readOnly: row.readOnly,
      destructive: row.destructive,
      tier: row.tier,
      enabled: row.enabled,
      available: row.available,
      lastUsedAt: row.lastUsedAt,
      callCount: row.callCount,
      registeredAt: row.registeredAt,
      updatedAt: row.updatedAt,
    });
  }

  private get repositories(): Repositories {
    return this.options.data.repositories;
  }

  private now(): number {
    return (this.options.clock ?? Date.now)();
  }
}

function toReference(row: ToolReferenceEntity): ToolReferenceDto {
  return ToolReferenceDto.parse({ kind: row.kind, refId: row.refId, name: row.name });
}
