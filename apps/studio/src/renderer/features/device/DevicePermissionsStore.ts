import { makeAutoObservable, runInAction } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import type {
  AppGrantDto,
  Contract,
  ConversationDto,
  ConversationId,
  FolderGrantScope,
} from "@zvs/shared";
import { APP_ADD_FAILED, APP_LOAD_FAILED, APP_PICK_FAILED } from "./deviceCopy";
import { revokeErrorCopy } from "../permissions/permissionsPresentation";

export interface AppPermissionGroup {
  readonly app: string;
  readonly scope: FolderGrantScope;
  readonly grants: readonly AppGrantDto[];
}

export function groupAppPermissions(grants: readonly AppGrantDto[]): AppPermissionGroup[] {
  const groups = new Map<string, { app: string; scope: FolderGrantScope; grants: AppGrantDto[] }>();
  for (const grant of grants) {
    const key = `${grant.scope}:${grant.app}`;
    const group = groups.get(key) ?? { app: grant.app, scope: grant.scope, grants: [] };
    group.grants.push(grant);
    groups.set(key, group);
  }
  return [...groups.values()].sort((a, b) => a.app.localeCompare(b.app, "ru-RU"));
}

export default class DevicePermissionsStore {
  grants: AppGrantDto[] = [];
  conversations: ConversationDto[] = [];
  loaded = false;
  loading = false;
  busy = false;
  error: string | null = null;
  selected: { app: string; scope: FolderGrantScope } | null = null;
  draftPath: string | null = null;
  draftScope: FolderGrantScope = "always";
  draftConversations: ConversationId[] = [];
  now: number;

  constructor(
    private readonly ipc: IpcClient<Contract>,
    private readonly clock: () => number = Date.now,
  ) {
    this.now = clock();
    makeAutoObservable<DevicePermissionsStore, "ipc" | "clock">(
      this,
      { ipc: false, clock: false },
      { autoBind: true },
    );
  }
  get groups() {
    return groupAppPermissions(this.grants);
  }
  get selectedGroup() {
    return (
      this.groups.find(
        (group) => group.app === this.selected?.app && group.scope === this.selected.scope,
      ) ?? null
    );
  }
  get canSave() {
    return (
      this.draftPath !== null &&
      !this.busy &&
      (this.draftScope === "always" || this.draftConversations.length > 0)
    );
  }
  select(group: AppPermissionGroup | null) {
    this.selected = group === null ? null : { app: group.app, scope: group.scope };
  }
  dismissError() {
    this.error = null;
  }
  setScope(scope: FolderGrantScope) {
    this.draftScope = scope;
  }
  toggleConversation(id: ConversationId) {
    this.draftConversations = this.draftConversations.includes(id)
      ? this.draftConversations.filter((item) => item !== id)
      : [...this.draftConversations, id];
  }
  cancelAdding() {
    this.draftPath = null;
    this.draftConversations = [];
  }
  async load() {
    if (this.loading) return;
    this.loading = true;
    this.error = null;
    try {
      const grants = await this.ipc.call("computer.grants", undefined);
      runInAction(() => {
        this.grants = grants;
        this.loaded = true;
        this.now = this.clock();
      });
    } catch {
      runInAction(() => {
        this.error = APP_LOAD_FAILED;
      });
    } finally {
      runInAction(() => {
        this.loading = false;
      });
    }
  }
  async pickApp() {
    if (this.busy) return;
    this.busy = true;
    this.error = null;
    try {
      const path = await this.ipc.call("computer.pickApp", undefined);
      if (path === null) return;
      const conversations = await this.ipc.call("chat.conversations.list", undefined);
      runInAction(() => {
        this.conversations = conversations;
        this.draftPath = path;
        this.draftScope = "always";
        this.draftConversations = [];
      });
    } catch {
      runInAction(() => {
        this.error = APP_PICK_FAILED;
      });
    } finally {
      runInAction(() => {
        this.busy = false;
      });
    }
  }
  async save() {
    if (!this.canSave || this.draftPath === null) return;
    const input = {
      path: this.draftPath,
      scope: this.draftScope,
      conversations: this.draftScope === "always" ? [] : [...this.draftConversations],
    };
    this.busy = true;
    this.error = null;
    try {
      const grants = await this.ipc.call("computer.addGrant", input);
      runInAction(() => {
        this.grants = grants;
        this.loaded = true;
        this.now = this.clock();
        this.cancelAdding();
      });
    } catch {
      runInAction(() => {
        this.error = APP_ADD_FAILED;
      });
    } finally {
      runInAction(() => {
        this.busy = false;
      });
    }
  }
  async revoke(id: string) {
    if (this.busy) return;
    this.busy = true;
    this.error = null;
    try {
      await this.ipc.call("computer.revoke", { id });
      const grants = await this.ipc.call("computer.grants", undefined);
      runInAction(() => {
        this.grants = grants;
        if (this.selectedGroup === null) this.selected = null;
        this.now = this.clock();
      });
    } catch (error) {
      runInAction(() => {
        this.error = revokeErrorCopy(error);
      });
    } finally {
      runInAction(() => {
        this.busy = false;
      });
    }
  }
}
