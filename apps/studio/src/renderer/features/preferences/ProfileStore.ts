import { makeAutoObservable, runInAction } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import type { Contract, Json } from "@zvs/shared";
import { SETTING_SAVE_FAILED } from "./preferencesPresentation";

export const REPLY_STYLES = ["standard", "concise", "detailed", "formal", "custom"] as const;
export type ReplyStyle = (typeof REPLY_STYLES)[number];

export const PROFILE_KEYS = {
  style: "profile.style",
  custom: "profile.style.custom",
  about: "profile.about",
  toolCalls: "chat.toolcalls",
} as const;
export type ProfileField = keyof typeof PROFILE_KEYS;

export const PROFILE_STYLE_MAX = 2000;
export const PROFILE_ABOUT_MAX = 4000;
export const TOOL_CALLS_MIN = 1;
export const TOOL_CALLS_MAX = 50;
export const TOOL_CALLS_DEFAULT = 15;

function isReplyStyle(value: unknown): value is ReplyStyle {
  return typeof value === "string" && (REPLY_STYLES as readonly string[]).includes(value);
}

export function parseToolCalls(raw: string): number | null {
  const trimmed = raw.trim();
  if (!/^\d+$/.test(trimmed)) return null;
  return Math.min(TOOL_CALLS_MAX, Math.max(TOOL_CALLS_MIN, Number(trimmed)));
}

export default class ProfileStore {
  style: ReplyStyle = "standard";
  custom = "";
  customDraft = "";
  about = "";
  aboutDraft = "";
  toolCalls = TOOL_CALLS_DEFAULT;
  toolCallsDraft = String(TOOL_CALLS_DEFAULT);
  errors: Partial<Record<ProfileField, string>> = {};
  pending: Partial<Record<ProfileField, boolean>> = {};
  loaded = false;

  constructor(private readonly ipc: IpcClient<Contract>) {
    makeAutoObservable<ProfileStore, "ipc">(this, { ipc: false }, { autoBind: true });
  }

  get customDirty(): boolean {
    return this.customDraft.trim() !== this.custom;
  }

  get aboutDirty(): boolean {
    return this.aboutDraft.trim() !== this.about;
  }

  private async read(key: string): Promise<Json | undefined> {
    try {
      return (await this.ipc.call("settings.get", { key })).value;
    } catch {
      return undefined;
    }
  }

  async load() {
    const [style, custom, about, toolCalls] = await Promise.all([
      this.read(PROFILE_KEYS.style),
      this.read(PROFILE_KEYS.custom),
      this.read(PROFILE_KEYS.about),
      this.read(PROFILE_KEYS.toolCalls),
    ]);
    runInAction(() => {
      if (isReplyStyle(style) && this.pending.style === undefined) this.style = style;
      if (typeof custom === "string" && !this.customDirty) {
        this.custom = custom.trim();
        this.customDraft = this.custom;
      }
      if (typeof about === "string" && !this.aboutDirty) {
        this.about = about.trim();
        this.aboutDraft = this.about;
      }
      if (
        typeof toolCalls === "number" &&
        Number.isInteger(toolCalls) &&
        toolCalls >= TOOL_CALLS_MIN &&
        toolCalls <= TOOL_CALLS_MAX &&
        this.pending.toolCalls === undefined
      ) {
        this.toolCalls = toolCalls;
        this.toolCallsDraft = String(toolCalls);
      }
      this.loaded = true;
    });
  }

  private async save(field: ProfileField, value: Json): Promise<boolean> {
    this.pending[field] = true;
    delete this.errors[field];
    try {
      await this.ipc.call("settings.set", { key: PROFILE_KEYS[field], value });
      return true;
    } catch {
      runInAction(() => {
        this.errors[field] = SETTING_SAVE_FAILED;
      });
      return false;
    } finally {
      runInAction(() => {
        delete this.pending[field];
      });
    }
  }

  async setStyle(style: ReplyStyle) {
    if (this.style === style || this.pending.style !== undefined) return;
    const previous = this.style;
    this.style = style;
    if (!(await this.save("style", style)))
      runInAction(() => {
        this.style = previous;
      });
  }

  setCustomDraft(value: string) {
    this.customDraft = value;
  }

  resetCustom() {
    this.customDraft = this.custom;
    delete this.errors.custom;
  }

  async saveCustom() {
    if (!this.customDirty || this.pending.custom !== undefined) return;
    const next = this.customDraft.trim();
    if (await this.save("custom", next))
      runInAction(() => {
        this.custom = next;
        this.customDraft = next;
      });
  }

  setAboutDraft(value: string) {
    this.aboutDraft = value;
  }

  resetAbout() {
    this.aboutDraft = this.about;
    delete this.errors.about;
  }

  async saveAbout() {
    if (!this.aboutDirty || this.pending.about !== undefined) return;
    const next = this.aboutDraft.trim();
    if (await this.save("about", next))
      runInAction(() => {
        this.about = next;
        this.aboutDraft = next;
      });
  }

  setToolCallsDraft(value: string) {
    this.toolCallsDraft = value;
  }

  async commitToolCalls() {
    const next = parseToolCalls(this.toolCallsDraft);
    if (next === null || next === this.toolCalls || this.pending.toolCalls !== undefined) {
      this.toolCallsDraft = String(this.toolCalls);
      return;
    }
    const previous = this.toolCalls;
    this.toolCalls = next;
    this.toolCallsDraft = String(next);
    if (!(await this.save("toolCalls", next)))
      runInAction(() => {
        this.toolCalls = previous;
        this.toolCallsDraft = String(previous);
      });
  }
}
