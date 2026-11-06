import { makeAutoObservable, runInAction } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import type { Contract, FolderGrantDto, FolderGrantId } from "@zvs/shared";
import { LOAD_ERROR, revokeErrorCopy } from "./permissionsPresentation";

export interface PermissionsStoreOptions {
  readonly clock?: () => number;
}

export default class PermissionsStore {
  grants: FolderGrantDto[] = [];
  loading = false;
  loaded = false;
  error: string | null = null;
  query = "";
  selectedId: FolderGrantId | null = null;
  revoking = false;
  now: number;
  private readonly clock: () => number;
  private revision = 0;

  constructor(
    private readonly ipc: IpcClient<Contract>,
    options: PermissionsStoreOptions = {},
  ) {
    this.clock = options.clock ?? Date.now;
    this.now = this.clock();
    makeAutoObservable<PermissionsStore, "ipc" | "clock" | "revision">(
      this,
      { ipc: false, clock: false, revision: false },
      { autoBind: true },
    );
  }

  get visible(): FolderGrantDto[] {
    const query = this.query.trim().toLocaleLowerCase("ru-RU");
    if (query.length === 0) return this.grants;
    return this.grants.filter(
      (grant) =>
        grant.path.toLocaleLowerCase("ru-RU").includes(query) ||
        [grant.conversation, ...grant.boundConversations].some(
          (chat) => chat !== null && chat.title.toLocaleLowerCase("ru-RU").includes(query),
        ),
    );
  }

  get always(): FolderGrantDto[] {
    return this.visible.filter((grant) => grant.scope === "always");
  }

  get dialog(): FolderGrantDto[] {
    return this.visible.filter((grant) => grant.scope === "conversation");
  }

  get selected(): FolderGrantDto | null {
    return this.grants.find((grant) => grant.id === this.selectedId) ?? null;
  }

  get isEmpty(): boolean {
    return this.loaded && this.grants.length === 0;
  }

  setQuery(query: string) {
    this.query = query;
  }

  select(id: FolderGrantId) {
    this.selectedId = id;
  }

  dismissError() {
    this.error = null;
  }

  async load() {
    const revision = ++this.revision;
    this.loading = true;
    try {
      const grants = await this.ipc.call("folders.list", undefined);
      runInAction(() => {
        if (revision !== this.revision) return;
        this.grants = grants;
        this.now = this.clock();
        this.loaded = true;
        if (this.error === LOAD_ERROR) this.error = null;
        if (this.selectedId !== null && !grants.some((grant) => grant.id === this.selectedId))
          this.selectedId = null;
        this.selectedId ??= grants[0]?.id ?? null;
      });
    } catch {
      runInAction(() => {
        if (revision === this.revision) this.error = LOAD_ERROR;
      });
    } finally {
      runInAction(() => {
        if (revision === this.revision) this.loading = false;
      });
    }
  }

  async revoke(id: FolderGrantId) {
    if (this.revoking) return;
    this.revoking = true;
    this.error = null;
    try {
      await this.ipc.call("folders.revoke", { id });
    } catch (error) {
      runInAction(() => {
        this.error = revokeErrorCopy(error);
      });
    } finally {
      runInAction(() => {
        this.revoking = false;
      });
    }
    await this.load();
  }

  dispose() {
    ++this.revision;
  }
}
