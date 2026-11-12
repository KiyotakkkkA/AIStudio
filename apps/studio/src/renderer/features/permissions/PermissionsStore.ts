import { makeAutoObservable, runInAction } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import type {
  AccessListDto,
  AppGrantDto,
  BrowserSiteDto,
  Contract,
  FolderGrantDto,
  FolderGrantId,
  ToolGrantDto,
  UserRuleDto,
} from "@zvs/shared";
import { CHANGE_FAILED } from "../access/accessCopy";
import { LOAD_ERROR, revokeErrorCopy } from "./permissionsPresentation";

export type AccessTab = "folders" | "sites" | "apps" | "tools" | "rules";

export interface PermissionsStoreOptions {
  readonly clock?: () => number;
}

export default class PermissionsStore {
  access: AccessListDto | null = null;
  grants: FolderGrantDto[] = [];
  tab: AccessTab = "folders";
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

  get sites(): readonly BrowserSiteDto[] {
    return this.access?.sites.sites ?? [];
  }

  get apps(): readonly AppGrantDto[] {
    return this.access?.apps ?? [];
  }

  get tools(): readonly ToolGrantDto[] {
    return this.access?.tools ?? [];
  }

  get rules(): readonly UserRuleDto[] {
    return this.access?.rules.rules ?? [];
  }

  get counts(): Record<AccessTab, number | null> {
    if (this.access === null)
      return { folders: null, sites: null, apps: null, tools: null, rules: null };
    return {
      folders: this.grants.length,
      sites: this.sites.length,
      apps: this.apps.length,
      tools: this.tools.length,
      rules: this.rules.length,
    };
  }

  setTab(tab: AccessTab) {
    this.tab = tab;
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
      const access = await this.ipc.call("access.list", undefined);
      runInAction(() => {
        if (revision !== this.revision) return;
        this.access = access;
        this.grants = access.folders;
        this.now = this.clock();
        this.loaded = true;
        if (this.error === LOAD_ERROR) this.error = null;
        if (
          this.selectedId !== null &&
          !access.folders.some((grant) => grant.id === this.selectedId)
        )
          this.selectedId = null;
        this.selectedId ??= access.folders[0]?.id ?? null;
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

  private async change(work: () => Promise<unknown>, failure: (error: unknown) => string) {
    if (this.revoking) return;
    this.revoking = true;
    this.error = null;
    try {
      await work();
    } catch (error) {
      runInAction(() => {
        this.error = failure(error);
      });
    } finally {
      runInAction(() => {
        this.revoking = false;
      });
    }
    await this.load();
  }

  async revoke(id: FolderGrantId) {
    await this.change(() => this.ipc.call("folders.revoke", { id }), revokeErrorCopy);
  }

  async revokeApp(id: string) {
    await this.change(() => this.ipc.call("computer.revoke", { id }), revokeErrorCopy);
  }

  async removeSite(origin: string) {
    await this.change(
      () => this.ipc.call("browserSites.remove", { origin }),
      () => CHANGE_FAILED,
    );
  }

  dispose() {
    ++this.revision;
  }
}
