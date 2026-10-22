import { makeAutoObservable, runInAction } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import {
  ExplorerChangesDto,
  type Contract,
  type ConversationId,
  type ExplorerChangeDto,
  type ExplorerEntryDto,
  type ExplorerFileDto,
  type StreamId,
} from "@zvs/shared";
import type { EventRouter, RoutedEvent } from "../../../app/EventRouter";
import { LIST_ERROR_FALLBACK, readErrorCopy, rootStateOf, type RootState } from "./explorerCopy";

export interface ExplorerFolder {
  readonly entries: readonly ExplorerEntryDto[];
  readonly truncated: boolean;
}

export interface ExplorerRow {
  readonly entry: ExplorerEntryDto;
  readonly depth: number;
  readonly expanded: boolean;
  readonly loading: boolean;
}

export type ExplorerMark = "created" | "modified";

export const VIEW_WIDTH_MIN = 360;
export const VIEW_WIDTH_DEFAULT = 560;

function parentOf(path: string): string {
  const at = path.lastIndexOf("/");
  return at === -1 ? "" : path.slice(0, at);
}

function within(path: string, folder: string): boolean {
  return path === folder || path.startsWith(`${folder}/`);
}

export default class ExplorerStore {
  conversationId: ConversationId | null = null;
  root: RootState = "ready";
  error: string | null = null;
  folders = new Map<string, ExplorerFolder>();
  loadingFolders = new Set<string>();
  expanded = new Set<string>();
  marks = new Map<string, ExplorerMark>();
  focused: string | null = null;
  openPath: string | null = null;
  file: ExplorerFileDto | null = null;
  fileLoading = false;
  fileError: string | null = null;
  fileRemoved = false;
  viewWidth = VIEW_WIDTH_DEFAULT;
  private streamId: StreamId | null = null;
  private stop: (() => void) | undefined;
  private generation = 0;
  private fileGeneration = 0;

  constructor(
    private readonly ipc: IpcClient<Contract>,
    private readonly events: EventRouter,
  ) {
    makeAutoObservable<
      ExplorerStore,
      "ipc" | "events" | "streamId" | "stop" | "generation" | "fileGeneration"
    >(
      this,
      {
        ipc: false,
        events: false,
        streamId: false,
        stop: false,
        generation: false,
        fileGeneration: false,
      },
      { autoBind: true },
    );
  }

  get rows(): ExplorerRow[] {
    const rows: ExplorerRow[] = [];
    const walk = (folder: string, depth: number) => {
      for (const entry of this.folders.get(folder)?.entries ?? []) {
        const expanded = entry.kind === "folder" && this.expanded.has(entry.path);
        rows.push({
          entry,
          depth,
          expanded,
          loading: expanded && this.loadingFolders.has(entry.path),
        });
        if (expanded) walk(entry.path, depth + 1);
      }
    };
    walk("", 0);
    return rows;
  }

  get rootLoaded() {
    return this.folders.has("");
  }

  get rootLoading() {
    return this.loadingFolders.has("");
  }

  get hasMarks() {
    return this.marks.size > 0;
  }

  get fileOpen() {
    return this.openPath !== null;
  }

  markOf(entry: ExplorerEntryDto): ExplorerMark | "inside" | null {
    const own = this.marks.get(entry.path);
    if (own !== undefined) return own;
    if (entry.kind !== "folder") return null;
    for (const path of this.marks.keys()) if (within(path, entry.path)) return "inside";
    return null;
  }

  async show(conversationId: ConversationId) {
    if (this.conversationId === conversationId && this.streamId !== null) return;
    this.hide();
    this.conversationId = conversationId;
    const generation = this.generation;
    await this.load("");
    if (generation !== this.generation || this.root !== "ready") return;
    await this.watch(generation);
  }

  hide() {
    this.generation += 1;
    this.fileGeneration += 1;
    this.stop?.();
    this.stop = undefined;
    const conversationId = this.conversationId;
    if (conversationId !== null && this.streamId !== null) {
      void this.ipc.call("workspace.unwatch", { id: conversationId }).catch(() => undefined);
    }
    this.streamId = null;
    this.conversationId = null;
    this.root = "ready";
    this.error = null;
    this.folders.clear();
    this.loadingFolders.clear();
    this.expanded.clear();
    this.marks.clear();
    this.focused = null;
    this.closeFile();
  }

  dispose() {
    this.hide();
  }

  async toggle(path: string) {
    if (this.expanded.has(path)) {
      this.expanded.delete(path);
      return;
    }
    this.expanded.add(path);
    if (!this.folders.has(path)) await this.load(path);
  }

  collapseAll() {
    this.expanded.clear();
    if (this.focused !== null && this.focused.includes("/")) this.focused = null;
  }

  clearMarks() {
    this.marks.clear();
  }

  focus(path: string | null) {
    this.focused = path;
  }

  setViewWidth(width: number) {
    this.viewWidth = Math.max(VIEW_WIDTH_MIN, Math.round(width));
  }

  async activate(entry: ExplorerEntryDto) {
    this.focused = entry.path;
    if (entry.kind === "folder") await this.toggle(entry.path);
    else await this.open(entry.path);
  }

  async open(path: string) {
    this.openPath = path;
    this.focused = path;
    this.marks.delete(path);
    this.file = null;
    this.fileRemoved = false;
    await this.loadFile(path);
  }

  closeFile() {
    this.fileGeneration += 1;
    this.openPath = null;
    this.file = null;
    this.fileLoading = false;
    this.fileError = null;
    this.fileRemoved = false;
  }

  moveFocus(step: 1 | -1) {
    const rows = this.rows;
    if (rows.length === 0) return;
    const index = rows.findIndex((row) => row.entry.path === this.focused);
    const next = index === -1 ? 0 : Math.min(rows.length - 1, Math.max(0, index + step));
    this.focused = rows[next]?.entry.path ?? null;
  }

  async expandFocused() {
    const row = this.rows.find((candidate) => candidate.entry.path === this.focused);
    if (row?.entry.kind !== "folder") return;
    if (!row.expanded) await this.toggle(row.entry.path);
    else this.moveFocus(1);
  }

  collapseFocused() {
    const row = this.rows.find((candidate) => candidate.entry.path === this.focused);
    if (row === undefined) return;
    if (row.expanded) {
      this.expanded.delete(row.entry.path);
      return;
    }
    const parent = parentOf(row.entry.path);
    if (parent !== "") this.focused = parent;
  }

  private async load(path: string) {
    const conversationId = this.conversationId;
    if (conversationId === null) return;
    const generation = this.generation;
    this.loadingFolders.add(path);
    try {
      const listing = await this.ipc.call("workspace.list", { id: conversationId, path });
      if (generation !== this.generation) return;
      runInAction(() => {
        this.folders.set(path, { entries: listing.entries, truncated: listing.truncated });
        if (path === "") {
          this.root = "ready";
          this.error = null;
        }
      });
    } catch (error: unknown) {
      if (generation !== this.generation) return;
      runInAction(() => {
        const state = rootStateOf(error);
        if (path === "" && state !== null) {
          this.root = state;
          this.folders.clear();
          return;
        }
        if (state !== null && path !== "") {
          this.forget(path);
          return;
        }
        this.error = LIST_ERROR_FALLBACK;
      });
    } finally {
      if (generation === this.generation) runInAction(() => this.loadingFolders.delete(path));
    }
  }

  private async loadFile(path: string) {
    const conversationId = this.conversationId;
    if (conversationId === null) return;
    const generation = ++this.fileGeneration;
    this.fileLoading = true;
    this.fileError = null;
    try {
      const file = await this.ipc.call("workspace.read", { id: conversationId, path });
      if (generation !== this.fileGeneration) return;
      runInAction(() => {
        this.file = file;
        this.fileRemoved = false;
      });
    } catch (error: unknown) {
      if (generation !== this.fileGeneration) return;
      runInAction(() => {
        if (rootStateOf(error) === "gone" && this.file !== null) this.fileRemoved = true;
        else this.fileError = readErrorCopy(error);
      });
    } finally {
      if (generation === this.fileGeneration) runInAction(() => (this.fileLoading = false));
    }
  }

  private async watch(generation: number) {
    const conversationId = this.conversationId;
    if (conversationId === null) return;
    try {
      const handle = await this.ipc.call("workspace.watch", { id: conversationId });
      if (generation !== this.generation) {
        void this.ipc.call("workspace.unwatch", { id: conversationId }).catch(() => undefined);
        return;
      }
      this.streamId = handle.streamId;
      this.stop = this.events.subscribe(handle.streamId, this.receive);
    } catch (error: unknown) {
      if (generation !== this.generation) return;
      runInAction(() => {
        const state = rootStateOf(error);
        if (state !== null) this.root = state;
      });
    }
  }

  private receive(event: RoutedEvent) {
    if (event.type !== "step") return;
    const parsed = ExplorerChangesDto.safeParse(event.step);
    if (!parsed.success || parsed.data.conversationId !== this.conversationId) return;
    this.apply(parsed.data.changes);
  }

  private forget(path: string) {
    for (const key of [...this.folders.keys()]) if (within(key, path)) this.folders.delete(key);
    for (const key of [...this.expanded]) if (within(key, path)) this.expanded.delete(key);
    for (const key of [...this.marks.keys()]) if (within(key, path)) this.marks.delete(key);
    if (this.focused !== null && within(this.focused, path)) this.focused = null;
  }

  apply(changes: readonly ExplorerChangeDto[]) {
    const reload = new Set<string>();
    let reopen = false;
    for (const change of changes) {
      if (change.path === "") {
        if (change.kind === "removed") {
          this.root = "gone";
          this.folders.clear();
          this.expanded.clear();
          if (this.openPath !== null) this.fileRemoved = true;
          return;
        }
        this.root = "ready";
        for (const folder of this.folders.keys()) reload.add(folder);
        reload.add("");
        reopen = this.openPath !== null;
        continue;
      }
      const parent = parentOf(change.path);
      if (this.folders.has(parent)) reload.add(parent);
      if (change.kind === "removed") {
        this.forget(change.path);
        if (this.openPath !== null && within(this.openPath, change.path)) this.fileRemoved = true;
        continue;
      }
      const previous = this.marks.get(change.path);
      this.marks.set(
        change.path,
        previous === "created" || change.kind === "created" ? "created" : "modified",
      );
      if (change.path === this.openPath) {
        reopen = true;
        this.marks.delete(change.path);
      }
    }
    for (const folder of reload) void this.load(folder);
    if (reopen && this.openPath !== null) void this.loadFile(this.openPath);
  }
}
