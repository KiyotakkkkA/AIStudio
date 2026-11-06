import { makeAutoObservable, runInAction } from "mobx";
import { isIpcError, type IpcClient } from "@zvs/ipc";
import {
  AppErrorCode,
  type BrowserBookmarkDto,
  type BrowserHistoryEntryDto,
  type BrowserImportSourceDto,
  type Contract,
} from "@zvs/shared";
import {
  BOOKMARKS_ACTION_FAILED,
  BOOKMARKS_LOAD_FAILED,
  HISTORY_ACTION_FAILED,
  HISTORY_LOAD_FAILED,
  IMPORT_FAILED,
  IMPORT_GONE,
  IMPORT_LOCKED,
  IMPORT_SOURCES_FAILED,
  importResultLabel,
} from "./browserLibraryCopy";

function importFailure(error: unknown): string {
  if (isIpcError(error) && error.code === AppErrorCode.CONFLICT) return IMPORT_LOCKED;
  if (isIpcError(error) && error.code === AppErrorCode.NOT_FOUND) return IMPORT_GONE;
  return IMPORT_FAILED;
}

export interface ImportChoice {
  readonly history: boolean;
  readonly bookmarks: boolean;
}

export default class BrowserLibraryStore {
  history: BrowserHistoryEntryDto[] = [];
  historyTotal = 0;
  historyLoaded = false;
  query = "";
  bookmarks: BrowserBookmarkDto[] = [];
  bookmarksLoaded = false;
  renamingId: string | null = null;
  renameDraft = "";
  sources: BrowserImportSourceDto[] = [];
  sourcesLoaded = false;
  choices: Record<string, ImportChoice> = {};
  importing: string | null = null;
  notice: string | null = null;
  busy = false;
  error: string | null = null;
  private revision = 0;

  constructor(private readonly ipc: IpcClient<Contract>) {
    makeAutoObservable<BrowserLibraryStore, "ipc" | "revision">(
      this,
      { ipc: false, revision: false },
      { autoBind: true },
    );
  }

  async loadHistory() {
    const revision = ++this.revision;
    const query = this.query.trim();
    try {
      const page = await this.ipc.call("browserHistory.list", query.length === 0 ? {} : { query });
      if (revision !== this.revision) return;
      runInAction(() => {
        this.history = page.items;
        this.historyTotal = page.total;
        this.historyLoaded = true;
      });
    } catch {
      runInAction(() => {
        this.historyLoaded = true;
        this.error = HISTORY_LOAD_FAILED;
      });
    }
  }

  setQuery(query: string) {
    this.query = query;
    void this.loadHistory();
  }

  private async run(work: () => Promise<unknown>, failure: string): Promise<boolean> {
    this.busy = true;
    this.error = null;
    try {
      await work();
      return true;
    } catch {
      runInAction(() => {
        this.error = failure;
      });
      return false;
    } finally {
      runInAction(() => {
        this.busy = false;
      });
    }
  }

  async removeVisit(id: string) {
    await this.run(() => this.ipc.call("browserHistory.remove", { id }), HISTORY_ACTION_FAILED);
    await this.loadHistory();
  }

  async clearHistory() {
    await this.run(() => this.ipc.call("browserHistory.clear", undefined), HISTORY_ACTION_FAILED);
    await this.loadHistory();
  }

  async loadBookmarks() {
    try {
      const list = await this.ipc.call("browserBookmarks.list", undefined);
      runInAction(() => {
        this.bookmarks = list.items;
        this.bookmarksLoaded = true;
      });
    } catch {
      runInAction(() => {
        this.bookmarksLoaded = true;
        this.error = BOOKMARKS_LOAD_FAILED;
      });
    }
  }

  startRename(bookmark: BrowserBookmarkDto) {
    this.renamingId = bookmark.id;
    this.renameDraft = bookmark.title;
  }

  setRenameDraft(value: string) {
    this.renameDraft = value;
  }

  cancelRename() {
    this.renamingId = null;
  }

  get canRename(): boolean {
    return this.renamingId !== null && this.renameDraft.trim().length > 0 && !this.busy;
  }

  async saveRename() {
    const id = this.renamingId;
    if (id === null || !this.canRename) return;
    const title = this.renameDraft.trim();
    const saved = await this.run(async () => {
      const list = await this.ipc.call("browserBookmarks.rename", { id, title });
      runInAction(() => {
        this.bookmarks = list.items;
      });
    }, BOOKMARKS_ACTION_FAILED);
    if (saved)
      runInAction(() => {
        this.renamingId = null;
      });
  }

  async removeBookmark(id: string) {
    await this.run(async () => {
      const list = await this.ipc.call("browserBookmarks.remove", { id });
      runInAction(() => {
        this.bookmarks = list.items;
      });
    }, BOOKMARKS_ACTION_FAILED);
  }

  async loadSources() {
    try {
      const found = await this.ipc.call("browserImport.sources", undefined);
      runInAction(() => {
        this.sources = found.sources;
        this.sourcesLoaded = true;
        for (const source of found.sources) {
          this.choices[source.id] ??= {
            history: source.history,
            bookmarks: source.bookmarks,
          };
        }
      });
    } catch {
      runInAction(() => {
        this.sourcesLoaded = true;
        this.error = IMPORT_SOURCES_FAILED;
      });
    }
  }

  choiceOf(id: string): ImportChoice {
    return this.choices[id] ?? { history: false, bookmarks: false };
  }

  setChoice(id: string, choice: Partial<ImportChoice>) {
    this.choices[id] = { ...this.choiceOf(id), ...choice };
  }

  async runImport(source: BrowserImportSourceDto) {
    const choice = this.choiceOf(source.id);
    if (this.importing !== null || (!choice.history && !choice.bookmarks)) return;
    this.importing = source.id;
    this.error = null;
    this.notice = null;
    try {
      const result = await this.ipc.call("browserImport.run", {
        source: source.id,
        history: choice.history,
        bookmarks: choice.bookmarks,
      });
      runInAction(() => {
        this.notice = importResultLabel(source.kind, result);
      });
    } catch (error) {
      runInAction(() => {
        this.error = importFailure(error);
      });
    } finally {
      runInAction(() => {
        this.importing = null;
      });
    }
  }

  dismissError() {
    this.error = null;
  }
}
