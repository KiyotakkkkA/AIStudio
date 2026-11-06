import { makeAutoObservable, runInAction } from "mobx";
import { isIpcError, type IpcClient } from "@zvs/ipc";
import {
  AppErrorCode,
  type BrowserSiteAction,
  type BrowserSiteDecision,
  type BrowserSiteRulesDto,
  type BrowserSitesDto,
  type Contract,
} from "@zvs/shared";
import { SITE_INVALID, SITES_LOAD_FAILED, SITES_SAVE_FAILED } from "./browserSitesCopy";

export const DEFAULT_SITE_RULES = {
  browse: "allow",
  click: "ask",
  type: "ask",
  submit: "ask",
  download: "ask",
} as const satisfies BrowserSitesDto["defaults"];

function failure(error: unknown): string {
  if (isIpcError(error) && error.code === AppErrorCode.VALIDATION_FAILED) return SITE_INVALID;
  return SITES_SAVE_FAILED;
}

export default class BrowserSitesStore {
  sites: BrowserSitesDto = { defaults: { ...DEFAULT_SITE_RULES }, sites: [] };
  loaded = false;
  busy = false;
  error: string | null = null;
  adding = false;
  draftSite = "";
  draftRules: BrowserSiteRulesDto = {};

  constructor(private readonly ipc: IpcClient<Contract>) {
    makeAutoObservable<BrowserSitesStore, "ipc">(this, { ipc: false }, { autoBind: true });
  }

  get canAdd(): boolean {
    return this.draftSite.trim().length > 0 && !this.busy;
  }

  async load() {
    try {
      const sites = await this.ipc.call("browserSites.get", undefined);
      runInAction(() => {
        this.sites = sites;
        this.loaded = true;
      });
    } catch {
      runInAction(() => {
        this.loaded = true;
        this.error = SITES_LOAD_FAILED;
      });
    }
  }

  private async apply(work: () => Promise<BrowserSitesDto>): Promise<boolean> {
    this.busy = true;
    this.error = null;
    try {
      const sites = await work();
      runInAction(() => {
        this.sites = sites;
      });
      return true;
    } catch (error) {
      runInAction(() => {
        this.error = failure(error);
      });
      return false;
    } finally {
      runInAction(() => {
        this.busy = false;
      });
    }
  }

  setDefault(action: BrowserSiteAction, decision: BrowserSiteDecision) {
    return this.apply(() => this.ipc.call("browserSites.setDefault", { action, decision }));
  }

  setRule(origin: string, action: BrowserSiteAction, decision: BrowserSiteDecision | null) {
    const site = this.sites.sites.find((entry) => entry.origin === origin);
    const rules: BrowserSiteRulesDto = { ...(site?.rules ?? {}) };
    if (decision === null) delete rules[action];
    else rules[action] = decision;
    return this.apply(() => this.ipc.call("browserSites.put", { site: origin, rules }));
  }

  remove(origin: string) {
    return this.apply(() => this.ipc.call("browserSites.remove", { origin }));
  }

  startAdding() {
    this.adding = true;
    this.draftSite = "";
    this.draftRules = {};
    this.error = null;
  }

  cancelAdding() {
    this.adding = false;
  }

  setDraftSite(value: string) {
    this.draftSite = value;
  }

  setDraftRule(action: BrowserSiteAction, decision: BrowserSiteDecision | null) {
    const rules = { ...this.draftRules };
    if (decision === null) delete rules[action];
    else rules[action] = decision;
    this.draftRules = rules;
  }

  async addDraft() {
    if (!this.canAdd) return;
    const site = this.draftSite.trim();
    const rules = this.draftRules;
    const saved = await this.apply(() => this.ipc.call("browserSites.put", { site, rules }));
    if (saved)
      runInAction(() => {
        this.adding = false;
      });
  }

  dismissError() {
    this.error = null;
  }
}
