import { AppError, AppErrorCode, type ChecksumDto } from "@zvs/shared";
import type { Logger } from "../platform/logger.ts";
import type { RuntimeDefinition } from "./definitions.ts";

export const LLAMA_RELEASES_URL =
  "https://api.github.com/repos/ggml-org/llama.cpp/releases?per_page=8";

export const LLAMA_RELEASES_PAGE = "https://github.com/ggml-org/llama.cpp/releases";

export interface ResolvedAsset {
  readonly name: string;
  readonly url: string;
  readonly sizeBytes: number;
  readonly checksum?: ChecksumDto;
}

export interface ResolvedBuild {
  readonly runtimeId: string;
  readonly tag: string;
  readonly assets: readonly ResolvedAsset[];
}

interface ReleaseAsset {
  name?: unknown;
  size?: unknown;
  browser_download_url?: unknown;
  digest?: unknown;
  state?: unknown;
}

interface Release {
  tag_name?: unknown;
  draft?: unknown;
  prerelease?: unknown;
  assets?: unknown;
}

export type FetchLike = (url: string, init?: RequestInit) => Promise<Response>;

export interface ReleaseResolverOptions {
  fetch?: FetchLike;
  logger?: Logger;
  platform?: NodeJS.Platform;
  arch?: string;
  /** How long a resolved release stays usable. GitHub allows 60 anonymous calls an hour. */
  ttlMs?: number;
  clock?: () => number;
  url?: string;
}

const DEFAULT_TTL_MS = 6 * 60 * 60 * 1000;

/**
 * Resolves engine builds against llama.cpp's own releases rather than pinning a tag in the
 * source. Upstream publishes a build several times a day, so a hard-coded URL would rot within
 * a week; the asset *naming* is what is stable, and that is what `RuntimeDefinition` encodes.
 *
 * The call leaves the machine, so it happens only when the user asks — opening the Downloads
 * page answers from the cache, or from nothing at all.
 */
export class ReleaseResolver {
  private cached: readonly Release[] | undefined;
  private cachedAt = 0;
  private pending: Promise<readonly Release[]> | undefined;

  constructor(private readonly options: ReleaseResolverOptions = {}) {}

  get cachedReleases(): boolean {
    return this.cached !== undefined;
  }

  async resolve(
    definition: RuntimeDefinition,
    refresh = false,
  ): Promise<ResolvedBuild | undefined> {
    const plan = definition.assets(
      this.options.platform ?? process.platform,
      this.options.arch ?? process.arch,
    );
    if (plan === undefined) return undefined;

    for (const release of await this.releases(refresh)) {
      const tag = typeof release.tag_name === "string" ? release.tag_name : "";
      if (!/^b\d+$/.test(tag)) continue;
      if (release.draft === true) continue;
      const assets = readAssets(release);
      const primary = assets.find((asset) => plan.primary.test(asset.name));
      if (primary === undefined) continue;

      const collected = [primary];
      if (plan.companion !== undefined) {
        const variant = plan.primary.exec(primary.name)?.[1] ?? "";
        const pattern = plan.companion(variant);
        const companion = assets.find((asset) => pattern.test(asset.name));
        // A CUDA build without its runtime libraries starts and then fails to load the device.
        // Refusing here beats installing something that cannot work.
        if (companion === undefined) continue;
        collected.push(companion);
      }
      return { runtimeId: definition.id, tag, assets: collected };
    }
    return undefined;
  }

  private async releases(refresh: boolean): Promise<readonly Release[]> {
    const ttl = this.options.ttlMs ?? DEFAULT_TTL_MS;
    const fresh = this.cached !== undefined && this.now() - this.cachedAt < ttl;
    if (!refresh && fresh) return this.cached!;
    this.pending ??= this.load().finally(() => {
      this.pending = undefined;
    });
    try {
      return await this.pending;
    } catch (error: unknown) {
      // A refresh that cannot reach GitHub must not erase what we already knew.
      if (this.cached !== undefined) {
        this.options.logger?.log("warn", "runtimes", "Kept the cached release list", {
          error: String(error),
        });
        return this.cached;
      }
      throw error;
    }
  }

  private async load(): Promise<readonly Release[]> {
    const request = this.options.fetch ?? globalThis.fetch;
    const response = await request(this.options.url ?? LLAMA_RELEASES_URL, {
      headers: { accept: "application/vnd.github+json" },
    });
    if (!response.ok)
      throw new AppError(
        response.status === 403 ? AppErrorCode.RATE_LIMITED : AppErrorCode.PROVIDER_UNREACHABLE,
        response.status === 403
          ? "GitHub временно ограничил число запросов. Попробуйте через несколько минут."
          : `Не удалось получить список сборок движка (${String(response.status)})`,
      );
    const payload: unknown = await response.json();
    if (!Array.isArray(payload))
      throw new AppError(AppErrorCode.VALIDATION_FAILED, "GitHub вернул неожиданный ответ");
    this.cached = payload as readonly Release[];
    this.cachedAt = this.now();
    this.options.logger?.log("info", "runtimes", "Read the llama.cpp release list", {
      releases: payload.length,
    });
    return this.cached;
  }

  private now(): number {
    return (this.options.clock ?? Date.now)();
  }
}

function readAssets(release: Release): readonly ResolvedAsset[] {
  if (!Array.isArray(release.assets)) return [];
  return release.assets.flatMap((raw: ReleaseAsset) => {
    if (typeof raw.name !== "string" || typeof raw.browser_download_url !== "string") return [];
    if (raw.state !== undefined && raw.state !== "uploaded") return [];
    const digest =
      typeof raw.digest === "string" ? /^sha256:([0-9a-f]{64})$/.exec(raw.digest) : null;
    return [
      {
        name: raw.name,
        url: raw.browser_download_url,
        sizeBytes: typeof raw.size === "number" ? raw.size : 0,
        ...(digest === null
          ? {}
          : { checksum: { algorithm: "sha256", value: digest[1]! } satisfies ChecksumDto }),
      },
    ];
  });
}
