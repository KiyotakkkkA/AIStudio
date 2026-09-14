import { access, mkdir, rm, stat } from "node:fs/promises";
import { constants } from "node:fs";
import { dirname } from "node:path";
import {
  AppError,
  AppErrorCode,
  CatalogueFilter,
  CatalogueItemDto,
  DiskUsageDto,
  DownloadDto,
  DownloadListFilter,
  DownloadReportDto,
  StartDownloadInput,
  type CatalogueItemState,
  type ChecksumDto,
  type DownloadItemKind,
  type DownloadStatus,
  type Json,
  type RunHandleDto,
  type Timestamp,
} from "@zvs/shared";
import type { UnitOfWork } from "../data/UnitOfWork.ts";
import type { DownloadEntity } from "../data/schema/index.ts";
import type { SidecarJobsPort } from "../drivers/sidecar/SidecarDriver.ts";
import type { RunService } from "../services/RunService.ts";
import { createId } from "../platform/ids.ts";
import { downloadTargetPath } from "../platform/paths.ts";
import type { Logger } from "../platform/logger.ts";
import type { SettingService } from "../services/SettingService.ts";
import { CatalogueService, installedKey, type CatalogueItem } from "./catalogue.ts";
import type { DiskService } from "./disk.ts";

export const DOWNLOAD_JOB = "job.download";
export const PART_SUFFIX = ".part";
export const CONCURRENCY_KEY = "downloads.concurrency";
export const DEFAULT_CONCURRENCY = 2;
/** Headroom kept free beyond the artefact itself, so a pull never fills the volume. */
export const FREE_SPACE_MARGIN_BYTES = 1024 ** 3;

/** Small artefacts jump the queue so a 22 MB MCP package is not stuck behind a 240 GB model. */
const PRIORITY_BY_KIND: Record<DownloadItemKind, number> = {
  mcp: 7,
  skill: 7,
  embedding: 6,
  model: 5,
};

export interface DownloadProgress {
  done: number;
  total: number;
  rate?: number;
}

export interface DownloadContext {
  signal?: AbortSignal;
  onProgress?: (progress: DownloadProgress) => void;
}

export type DownloadRunner = Pick<RunService, "start" | "cancel" | "subscribe">;

export interface DownloadServiceOptions {
  data: UnitOfWork;
  jobs: SidecarJobsPort;
  disk: DiskService;
  catalogue?: CatalogueService;
  runs?: DownloadRunner;
  settings?: SettingService;
  downloadsDir: string;
  logger?: Logger;
  clock?: () => number;
  concurrency?: number;
  /** How often a moving transfer writes its byte count back to the database. */
  persistIntervalMs?: number;
}

interface Live {
  rate: number;
  bytesDone: number;
  persistedAt: number;
}

type Intent = "pause" | "cancel";

export class DownloadService {
  private readonly live = new Map<string, Live>();
  private readonly intents = new Map<string, Intent>();
  private readonly catalogueService: CatalogueService;
  private runner: DownloadRunner | undefined;
  private closed = false;

  constructor(private readonly options: DownloadServiceOptions) {
    this.catalogueService = options.catalogue ?? new CatalogueService({ logger: options.logger });
    this.runner = options.runs;
  }

  /**
   * The kernel needs this service to execute a download node and this service needs the kernel
   * to start the run, so the two are joined after both exist rather than in a constructor.
   */
  attach(runs: DownloadRunner): void {
    this.runner = runs;
    this.pump();
  }

  get concurrency(): number {
    const stored = this.options.settings?.get(CONCURRENCY_KEY)?.value;
    if (typeof stored === "number" && Number.isInteger(stored) && stored >= 1 && stored <= 8)
      return stored;
    return this.options.concurrency ?? DEFAULT_CONCURRENCY;
  }

  list(raw: DownloadListFilter = DownloadListFilter.parse({})): DownloadDto[] {
    const filter = DownloadListFilter.parse(raw);
    return this.repository
      .list()
      .filter(
        (row) =>
          (filter.statuses === undefined || filter.statuses.includes(row.status)) &&
          (filter.kinds === undefined || filter.kinds.includes(row.itemKind)),
      )
      .map((row) => this.toDto(row));
  }

  get(id: string): DownloadDto {
    return this.toDto(this.require(id));
  }

  disk(refresh = false): Promise<DiskUsageDto> {
    return this.options.disk.usage(refresh);
  }

  /**
   * The catalogue as the page shows it: every known item, its install state, and — for the
   * mockup's "not enough free disk" row — why it cannot be started right now.
   */
  async catalogue(raw: CatalogueFilter = CatalogueFilter.parse({})): Promise<CatalogueItemDto[]> {
    const filter = CatalogueFilter.parse(raw);
    const [items, installed, freeBytes] = await Promise.all([
      this.catalogueService.list(filter.refresh),
      this.catalogueService.installed(filter.refresh),
      this.options.disk.freeBytes().catch(() => Number.POSITIVE_INFINITY),
    ]);
    const active = new Map<string, DownloadEntity>();
    const succeeded = new Map<string, DownloadEntity>();
    for (const row of this.repository.list()) {
      if (row.status === "succeeded") {
        if (!succeeded.has(row.itemRef)) succeeded.set(row.itemRef, row);
      } else if (!TERMINAL.includes(row.status) && !active.has(row.itemRef)) {
        active.set(row.itemRef, row);
      }
    }

    const query = filter.query?.toLowerCase();
    return items
      .map((item) => {
        const running = active.get(item.ref);
        const present = succeeded.get(item.ref);
        const reported = installed.get(installedKey(item.kind, item.name));
        const installedVersion = reported?.version ?? present?.version ?? undefined;
        const state = resolveState(item, running?.status, {
          installed: reported !== undefined || present !== undefined,
          installedVersion,
          digest: reported?.digest,
        });
        const blocked = this.precondition(item, freeBytes, running !== undefined);
        return CatalogueItemDto.parse({
          ref: item.ref,
          kind: item.kind,
          source: item.source,
          name: item.name,
          displayName: item.displayName,
          description: item.description,
          sizeBytes: item.sizeBytes,
          url: item.url,
          tags: [...item.tags],
          state,
          downloadable: state !== "installed" && blocked === undefined,
          ...(item.version === undefined ? {} : { version: item.version }),
          ...(installedVersion === undefined ? {} : { installedVersion }),
          ...(item.checksum === undefined ? {} : { checksum: item.checksum }),
          ...(item.dimension === undefined ? {} : { dimension: item.dimension }),
          ...(blocked === undefined ? {} : { blockedReason: blocked }),
          ...(running === undefined ? {} : { downloadId: running.id }),
        });
      })
      .filter(
        (item) =>
          (filter.kinds === undefined || filter.kinds.includes(item.kind)) &&
          (filter.states === undefined || filter.states.includes(item.state)) &&
          (query === undefined ||
            item.displayName.toLowerCase().includes(query) ||
            item.name.toLowerCase().includes(query)),
      );
  }

  async start(raw: StartDownloadInput): Promise<DownloadDto> {
    const input = StartDownloadInput.parse(raw);
    if (this.closed) throw new AppError(AppErrorCode.CONFLICT, "Сервис загрузок остановлен");
    const item = await this.catalogueService.find(input.ref);
    if (this.repository.findActiveByRef(item.ref))
      throw new AppError(AppErrorCode.CONFLICT, "Этот элемент уже в очереди загрузки");

    const targetPath = downloadTargetPath(this.options.downloadsDir, item.kind, item.fileName);
    await this.requireWritable(dirname(targetPath));
    const blocked = this.precondition(
      item,
      await this.freeBytes(),
      false,
      await this.partial(targetPath),
    );
    if (blocked !== undefined) throw new AppError(AppErrorCode.CONFLICT, blocked);

    const at = this.now();
    const row = this.repository.create({
      id: createId(),
      itemKind: item.kind,
      itemRef: item.ref,
      displayName: item.displayName,
      version: item.version ?? null,
      sizeBytes: item.sizeBytes,
      bytesDone: await this.partial(targetPath),
      status: "queued",
      priority: input.priority ?? PRIORITY_BY_KIND[item.kind],
      targetPath,
      url: item.url,
      checksumAlgorithm: item.checksum?.algorithm ?? null,
      checksumValue: item.checksum?.value ?? null,
      createdAt: at,
      updatedAt: at,
    });
    this.options.logger?.log("info", "downloads", "Queued a download", {
      id: row.id,
      ref: item.ref,
      sizeBytes: item.sizeBytes,
      priority: row.priority,
    });
    this.pump();
    return this.toDto(this.repository.findById(row.id) ?? row);
  }

  async pause(id: string): Promise<DownloadDto> {
    const row = this.require(id);
    if (row.status === "queued") return this.settle(id, "paused");
    if (row.status !== "running")
      throw new AppError(AppErrorCode.CONFLICT, "Загрузка не выполняется");
    // A pause keeps the partial file; the transfer resumes from it with a range request.
    this.intents.set(id, "pause");
    await this.abort(row);
    return this.get(id);
  }

  resume(id: string): DownloadDto {
    const row = this.require(id);
    if (row.status !== "paused" && row.status !== "failed")
      throw new AppError(
        AppErrorCode.CONFLICT,
        "Возобновить можно только приостановленную загрузку",
      );
    const updated = this.repository.update(id, {
      status: "queued",
      error: null,
      updatedAt: this.now(),
    });
    this.pump();
    return this.toDto(updated);
  }

  async cancel(id: string): Promise<DownloadDto> {
    const row = this.require(id);
    if (TERMINAL.includes(row.status)) return this.toDto(row);
    if (row.status === "running") {
      this.intents.set(id, "cancel");
      await this.abort(row);
      return this.get(id);
    }
    await this.discard(row.targetPath);
    const cancelled = this.settle(id, "cancelled");
    this.pump();
    return cancelled;
  }

  /**
   * Forgets the record. A finished download also loses the artefact it installed — this is the
   * mockup's Remove button on an installed row.
   */
  async remove(id: string): Promise<void> {
    const row = this.require(id);
    if (!TERMINAL.includes(row.status) && row.status !== "paused")
      throw new AppError(AppErrorCode.CONFLICT, "Сначала отмените загрузку");
    await this.discard(row.targetPath);
    if (row.status === "succeeded")
      await rm(row.targetPath, { force: true }).catch(() => undefined);
    this.repository.remove(id);
    this.live.delete(id);
    this.intents.delete(id);
    this.pump();
  }

  /**
   * Runs one download through the sidecar. The kernel node calls this, so a download is a run
   * like any other: cancellation arrives on the step's signal and progress leaves on its stream.
   */
  async execute(downloadId: string, context: DownloadContext = {}): Promise<DownloadReportDto> {
    const row = this.require(downloadId);
    const checksum: ChecksumDto | undefined =
      row.checksumAlgorithm && row.checksumValue
        ? { algorithm: row.checksumAlgorithm, value: row.checksumValue }
        : undefined;
    this.live.set(downloadId, { rate: 0, bytesDone: row.bytesDone, persistedAt: 0 });
    try {
      const result = await this.options.jobs.run(
        DOWNLOAD_JOB,
        {
          url: row.url,
          targetPath: row.targetPath,
          sizeBytes: row.sizeBytes > 0 ? row.sizeBytes : null,
          ...(checksum === undefined
            ? {}
            : { checksum: { algorithm: checksum.algorithm, value: checksum.value } }),
        },
        {
          ...(context.signal === undefined ? {} : { signal: context.signal }),
          onProgress: ({ done, total, rate }) => {
            this.track(downloadId, done, rate ?? 0);
            context.onProgress?.({ done, total, ...(rate === undefined ? {} : { rate }) });
          },
        },
      );
      const report = readReport(result);
      const at = this.now();
      this.repository.update(downloadId, {
        status: "succeeded",
        bytesDone: report.bytes,
        sizeBytes: report.bytes > 0 ? report.bytes : row.sizeBytes,
        error: null,
        finishedAt: at,
        updatedAt: at,
      });
      this.options.logger?.log("info", "downloads", "A download landed", {
        id: downloadId,
        ref: row.itemRef,
        bytes: report.bytes,
        resumedFrom: report.resumedFrom,
        verified: checksum !== undefined,
      });
      return DownloadReportDto.parse({ id: downloadId, ...report });
    } catch (error: unknown) {
      await this.fail(row, error);
      throw error;
    } finally {
      this.live.delete(downloadId);
      this.intents.delete(downloadId);
      this.catalogueService.invalidate();
      this.pump();
    }
  }

  /**
   * After a restart, whatever was mid-flight goes back into the queue. Its partial file is
   * still on disk, so it resumes with a range request rather than starting over.
   */
  recover(): number {
    const at = this.now();
    const interrupted = this.repository.running();
    for (const row of interrupted)
      this.repository.update(row.id, { status: "queued", runId: null, updatedAt: at });
    const resumed = interrupted.length;
    if (resumed > 0)
      this.options.logger?.log("info", "downloads", "Requeued downloads after a restart", {
        count: resumed,
      });
    this.pump();
    return resumed;
  }

  dispose(): void {
    this.closed = true;
    this.live.clear();
    this.intents.clear();
  }

  /** Fills free slots from the queue, highest priority and smallest artefact first. */
  pump(): void {
    if (this.closed || !this.runner) return;
    const free = this.concurrency - this.repository.running().length;
    if (free <= 0) return;
    for (const row of this.repository.nextQueued(free)) this.launch(row);
  }

  private launch(row: DownloadEntity): void {
    const runs = this.runner;
    if (!runs) return;
    const at = this.now();
    // Claim the slot before the run starts so a second pump cannot hand out the same one.
    this.repository.update(row.id, {
      status: "running",
      startedAt: at,
      updatedAt: at,
      error: null,
    });
    let handle: RunHandleDto;
    try {
      handle = runs.start({
        kind: "download",
        subjectId: row.id,
        title: `Загрузка «${row.displayName}»`,
        graph: {
          nodes: [
            {
              id: "fetch",
              type: "download.fetch",
              dependencies: [],
              bindings: {},
              input: { downloadId: row.id },
              retry: { maxAttempts: 1, backoffMs: 0 },
            },
          ],
        },
        input: null,
        concurrency: 1,
      });
    } catch (error: unknown) {
      this.repository.update(row.id, {
        status: "failed",
        error: describe(error),
        finishedAt: this.now(),
        updatedAt: this.now(),
      });
      return;
    }
    this.repository.update(row.id, { runId: handle.id, updatedAt: this.now() });
    // A run that dies before reaching the node would otherwise leave the row claiming a slot.
    runs.subscribe(handle.id, (event) => {
      if (event.type !== "end") return;
      const current = this.repository.findById(row.id);
      if (!current || current.status !== "running") return;
      this.repository.update(row.id, {
        status: "failed",
        error: event.outcome.message ?? "Запуск завершился без результата",
        finishedAt: this.now(),
        updatedAt: this.now(),
      });
      this.pump();
    });
  }

  private async abort(row: DownloadEntity): Promise<void> {
    if (!row.runId || !this.runner) {
      this.settle(row.id, this.intents.get(row.id) === "cancel" ? "cancelled" : "paused");
      return;
    }
    await this.runner.cancel(row.runId);
  }

  private async fail(row: DownloadEntity, error: unknown): Promise<void> {
    const at = this.now();
    const cancelled =
      (error instanceof AppError && error.code === AppErrorCode.RUN_CANCELLED) ||
      (error instanceof Error && error.name === "AbortError");
    if (cancelled) {
      // An abort with no recorded intent is an app shutdown: keep the partial and requeue.
      const intent = this.intents.get(row.id);
      if (intent === "cancel") await this.discard(row.targetPath);
      this.repository.update(row.id, {
        status: intent === "cancel" ? "cancelled" : "paused",
        bytesDone: await this.partial(row.targetPath),
        finishedAt: intent === "cancel" ? at : null,
        updatedAt: at,
      });
      return;
    }
    this.repository.update(row.id, {
      status: "failed",
      error: describe(error).slice(0, 2000),
      bytesDone: await this.partial(row.targetPath),
      finishedAt: at,
      updatedAt: at,
    });
    this.options.logger?.log("warn", "downloads", "A download failed", {
      id: row.id,
      ref: row.itemRef,
      error: describe(error),
    });
  }

  private track(id: string, done: number, rate: number): void {
    const at = this.now();
    const live = this.live.get(id) ?? { rate: 0, bytesDone: done, persistedAt: 0 };
    live.rate = rate;
    live.bytesDone = done;
    // The byte count survives a crash only if it reaches the database, but not on every chunk.
    if (at - live.persistedAt >= (this.options.persistIntervalMs ?? 1_000)) {
      live.persistedAt = at;
      this.repository.update(id, { bytesDone: done, updatedAt: at });
    }
    this.live.set(id, live);
  }

  private settle(id: string, status: DownloadStatus): DownloadDto {
    const at = this.now();
    return this.toDto(
      this.repository.update(id, {
        status,
        updatedAt: at,
        ...(TERMINAL.includes(status) ? { finishedAt: at } : {}),
      }),
    );
  }

  /**
   * Why this item cannot start right now, or `undefined` when it can. Free space is a real
   * precondition, checked before a byte moves and again for every catalogue row.
   */
  private precondition(
    item: CatalogueItem,
    freeBytes: number,
    active: boolean,
    alreadyOnDisk = 0,
  ): string | undefined {
    if (active) return undefined;
    const needed = Math.max(0, item.sizeBytes - alreadyOnDisk) + FREE_SPACE_MARGIN_BYTES;
    if (freeBytes < needed)
      return `Недостаточно места на диске: нужно ${gigabytes(needed)}, свободно ${gigabytes(freeBytes)}`;
    return undefined;
  }

  private async requireWritable(directory: string): Promise<void> {
    try {
      await mkdir(directory, { recursive: true });
      await access(directory, constants.W_OK);
    } catch (cause: unknown) {
      throw new AppError(
        AppErrorCode.PERMISSION_DENIED,
        `Каталог загрузок недоступен для записи: ${directory}`,
        { cause },
      );
    }
  }

  private freeBytes(): Promise<number> {
    return this.options.disk.freeBytes().catch(() => Number.POSITIVE_INFINITY);
  }

  private async partial(targetPath: string): Promise<number> {
    try {
      return (await stat(`${targetPath}${PART_SUFFIX}`)).size;
    } catch {
      return 0;
    }
  }

  private discard(targetPath: string): Promise<void> {
    return rm(`${targetPath}${PART_SUFFIX}`, { force: true }).catch(() => undefined);
  }

  private require(id: string): DownloadEntity {
    const row = this.repository.findById(id);
    if (!row) throw new AppError(AppErrorCode.NOT_FOUND, "Загрузка не найдена");
    return row;
  }

  private toDto(row: DownloadEntity): DownloadDto {
    const live = this.live.get(row.id);
    const bytesDone = live?.bytesDone ?? row.bytesDone;
    const rate = row.status === "running" ? (live?.rate ?? 0) : 0;
    const remaining = Math.max(0, row.sizeBytes - bytesDone);
    return DownloadDto.parse({
      id: row.id,
      itemKind: row.itemKind,
      itemRef: row.itemRef,
      displayName: row.displayName,
      sizeBytes: row.sizeBytes,
      bytesDone,
      status: row.status,
      priority: row.priority,
      targetPath: row.targetPath,
      url: row.url,
      rateBytesPerSecond: rate,
      createdAt: row.createdAt,
      updatedAt: row.updatedAt,
      ...(row.checksumAlgorithm && row.checksumValue
        ? { checksum: { algorithm: row.checksumAlgorithm, value: row.checksumValue } }
        : {}),
      ...(row.version === null ? {} : { version: row.version }),
      ...(row.error === null ? {} : { error: row.error }),
      ...(row.runId === null ? {} : { runId: row.runId }),
      ...(rate > 0 && remaining > 0 ? { etaMs: Math.round((remaining / rate) * 1000) } : {}),
      ...(row.startedAt === null ? {} : { startedAt: row.startedAt }),
      ...(row.finishedAt === null ? {} : { finishedAt: row.finishedAt }),
    });
  }

  private get repository() {
    return this.options.data.repositories.downloads;
  }

  private now(): Timestamp {
    return (this.options.clock ?? Date.now)() as Timestamp;
  }
}

const TERMINAL: readonly DownloadStatus[] = ["succeeded", "failed", "cancelled"];

function resolveState(
  item: CatalogueItem,
  active: DownloadStatus | undefined,
  present: { installed: boolean; installedVersion?: string; digest?: string },
): CatalogueItemState {
  if (active === "running" || active === "paused") return "downloading";
  if (active === "queued") return "queued";
  if (!present.installed) return "available";
  // An installed item whose digest or version has moved on is the mockup's amber `update`.
  if (item.digest !== undefined && present.digest !== undefined)
    return item.digest.toLowerCase() === present.digest.toLowerCase() ? "installed" : "update";
  if (item.version !== undefined && present.installedVersion !== undefined)
    return item.version === present.installedVersion ? "installed" : "update";
  return "installed";
}

function readReport(result: Json): Omit<DownloadReportDto, "id"> {
  if (result === null || typeof result !== "object" || Array.isArray(result))
    throw new AppError(AppErrorCode.NATIVE_ERROR, "Загрузка вернула неожиданный результат");
  const record = result as Record<string, Json>;
  const bytes = typeof record.bytes === "number" ? record.bytes : 0;
  const resumedFrom = typeof record.resumedFrom === "number" ? record.resumedFrom : 0;
  const path = typeof record.path === "string" ? record.path : "";
  if (!path) throw new AppError(AppErrorCode.NATIVE_ERROR, "Загрузка не сообщила путь");
  return {
    bytes,
    resumedFrom,
    path,
    ...(typeof record.checksum === "string" ? { checksum: record.checksum } : {}),
  };
}

function gigabytes(bytes: number): string {
  if (!Number.isFinite(bytes)) return "∞";
  return `${(bytes / 1024 ** 3).toFixed(1)} ГБ`;
}

function describe(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
