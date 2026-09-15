import { readdir, rm, stat } from "node:fs/promises";
import { basename, dirname, join } from "node:path";
import {
  AppError,
  AppErrorCode,
  InstallRuntimeInput,
  RuntimeOverviewDto,
  type Accelerator,
  type DownloadDto,
  type ModelFormat,
  type RuntimeDto,
  type RuntimeState,
  type VectorOcrLanguage,
} from "@zvs/shared";
import type { DownloadEntity } from "../data/schema/index.ts";
import type { CatalogueItem, CatalogueService } from "../downloads/catalogue.ts";
import type { DownloadService } from "../downloads/DownloadService.ts";
import { extractArchive } from "../platform/archive.ts";
import type { DeviceProbe } from "../platform/device.ts";
import type { Logger } from "../platform/logger.ts";
import { DOWNLOAD_DIRECTORIES, runtimeInstallPath } from "../platform/paths.ts";
import type { SettingService } from "../services/SettingService.ts";
import {
  definitionOf,
  formatOf,
  planAccelerator,
  RUNTIME_DEFINITIONS,
  type RuntimeDefinition,
} from "./definitions.ts";
import { LlamaServer, type LlamaServerOptions, type LlamaServerRole } from "./LlamaServer.ts";
import { ReleaseResolver, type ResolvedBuild } from "./releases.ts";

export const INSTALLED_KEY = "runtimes.installed";
/** Catalogue refs of the embedding models that pair with a local engine, best first. */
export const RECOMMENDED_MODEL_REFS: readonly string[] = ["curated:embedding:bge-m3-f16"];

interface InstalledRuntime {
  tag: string;
  installPath: string;
  executablePath: string;
  sizeBytes: number;
}

type InstalledMap = Record<string, InstalledRuntime>;

export interface RuntimeServiceOptions {
  /** Narrowed to what is actually used, so a test can stand these in without a database. */
  downloads: Pick<DownloadService, "start" | "list">;
  catalogue: Pick<CatalogueService, "offer" | "find">;
  settings: Pick<SettingService, "get" | "set">;
  downloadsDir: string;
  device?: Pick<DeviceProbe, "profile">;
  resolver?: ReleaseResolver;
  logger?: Logger;
  platform?: NodeJS.Platform;
  arch?: string;
  /** Overridden in tests so no child process is ever spawned. */
  createServer?: (options: LlamaServerOptions) => LlamaServer;
  contextSize?: number;
}

const SERVER_NAMES = new Set(["llama-server", "llama-server.exe"]);

/**
 * Owns the local inference engines: which builds exist for this machine, which one is
 * installed, and the process that answers an embedding call.
 *
 * The engine is deliberately *not* bundled. llama.cpp publishes a build several times a day
 * across a dozen backends; shipping one would pin us to a stale binary and add hundreds of
 * megabytes to the installer for a feature most users of API providers never touch. Instead the
 * Downloads page offers the build this machine should have, and nothing leaves the network
 * until the user asks for it.
 */
export class RuntimeService {
  readonly #servers = new Map<string, LlamaServer>();
  readonly #resolver: ReleaseResolver;
  #installing = new Set<string>();

  constructor(private readonly options: RuntimeServiceOptions) {
    this.#resolver =
      options.resolver ??
      new ReleaseResolver({
        ...(options.logger === undefined ? {} : { logger: options.logger }),
        ...(options.platform === undefined ? {} : { platform: options.platform }),
        ...(options.arch === undefined ? {} : { arch: options.arch }),
      });
  }

  private get platform(): NodeJS.Platform {
    return this.options.platform ?? process.platform;
  }
  private get arch(): string {
    return this.options.arch ?? process.arch;
  }

  /** Everything the Downloads page needs to present the local stack in one call. */
  async overview(): Promise<RuntimeOverviewDto> {
    const device = (await this.options.device?.profile().catch(() => null)) ?? null;
    const plan = planAccelerator(device, this.platform, this.arch);
    const installed = this.installed();
    const active = this.activeDownloads();

    const runtimes = RUNTIME_DEFINITIONS.map((definition) =>
      this.present(definition, plan.accelerator, installed, active),
    );
    return RuntimeOverviewDto.parse({
      plan: {
        accelerator: plan.accelerator,
        reason: plan.reason,
        gpu: device?.gpu ?? null,
        ready: Object.keys(installed).length > 0,
      },
      runtimes,
      recommendedModels: [...RECOMMENDED_MODEL_REFS],
    });
  }

  /**
   * Resolves the build against today's llama.cpp release and queues its archives. This is the
   * only path that reaches GitHub, and only ever because the user pressed a button.
   */
  async install(raw: InstallRuntimeInput = {}): Promise<DownloadDto[]> {
    const input = InstallRuntimeInput.parse(raw);
    const definition = await this.definitionFor(input.id);
    if (this.#installing.has(definition.id))
      throw new AppError(AppErrorCode.CONFLICT, "Этот движок уже устанавливается");

    const build = await this.#resolver.resolve(definition, true);
    if (build === undefined)
      throw new AppError(
        AppErrorCode.UNSUPPORTED_FORMAT,
        `Для этой системы нет сборки «${definition.displayName}»`,
        { details: { runtimeId: definition.id, platform: this.platform, arch: this.arch } },
      );

    this.#installing.add(definition.id);
    try {
      const started: DownloadDto[] = [];
      for (const item of this.catalogueItems(definition, build)) {
        this.options.catalogue.offer(item);
        started.push(await this.options.downloads.start({ ref: item.ref }));
      }
      this.options.logger?.log("info", "runtimes", "Queued an engine install", {
        runtimeId: definition.id,
        tag: build.tag,
        archives: started.length,
      });
      return started;
    } finally {
      this.#installing.delete(definition.id);
    }
  }

  /**
   * Unpacks an engine archive once it has landed. Registered as the download service's
   * installer, so it runs for every kind and returns immediately for the ones that are already
   * usable as a bare file.
   */
  readonly unpackDownload = async (row: DownloadEntity): Promise<void> => {
    if (row.itemKind !== "runtime") return;
    const parsed = parseRuntimeRef(row.itemRef);
    if (parsed === undefined) return;
    const definition = definitionOf(parsed.runtimeId);
    if (definition === undefined) return;

    const target = runtimeInstallPath(this.options.downloadsDir, definition.id);
    const report = await extractArchive(row.targetPath, target);
    // The archive is several hundred megabytes of duplicate; the unpacked tree is what runs.
    await rm(row.targetPath, { force: true }).catch(() => undefined);

    const executable = await findExecutable(target);
    const previous = this.installed()[definition.id];
    const record: InstalledRuntime = {
      tag: parsed.tag,
      installPath: target,
      executablePath: executable ?? previous?.executablePath ?? "",
      sizeBytes: (previous?.tag === parsed.tag ? (previous.sizeBytes ?? 0) : 0) + report.bytes,
    };
    this.remember(definition.id, record);
    // A companion archive lands separately and may be the one carrying the executable, so an
    // install with no server binary yet is not an error until the last archive is in.
    this.options.logger?.log("info", "runtimes", "Unpacked an engine archive", {
      runtimeId: definition.id,
      tag: parsed.tag,
      files: report.files,
      executable: record.executablePath === "" ? null : record.executablePath,
    });
    this.stop(definition.id);
  };

  /**
   * Embeds through whichever installed engine can load this model's format. The model name is
   * the weight file as `FileSystemModelStore` reported it.
   */
  async embed(
    modelName: string,
    texts: readonly string[],
    signal: AbortSignal,
  ): Promise<Float32Array[]> {
    const format = formatOf(modelName);
    if (format === undefined)
      throw new AppError(
        AppErrorCode.UNSUPPORTED_FORMAT,
        `Формат файла «${basename(modelName)}» не поддерживается локальным движком`,
      );
    const { definition, record } = this.engineFor(format);
    const modelPath = await this.modelPath(modelName);
    const server = this.serverFor(definition, record, { modelPath, role: "embedding" });
    return server.embed(texts, signal);
  }

  /**
   * Reads an image with the vision model named by a catalogue ref — the store's `ocr.modelRef`.
   *
   * OCR is a vision model rather than a classical engine because the app already needs one:
   * Qwen2.5-VL is in the catalogue, it reads Russian and handwriting, and it runs on the engine
   * that is already installed. Nothing new has to ship to make a scanned page searchable.
   */
  async readImage(
    modelRef: string,
    image: { readonly mediaType: string; readonly bytes: Uint8Array },
    language: VectorOcrLanguage,
    signal: AbortSignal,
  ): Promise<string> {
    const item = await this.options.catalogue.find(modelRef);
    const { definition, record } = this.engineFor("gguf");
    const modelPath = await this.modelPath(item.fileName);
    const mmprojPath = await this.projectorFor(modelPath);
    const server = this.serverFor(definition, record, { modelPath, role: "vision", mmprojPath });
    const text = await server.describeImage(image, ocrPrompt(language), signal);
    return text.trim();
  }

  /**
   * A vision model is two files: the weights and the projector that turns pixels into tokens.
   * They download together and sit side by side, so the projector is found by name rather than
   * configured — one less thing for the user to get wrong.
   */
  private async projectorFor(modelPath: string): Promise<string> {
    const directory = dirname(modelPath);
    const entries = await readdir(directory).catch(() => [] as string[]);
    const projectors = entries.filter((name) => PROJECTOR.test(name));
    if (projectors.length === 0)
      throw new AppError(
        AppErrorCode.NOT_FOUND,
        "Рядом с моделью нет файла mmproj: скачайте проектор в «Загрузках», без него распознавание не запустится",
        { details: { directory } },
      );
    // With several projectors present, the one sharing the most of the model's name wins.
    const stem = basename(modelPath).toLowerCase().replace(GGUF_SUFFIX, "");
    const best = projectors.sort((left, right) => overlap(right, stem) - overlap(left, stem))[0]!;
    return join(directory, best);
  }

  /** Whether anything installed can load this format — used to explain a refusal up front. */
  supports(format: ModelFormat): boolean {
    try {
      this.engineFor(format);
      return true;
    } catch {
      return false;
    }
  }

  /** Stops every server of a runtime, whichever role it was serving. */
  stop(runtimeId: string): boolean {
    let stopped = false;
    for (const [key, server] of [...this.#servers]) {
      if (!key.startsWith(`${runtimeId}:`)) continue;
      stopped ||= server.running;
      server.stop();
      this.#servers.delete(key);
    }
    return stopped;
  }

  dispose(): void {
    for (const server of this.#servers.values()) server.stop();
    this.#servers.clear();
  }

  private engineFor(format: ModelFormat): {
    definition: RuntimeDefinition;
    record: InstalledRuntime;
  } {
    const installed = this.installed();
    // Whatever is installed wins; between two, the accelerated build is preferred.
    const candidates = RUNTIME_DEFINITIONS.filter(
      (definition) =>
        definition.formats.includes(format) &&
        (installed[definition.id]?.executablePath ?? "") !== "",
    ).sort((left, right) => rank(right.accelerator) - rank(left.accelerator));
    const definition = candidates[0];
    if (definition === undefined)
      throw new AppError(
        AppErrorCode.CONFLICT,
        format === "onnx"
          ? "Для файлов .onnx движок ещё не подключён: используйте модель в формате GGUF"
          : "Локальный движок не установлен. Откройте «Загрузки» и установите llama.cpp.",
        { details: { format } },
      );
    return { definition, record: installed[definition.id]! };
  }

  /**
   * One server per runtime and role. Embedding and vision cannot share a process — `--embeddings`
   * puts llama.cpp in a pooling mode that cannot generate — but indexing a folder of scanned PDFs
   * needs both at once, so the two run side by side rather than replacing each other.
   */
  private serverFor(
    definition: RuntimeDefinition,
    record: InstalledRuntime,
    want: { modelPath: string; role: LlamaServerRole; mmprojPath?: string },
  ): LlamaServer {
    const key = `${definition.id}:${want.role}`;
    const existing = this.#servers.get(key);
    if (existing !== undefined && existing.modelPath === want.modelPath) return existing;
    existing?.stop();
    const options: LlamaServerOptions = {
      executablePath: record.executablePath,
      modelPath: want.modelPath,
      role: want.role,
      ...(want.mmprojPath === undefined ? {} : { mmprojPath: want.mmprojPath }),
      gpuLayers: definition.accelerator === "cpu" ? 0 : 999,
      ...(this.options.contextSize === undefined ? {} : { contextSize: this.options.contextSize }),
      ...(this.options.logger === undefined ? {} : { logger: this.options.logger }),
    };
    const server = (this.options.createServer ?? ((given) => new LlamaServer(given)))(options);
    this.#servers.set(key, server);
    return server;
  }

  /** Weight files land under `embeddings/` or `models/`; either is a valid place to look. */
  private async modelPath(modelName: string): Promise<string> {
    const safe = basename(modelName);
    for (const folder of [DOWNLOAD_DIRECTORIES.embedding, DOWNLOAD_DIRECTORIES.model]) {
      const candidate = join(this.options.downloadsDir, folder, safe);
      const found = await stat(candidate).catch(() => undefined);
      if (found?.isFile() === true) return candidate;
    }
    throw new AppError(AppErrorCode.NOT_FOUND, `Файл модели «${safe}» не найден на диске`, {
      details: { modelName },
    });
  }

  private async definitionFor(id: string | undefined): Promise<RuntimeDefinition> {
    if (id !== undefined) {
      const named = definitionOf(id);
      if (named === undefined)
        throw new AppError(AppErrorCode.NOT_FOUND, "Такого движка нет", { details: { id } });
      return named;
    }
    const device = (await this.options.device?.profile().catch(() => null)) ?? null;
    const plan = planAccelerator(device, this.platform, this.arch);
    const chosen = RUNTIME_DEFINITIONS.find(
      (definition) =>
        definition.accelerator === plan.accelerator &&
        definition.assets(this.platform, this.arch) !== undefined,
    );
    // The recommendation is a preference, not a promise: fall back to the build that always
    // exists rather than refusing to install anything.
    return (
      chosen ??
      RUNTIME_DEFINITIONS.find(
        (definition) => definition.assets(this.platform, this.arch) !== undefined,
      ) ??
      RUNTIME_DEFINITIONS[0]!
    );
  }

  private catalogueItems(
    definition: RuntimeDefinition,
    build: ResolvedBuild,
  ): readonly CatalogueItem[] {
    return build.assets.map((asset) => ({
      ref: runtimeRef(definition.id, build.tag, asset.name),
      kind: "runtime" as const,
      source: "github" as const,
      name: `${definition.id}/${asset.name}`,
      displayName: `${definition.displayName} · ${build.tag}`,
      description: definition.description,
      version: build.tag,
      sizeBytes: asset.sizeBytes,
      url: asset.url,
      fileName: asset.name,
      ...(asset.checksum === undefined ? {} : { checksum: asset.checksum }),
      digest: build.tag,
      tags: ["llama.cpp", definition.accelerator, build.tag],
    }));
  }

  private present(
    definition: RuntimeDefinition,
    recommended: Accelerator,
    installed: InstalledMap,
    active: ReadonlySet<string>,
  ): RuntimeDto {
    const supported = definition.assets(this.platform, this.arch) !== undefined;
    const record = installed[definition.id];
    const server = this.#servers.get(`${definition.id}:embedding`);
    const refs = [...active].filter((ref) => parseRuntimeRef(ref)?.runtimeId === definition.id);

    const state: RuntimeState = !supported
      ? "unsupported"
      : refs.length > 0
        ? "installing"
        : record === undefined || record.executablePath === ""
          ? "available"
          : server?.running === true
            ? "ready"
            : "installed";

    return {
      id: definition.id,
      engine: definition.engine,
      accelerator: definition.accelerator,
      displayName: definition.displayName,
      description: definition.description,
      formats: [...definition.formats],
      state,
      sizeBytes: record?.sizeBytes ?? definition.approximateBytes,
      recommended: supported && definition.accelerator === recommended,
      refs,
      ...(record === undefined
        ? {}
        : {
            installedVersion: record.tag,
            installPath: record.installPath,
            ...(record.executablePath === "" ? {} : { executablePath: record.executablePath }),
          }),
      ...(supported
        ? {}
        : { blockedReason: "Для этой операционной системы такой сборки не существует" }),
    };
  }

  private activeDownloads(): ReadonlySet<string> {
    return new Set(
      this.options.downloads
        .list({ kinds: ["runtime"] })
        .filter((row) => row.status === "queued" || row.status === "running")
        .map((row) => row.itemRef),
    );
  }

  private installed(): InstalledMap {
    const stored: unknown = this.options.settings.get(INSTALLED_KEY)?.value;
    if (stored === null || typeof stored !== "object" || Array.isArray(stored)) return {};
    const installed: InstalledMap = {};
    for (const [id, raw] of Object.entries(stored as Record<string, unknown>)) {
      if (raw === null || typeof raw !== "object") continue;
      const row = raw as Partial<InstalledRuntime>;
      if (typeof row.tag !== "string" || typeof row.installPath !== "string") continue;
      installed[id] = {
        tag: row.tag,
        installPath: row.installPath,
        executablePath: typeof row.executablePath === "string" ? row.executablePath : "",
        sizeBytes: typeof row.sizeBytes === "number" ? row.sizeBytes : 0,
      };
    }
    return installed;
  }

  private remember(runtimeId: string, record: InstalledRuntime): void {
    this.options.settings.set(INSTALLED_KEY, { ...this.installed(), [runtimeId]: record });
  }
}

/** `runtime:<runtimeId>:<tag>:<assetName>` — the tag makes an upgrade a different row. */
export function runtimeRef(runtimeId: string, tag: string, assetName: string): string {
  return `runtime:${runtimeId}:${tag}:${assetName}`;
}

export function parseRuntimeRef(
  ref: string,
): { runtimeId: string; tag: string; assetName: string } | undefined {
  const match = /^runtime:([a-z0-9-]+):(b\d+):(.+)$/.exec(ref);
  if (match === null) return undefined;
  return { runtimeId: match[1]!, tag: match[2]!, assetName: match[3]! };
}

const PROJECTOR = /^mmproj.*\.gguf$/i;
const GGUF_SUFFIX = /\.gguf$/;

const OCR_LANGUAGE_HINTS: Record<VectorOcrLanguage, string> = {
  auto: "",
  rus: " Текст на русском языке.",
  eng: " The text is in English.",
  "rus+eng": " В тексте есть и русский, и английский.",
};

/** Transcription, not description: the answer gets indexed, so anything added to it is noise. */
export function ocrPrompt(language: VectorOcrLanguage): string {
  return (
    "Прочитай весь текст на изображении и выпиши его целиком, сохраняя порядок строк, " +
    "абзацы и структуру таблиц. Не переводи, не пересказывай и не добавляй комментариев. " +
    "Если текста нет, ответь пустой строкой." +
    OCR_LANGUAGE_HINTS[language]
  );
}

/** How much of the model's name a projector file repeats — the pairing heuristic. */
function overlap(candidate: string, stem: string): number {
  const name = candidate.toLowerCase();
  let score = 0;
  for (const part of stem.split(/[-_.]/)) if (part.length > 2 && name.includes(part)) score += 1;
  return score;
}

function rank(accelerator: Accelerator): number {
  return accelerator === "cpu" ? 0 : 1;
}

/** Walks the unpacked tree for the server binary; upstream nests it under `build/bin` on unix. */
async function findExecutable(root: string, depth = 0): Promise<string | undefined> {
  if (depth > 4) return undefined;
  let entries;
  try {
    entries = await readdir(root, { withFileTypes: true });
  } catch {
    return undefined;
  }
  for (const entry of entries) {
    if (entry.isFile() && SERVER_NAMES.has(entry.name.toLowerCase())) return join(root, entry.name);
  }
  for (const entry of entries) {
    if (!entry.isDirectory()) continue;
    const found = await findExecutable(join(root, entry.name), depth + 1);
    if (found !== undefined) return found;
  }
  return undefined;
}
