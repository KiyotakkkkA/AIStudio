import { makeAutoObservable, runInAction } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import type {
  ArtifactDto,
  ChatCitationDto,
  Contract,
  DocumentDto,
  DocumentId,
  DocumentRef,
  DocumentSelectionDto,
} from "@zvs/shared";
import {
  VIEWER_DOCK_DEFAULT_WIDTH,
  VIEWER_DOCK_MIN_WIDTH,
  clampPage,
  clampZoom,
  clipSelection,
  decodeBase64,
  joinBytes,
  nextZoom,
} from "../viewer/viewerModel";
import {
  REVEAL_FAILED,
  SAVED,
  SAVE_FAILED,
  actionErrorCopy,
  viewerProblem,
  type ViewerProblem,
} from "../viewer/viewerCopy";

export const VIEWER_WIDTH_KEY = "viewer.dock.width";
export const VIEWER_ZOOM_KEY = "viewer.zoom";
export const VIEWER_FIT_KEY = "viewer.fit";
export const VIEWER_DOCK_MAX_WIDTH = 1600;

export type ViewerStatus = "idle" | "loading" | "ready" | "failed";

export type ViewerContent =
  | { readonly type: "pdf"; readonly bytes: Uint8Array }
  | { readonly type: "image"; readonly url: string }
  | {
      readonly type: "text";
      readonly text: string;
      readonly kind: "markdown" | "code" | "csv" | "text";
      readonly language: string | null;
    }
  | { readonly type: "docx" | "xlsx" | "pptx"; readonly bytes: Uint8Array };

export interface ViewerTarget {
  readonly ref: DocumentRef;
  readonly name: string;
  readonly location: string | null;
}

export interface ViewerSelection {
  readonly page: number | null;
  readonly start: number;
  readonly end: number;
  readonly text: string;
}

export interface ViewerSink {
  attachSelection(selection: DocumentSelectionDto): void;
}

export interface OpenOptions {
  readonly name?: string;
  readonly location?: string | null;
  readonly page?: number | null;
  readonly passage?: Promise<string | null> | null;
}

export interface ViewerUrls {
  create(bytes: Uint8Array, type: string): string;
  revoke(url: string): void;
}

const browserUrls: ViewerUrls = {
  create: (bytes, type) => URL.createObjectURL(new Blob([bytes as BlobPart], { type })),
  revoke: (url) => URL.revokeObjectURL(url),
};

function nameOf(path: string): string {
  return (
    path
      .split(/[\\/]/u)
      .filter((part) => part.length > 0)
      .at(-1) ?? path
  );
}

export default class ViewerStore {
  target: ViewerTarget | null = null;
  status: ViewerStatus = "idle";
  problem: ViewerProblem | null = null;
  document: DocumentDto | null = null;
  content: ViewerContent | null = null;
  pageCount: number | null = null;
  page = 1;
  pageRequest: { readonly page: number; readonly nonce: number } | null = null;
  zoom = 1;
  fitWidth = true;
  dockWidth = VIEWER_DOCK_DEFAULT_WIDTH;
  expanded = false;
  selection: ViewerSelection | null = null;
  passage: string | null = null;
  searchOpen = false;
  query = "";
  matchCount = 0;
  matchIndex = 0;
  saving = false;
  notice: string | null = null;
  actionError: string | null = null;
  private generation = 0;
  private nonce = 0;

  constructor(
    private readonly ipc: IpcClient<Contract>,
    private readonly sink: ViewerSink,
    private readonly urls: ViewerUrls = browserUrls,
  ) {
    makeAutoObservable<ViewerStore, "ipc" | "sink" | "urls" | "generation" | "nonce">(
      this,
      { ipc: false, sink: false, urls: false, generation: false, nonce: false },
      { autoBind: true },
    );
  }

  get isOpen(): boolean {
    return this.target !== null;
  }

  get name(): string {
    return this.document?.name ?? this.target?.name ?? "";
  }

  get canAsk(): boolean {
    return this.selection !== null && this.document !== null;
  }

  async restore(): Promise<void> {
    const [width, zoom, fit] = await Promise.all([
      this.read(VIEWER_WIDTH_KEY),
      this.read(VIEWER_ZOOM_KEY),
      this.read(VIEWER_FIT_KEY),
    ]);
    runInAction(() => {
      if (typeof width === "number") this.dockWidth = this.clampWidth(width);
      if (typeof zoom === "number") this.zoom = clampZoom(zoom);
      if (typeof fit === "boolean") this.fitWidth = fit;
    });
  }

  openPath(path: string, location: string | null = null): Promise<void> {
    return this.open({ source: "path", path }, { name: nameOf(path), location });
  }

  openArtifact(artifact: ArtifactDto): Promise<void> {
    return this.open({ source: "artifact", artifactId: artifact.id }, { name: artifact.name });
  }

  openCitation(citation: ChatCitationDto, location: string | null = null): Promise<void> {
    const passage = this.ipc
      .call("vectorStores.chunks", {
        storeId: citation.storeId,
        documentId: citation.documentId,
        indices: [citation.chunkIndex],
      })
      .then((chunks) => chunks[0]?.text ?? null)
      .catch(() => null);
    return this.open(
      { source: "citation", citation },
      { name: nameOf(citation.sourcePath), location, page: citation.page, passage },
    );
  }

  openSelection(
    selection: DocumentSelectionDto & { readonly documentId: DocumentId },
    location: string | null = null,
  ): Promise<void> {
    return this.open(
      { source: "document", documentId: selection.documentId },
      {
        name: selection.name,
        location,
        page: selection.page,
        passage: Promise.resolve(selection.text),
      },
    );
  }

  async open(ref: DocumentRef, options: OpenOptions = {}): Promise<void> {
    this.release();
    const generation = ++this.generation;
    const name = options.name ?? "";
    this.target = { ref, name, location: options.location ?? null };
    this.status = "loading";
    this.page = clampPage(options.page ?? 1, Number.MAX_SAFE_INTEGER);
    this.pageRequest = null;
    this.passage = null;
    this.resetSearch();
    const passage = options.passage ?? null;
    let document: DocumentDto;
    try {
      document = await this.ipc.call("documents.open", ref);
    } catch (error) {
      runInAction(() => {
        if (generation !== this.generation) return;
        this.status = "failed";
        this.problem = viewerProblem(error);
      });
      return;
    }
    if (generation !== this.generation) {
      void this.ipc.call("documents.close", { id: document.id }).catch(() => undefined);
      return;
    }
    runInAction(() => {
      this.document = document;
      this.status = "loading";
      this.pageCount = document.pageCount ?? null;
      const page = options.page ?? document.initialPage ?? null;
      if (page !== null) this.requestPage(page);
    });
    try {
      const content = await this.load(document);
      const text = await passage;
      runInAction(() => {
        if (generation !== this.generation) {
          if (content.type === "image") this.urls.revoke(content.url);
          return;
        }
        this.content = content;
        this.passage = text;
        this.status = "ready";
      });
    } catch (error) {
      runInAction(() => {
        if (generation !== this.generation) return;
        this.status = "failed";
        this.problem = viewerProblem(error);
      });
    }
  }

  close(): void {
    this.release();
    this.generation += 1;
    this.target = null;
    this.status = "idle";
    this.expanded = false;
  }

  setPageCount(count: number): void {
    if (count <= 0) return;
    this.pageCount = count;
    this.page = clampPage(this.page, count);
  }

  reportPage(page: number): void {
    this.page = clampPage(page, this.pageCount ?? page);
  }

  requestPage(page: number): void {
    const next = clampPage(page, this.pageCount ?? Math.max(page, 1));
    this.page = next;
    this.nonce += 1;
    this.pageRequest = { page: next, nonce: this.nonce };
  }

  previousPage(): void {
    if (this.page > 1) this.requestPage(this.page - 1);
  }

  nextPage(): void {
    if (this.pageCount !== null && this.page < this.pageCount) this.requestPage(this.page + 1);
  }

  setZoom(zoom: number): void {
    this.fitWidth = false;
    this.zoom = clampZoom(zoom);
    void this.write(VIEWER_ZOOM_KEY, this.zoom);
    void this.write(VIEWER_FIT_KEY, false);
  }

  zoomIn(): void {
    this.setZoom(nextZoom(this.zoom, 1));
  }

  zoomOut(): void {
    this.setZoom(nextZoom(this.zoom, -1));
  }

  setFitWidth(): void {
    this.fitWidth = true;
    void this.write(VIEWER_FIT_KEY, true);
  }

  applyFitZoom(zoom: number): void {
    if (this.fitWidth) this.zoom = clampZoom(zoom);
  }

  setDockWidth(width: number): void {
    this.dockWidth = this.clampWidth(width);
  }

  commitDockWidth(): void {
    void this.write(VIEWER_WIDTH_KEY, this.dockWidth);
  }

  toggleExpanded(): void {
    this.expanded = !this.expanded;
  }

  select(selection: ViewerSelection | null): void {
    if (selection === null) {
      this.selection = null;
      return;
    }
    const text = clipSelection(selection.text);
    this.selection =
      text.length === 0 || selection.end <= selection.start ? null : { ...selection, text };
  }

  ask(): boolean {
    const document = this.document;
    const selection = this.selection;
    if (document === null || selection === null) return false;
    this.sink.attachSelection({
      documentId: document.id,
      name: document.name,
      page: selection.page,
      start: selection.start,
      end: selection.end,
      text: selection.text,
      untrusted: true,
    });
    this.selection = null;
    this.expanded = false;
    return true;
  }

  openSearch(): void {
    this.searchOpen = true;
  }

  closeSearch(): void {
    this.resetSearch();
  }

  setQuery(query: string): void {
    this.query = query;
    this.matchIndex = 0;
  }

  setMatchCount(count: number): void {
    this.matchCount = count;
    if (this.matchIndex >= count) this.matchIndex = 0;
  }

  nextMatch(): void {
    if (this.matchCount > 0) this.matchIndex = (this.matchIndex + 1) % this.matchCount;
  }

  previousMatch(): void {
    if (this.matchCount > 0)
      this.matchIndex = (this.matchIndex - 1 + this.matchCount) % this.matchCount;
  }

  async save(): Promise<void> {
    const document = this.document;
    if (document === null || this.saving) return;
    this.saving = true;
    this.notice = null;
    this.actionError = null;
    try {
      const result = await this.ipc.call("documents.save", { id: document.id });
      runInAction(() => {
        if (result.path !== undefined) this.notice = SAVED;
      });
    } catch (error) {
      runInAction(() => {
        this.actionError = actionErrorCopy(error, SAVE_FAILED);
      });
    } finally {
      runInAction(() => {
        this.saving = false;
      });
    }
  }

  async reveal(): Promise<void> {
    const document = this.document;
    if (document === null) return;
    this.actionError = null;
    try {
      await this.ipc.call("documents.reveal", { id: document.id });
    } catch (error) {
      runInAction(() => {
        this.actionError = actionErrorCopy(error, REVEAL_FAILED);
      });
    }
  }

  dispose(): void {
    this.close();
  }

  private async load(document: DocumentDto): Promise<ViewerContent> {
    if (document.kind === "markdown" || document.kind === "code" || document.kind === "csv") {
      return {
        type: "text",
        text: await this.text(document),
        kind: document.kind,
        language: document.language ?? null,
      };
    }
    if (document.kind === "text") {
      return { type: "text", text: await this.text(document), kind: "text", language: null };
    }
    const bytes = await this.bytes(document);
    if (document.kind === "pdf") return { type: "pdf", bytes };
    if (document.kind === "image")
      return { type: "image", url: this.urls.create(bytes, document.mimeType) };
    return { type: document.officeFormat ?? "docx", bytes };
  }

  private async text(document: DocumentDto): Promise<string> {
    const parts: string[] = [];
    for (let index = 0; index < document.partCount; index += 1) {
      const page = await this.ipc.call("documents.page", { id: document.id, index });
      if (page.content !== "text") throw new Error("unexpected part");
      parts.push(page.text);
    }
    return parts.join("");
  }

  private async bytes(document: DocumentDto): Promise<Uint8Array> {
    const parts: Uint8Array[] = [];
    let total = document.sizeBytes;
    for (let index = 0; index < document.partCount; index += 1) {
      const page = await this.ipc.call("documents.page", { id: document.id, index });
      if (page.content !== "bytes") throw new Error("unexpected part");
      total = page.totalBytes;
      parts.push(decodeBase64(page.data));
    }
    return joinBytes(parts, total);
  }

  private release(): void {
    const document = this.document;
    if (document !== null) {
      void this.ipc.call("documents.close", { id: document.id }).catch(() => undefined);
    }
    if (this.content?.type === "image") this.urls.revoke(this.content.url);
    this.document = null;
    this.content = null;
    this.problem = null;
    this.pageCount = null;
    this.selection = null;
    this.passage = null;
    this.notice = null;
    this.actionError = null;
  }

  private resetSearch(): void {
    this.searchOpen = false;
    this.query = "";
    this.matchCount = 0;
    this.matchIndex = 0;
  }

  private clampWidth(width: number): number {
    return Math.min(VIEWER_DOCK_MAX_WIDTH, Math.max(VIEWER_DOCK_MIN_WIDTH, Math.round(width)));
  }

  private async read(key: string): Promise<unknown> {
    try {
      const stored = await this.ipc.call("settings.get", { key });
      return stored.value;
    } catch {
      return undefined;
    }
  }

  private async write(key: string, value: boolean | number): Promise<void> {
    try {
      await this.ipc.call("settings.set", { key, value });
    } catch {
      return;
    }
  }
}
