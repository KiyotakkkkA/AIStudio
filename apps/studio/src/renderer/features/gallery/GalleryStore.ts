import { makeAutoObservable, runInAction } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import type {
  Contract,
  DashboardStatusDto,
  GalleryItemDto,
  GalleryKind,
  GalleryThumbnailDto,
} from "@zvs/shared";
import ArtifactActions from "../chat/ArtifactActions";
import {
  GALLERY_KINDS,
  GALLERY_LOAD_FAILED,
  GALLERY_OUTSIDE_DIALOG,
  galleryErrorCopy,
  type GallerySort,
} from "./galleryCopy";
import { decodeBase64 } from "./base64";

export const DASHBOARDS_OWNER = "dashboards";

export interface GallerySection {
  readonly kind: GalleryKind;
  readonly label: string;
  readonly items: readonly GalleryItemDto[];
}

export interface GalleryThumbnail {
  readonly state: "loading" | "ready" | "none";
  readonly url: string | null;
}

export interface ObjectUrls {
  create(blob: Blob): string;
  revoke(url: string): void;
}

export interface GalleryDependencies {
  readonly renderPdf: (bytes: Uint8Array) => Promise<Blob | null>;
  readonly urls: ObjectUrls;
  readonly now: () => number;
}

const browserUrls: ObjectUrls = {
  create: (blob) => URL.createObjectURL(blob),
  revoke: (url) => URL.revokeObjectURL(url),
};

const defaults: GalleryDependencies = {
  renderPdf: async () => null,
  urls: browserUrls,
  now: () => Date.now(),
};

function thumbnailKey(item: GalleryItemDto): string {
  return `${item.id}@${item.modifiedAt}`;
}

export default class GalleryStore {
  items: GalleryItemDto[] = [];
  titles = new Map<string, string>();
  loaded = false;
  loading = false;
  error: string | null = null;
  query = "";
  sort: GallerySort = "newest";
  thumbnails = new Map<string, GalleryThumbnail>();
  statuses = new Map<string, DashboardStatusDto>();
  pending = new Set<string>();
  failures = new Map<string, string>();
  readonly actions: ArtifactActions;
  private dependencies: GalleryDependencies;

  constructor(
    private readonly ipc: IpcClient<Contract>,
    dependencies: Partial<GalleryDependencies> = {},
  ) {
    this.dependencies = { ...defaults, ...dependencies };
    this.actions = new ArtifactActions(ipc);
    makeAutoObservable<GalleryStore, "ipc" | "dependencies">(
      this,
      { ipc: false, dependencies: false, actions: false },
      { autoBind: true },
    );
  }

  get now(): number {
    return this.dependencies.now();
  }

  async load(): Promise<void> {
    if (this.loading) return;
    this.loading = true;
    try {
      const [list, conversations] = await Promise.all([
        this.ipc.call("gallery.list", undefined),
        this.ipc.call("chat.conversations.list", undefined).catch(() => []),
      ]);
      runInAction(() => {
        this.items = list.items;
        this.titles = new Map(
          conversations.map((conversation) => [conversation.id, conversation.title]),
        );
        this.error = null;
        this.loaded = true;
        this.forgetStale();
      });
      await Promise.all(
        list.items.filter((item) => item.live).map((item) => this.loadStatus(item.id)),
      );
    } catch {
      runInAction(() => {
        this.error = GALLERY_LOAD_FAILED;
        this.loaded = true;
      });
    } finally {
      runInAction(() => {
        this.loading = false;
      });
    }
  }

  usePdfRenderer(renderPdf: GalleryDependencies["renderPdf"]): void {
    this.dependencies = { ...this.dependencies, renderPdf };
  }

  setQuery(query: string): void {
    this.query = query;
  }

  setSort(sort: GallerySort): void {
    this.sort = sort;
  }

  ownerLabel(item: GalleryItemDto): string | null {
    if (item.owner === DASHBOARDS_OWNER) return null;
    return this.titles.get(item.owner) ?? GALLERY_OUTSIDE_DIALOG;
  }

  get visible(): GalleryItemDto[] {
    const needle = this.query.trim().toLocaleLowerCase("ru-RU");
    const found = needle
      ? this.items.filter((item) =>
          [item.name, this.ownerLabel(item) ?? ""].some((text) =>
            text.toLocaleLowerCase("ru-RU").includes(needle),
          ),
        )
      : [...this.items];
    const byName = (a: GalleryItemDto, b: GalleryItemDto) => a.name.localeCompare(b.name, "ru-RU");
    if (this.sort === "name") return found.sort(byName);
    const sign = this.sort === "newest" ? -1 : 1;
    return found.sort((a, b) => sign * (a.modifiedAt - b.modifiedAt) || byName(a, b));
  }

  get sections(): GallerySection[] {
    const visible = this.visible;
    return GALLERY_KINDS.map(({ kind, label }) => ({
      kind,
      label,
      items: visible.filter((item) => item.kind === kind),
    })).filter((section) => section.items.length > 0);
  }

  get isEmpty(): boolean {
    return this.loaded && this.error === null && this.items.length === 0;
  }

  thumbnailOf(item: GalleryItemDto): GalleryThumbnail | undefined {
    return this.thumbnails.get(thumbnailKey(item));
  }

  async loadThumbnail(item: GalleryItemDto): Promise<void> {
    const key = thumbnailKey(item);
    if (this.thumbnails.has(key)) return;
    this.thumbnails.set(key, { state: "loading", url: null });
    let url: string | null = null;
    try {
      const preview = await this.ipc.call("gallery.thumbnail", { id: item.id });
      url = await this.imageOf(preview);
    } catch {
      url = null;
    }
    runInAction(() => {
      if (!this.thumbnails.has(key)) {
        if (url !== null) this.dependencies.urls.revoke(url);
        return;
      }
      this.thumbnails.set(
        key,
        url === null ? { state: "none", url: null } : { state: "ready", url },
      );
    });
  }

  private async imageOf(preview: GalleryThumbnailDto): Promise<string | null> {
    if (!("mediaType" in preview)) return null;
    const bytes = decodeBase64(preview.data);
    if (preview.mediaType === "application/pdf") {
      const rendered = await this.dependencies.renderPdf(bytes);
      return rendered === null ? null : this.dependencies.urls.create(rendered);
    }
    return this.dependencies.urls.create(new Blob([bytes], { type: preview.mediaType }));
  }

  private forgetStale(): void {
    const current = new Set(this.items.map(thumbnailKey));
    for (const [key, thumbnail] of this.thumbnails) {
      if (current.has(key)) continue;
      if (thumbnail.url !== null) this.dependencies.urls.revoke(thumbnail.url);
      this.thumbnails.delete(key);
    }
    const ids = new Set(this.items.map((item) => item.id));
    for (const id of this.statuses.keys()) if (!ids.has(id)) this.statuses.delete(id);
  }

  statusOf(id: string): DashboardStatusDto | undefined {
    return this.statuses.get(id);
  }

  needsApproval(id: string): boolean {
    return this.statuses.get(id)?.scripts.some((script) => !script.approved) ?? false;
  }

  async loadStatus(id: string): Promise<void> {
    try {
      const status = await this.ipc.call("dashboards.status", { id });
      runInAction(() => {
        this.statuses.set(id, status);
      });
    } catch {
      runInAction(() => {
        this.statuses.delete(id);
      });
    }
  }

  private async act(id: string, call: () => Promise<void>): Promise<boolean> {
    if (this.pending.has(id)) return false;
    this.pending.add(id);
    this.failures.delete(id);
    try {
      await call();
      return true;
    } catch (error) {
      runInAction(() => {
        this.failures.set(id, galleryErrorCopy(error));
      });
      return false;
    } finally {
      runInAction(() => {
        this.pending.delete(id);
      });
    }
  }

  refresh(id: string): Promise<boolean> {
    return this.act(id, async () => {
      try {
        const status = await this.ipc.call("dashboards.refresh", { id });
        runInAction(() => {
          this.statuses.set(id, status);
        });
      } finally {
        await this.loadStatus(id);
      }
      await this.load();
    });
  }

  setInterval(id: string, interval: number | null): Promise<boolean> {
    return this.act(id, async () => {
      const status = await this.ipc.call("dashboards.setInterval", {
        id,
        ...(interval === null ? {} : { interval }),
      });
      runInAction(() => {
        this.statuses.set(id, status);
      });
    });
  }

  approve(id: string): Promise<boolean> {
    const hashes = this.statuses.get(id)?.scripts.map((script) => script.sha256) ?? [];
    if (hashes.length === 0) return Promise.resolve(false);
    return this.act(id, async () => {
      try {
        const status = await this.ipc.call("dashboards.approve", { id, hashes });
        runInAction(() => {
          this.statuses.set(id, status);
        });
      } catch (error) {
        await this.loadStatus(id);
        throw error;
      }
    });
  }

  remove(id: string): Promise<boolean> {
    return this.act(id, async () => {
      await this.ipc.call("gallery.remove", { id });
      runInAction(() => {
        this.items = this.items.filter((item) => item.id !== id);
        this.forgetStale();
      });
    });
  }

  dismissFailure(id: string): void {
    this.failures.delete(id);
  }

  dispose(): void {
    for (const thumbnail of this.thumbnails.values()) {
      if (thumbnail.url !== null) this.dependencies.urls.revoke(thumbnail.url);
    }
    this.thumbnails.clear();
  }
}
