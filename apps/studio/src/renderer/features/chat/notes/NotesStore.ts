import { makeAutoObservable, runInAction } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import {
  isNoteName,
  type Contract,
  type ConversationId,
  type NoteDto,
  type NoteId,
  type NoteSummaryDto,
} from "@zvs/shared";
import {
  NOTE_REMOVED,
  NOTE_RENAME_INVALID,
  NOTES_LOAD_ERROR,
  isConflict,
  isMissing,
  noteActionErrorCopy,
  noteOpenErrorCopy,
  noteSaveErrorCopy,
} from "./notesCopy";

export type NoteMode = "view" | "edit" | "rename";

const VIEW_WIDTH_MIN = 360;
const VIEW_WIDTH_DEFAULT = 560;

export default class NotesStore {
  conversationId: ConversationId | null = null;
  notes: NoteSummaryDto[] = [];
  loading = false;
  loaded = false;
  error: string | null = null;
  openName: string | null = null;
  note: NoteDto | null = null;
  noteLoading = false;
  noteError: string | null = null;
  removed = false;
  mode: NoteMode = "view";
  draft = "";
  renameDraft = "";
  busy = false;
  actionError: string | null = null;
  conflict = false;
  removing: string | null = null;
  viewWidth = VIEW_WIDTH_DEFAULT;
  private generation = 0;
  private noteGeneration = 0;

  constructor(private readonly ipc: IpcClient<Contract>) {
    makeAutoObservable<NotesStore, "ipc" | "generation" | "noteGeneration">(
      this,
      { ipc: false, generation: false, noteGeneration: false },
      { autoBind: true },
    );
  }

  get viewOpen(): boolean {
    return this.openName !== null || this.removed;
  }

  get dirty(): boolean {
    return this.mode === "edit" && this.note !== null && this.draft !== this.note.content;
  }

  get renameValid(): boolean {
    return isNoteName(this.renameDraft.trim());
  }

  get renameHint(): string | null {
    return this.renameDraft.trim().length > 0 && !this.renameValid ? NOTE_RENAME_INVALID : null;
  }

  async show(conversationId: ConversationId) {
    if (this.conversationId === conversationId && (this.loaded || this.loading)) return;
    this.reset();
    this.conversationId = conversationId;
    await this.refresh();
  }

  hide() {
    this.reset();
  }

  async refresh() {
    const conversationId = this.conversationId;
    if (conversationId === null) return;
    const generation = this.generation;
    this.loading = true;
    try {
      const notes = await this.ipc.call("notes.list", { id: conversationId });
      runInAction(() => {
        if (generation !== this.generation) return;
        this.notes = notes;
        this.loaded = true;
        this.error = null;
      });
    } catch {
      runInAction(() => {
        if (generation === this.generation) this.error = NOTES_LOAD_ERROR;
      });
    } finally {
      runInAction(() => {
        if (generation === this.generation) this.loading = false;
      });
    }
  }

  async open(name: string) {
    const conversationId = this.conversationId;
    if (conversationId === null) return;
    await this.load(conversationId, { name });
  }

  async openById(conversationId: ConversationId, id: NoteId) {
    if (this.conversationId !== conversationId) {
      this.reset();
      this.conversationId = conversationId;
      void this.refresh();
    }
    await this.load(conversationId, { id });
  }

  close() {
    this.noteGeneration++;
    this.openName = null;
    this.note = null;
    this.noteLoading = false;
    this.noteError = null;
    this.removed = false;
    this.leaveEditing();
  }

  edit() {
    if (this.note === null || this.busy) return;
    this.mode = "edit";
    this.draft = this.note.content;
    this.actionError = null;
    this.conflict = false;
  }

  setDraft(text: string) {
    this.draft = text;
  }

  startRename() {
    if (this.note === null || this.busy) return;
    this.mode = "rename";
    this.renameDraft = this.note.name;
    this.actionError = null;
  }

  setRenameDraft(text: string) {
    this.renameDraft = text;
  }

  cancel() {
    this.leaveEditing();
  }

  async save(): Promise<boolean> {
    const conversationId = this.conversationId;
    const note = this.note;
    if (conversationId === null || note === null || this.busy || this.mode !== "edit") return false;
    this.busy = true;
    this.actionError = null;
    try {
      const saved = await this.ipc.call("notes.save", {
        conversationId,
        name: note.name,
        content: this.draft,
        expectedHash: note.hash,
      });
      runInAction(() => {
        this.note = saved;
        this.openName = saved.name;
        this.leaveEditing();
      });
      void this.refresh();
      return true;
    } catch (error) {
      runInAction(() => this.fail(error, noteSaveErrorCopy(error)));
      return false;
    } finally {
      runInAction(() => {
        this.busy = false;
      });
    }
  }

  async reload() {
    const name = this.openName;
    if (name === null) return;
    this.leaveEditing();
    await this.open(name);
  }

  async rename(): Promise<boolean> {
    const conversationId = this.conversationId;
    const note = this.note;
    const to = this.renameDraft.trim();
    if (conversationId === null || note === null || this.busy || !isNoteName(to)) return false;
    if (to === note.name) {
      this.leaveEditing();
      return true;
    }
    this.busy = true;
    this.actionError = null;
    try {
      const renamed = await this.ipc.call("notes.rename", {
        conversationId,
        name: note.name,
        to,
      });
      runInAction(() => {
        this.note = renamed;
        this.openName = renamed.name;
        this.leaveEditing();
      });
      void this.refresh();
      return true;
    } catch (error) {
      runInAction(() => this.fail(error, noteActionErrorCopy(error)));
      return false;
    } finally {
      runInAction(() => {
        this.busy = false;
      });
    }
  }

  requestRemove(name: string) {
    this.removing = name;
    this.actionError = null;
  }

  cancelRemove() {
    this.removing = null;
  }

  async confirmRemove(): Promise<boolean> {
    const conversationId = this.conversationId;
    const name = this.removing;
    if (conversationId === null || name === null || this.busy) return false;
    this.busy = true;
    try {
      await this.ipc.call("notes.remove", { conversationId, name });
      runInAction(() => {
        this.removing = null;
        if (this.openName === name) this.close();
        this.notes = this.notes.filter((note) => note.name !== name);
      });
      void this.refresh();
      return true;
    } catch (error) {
      runInAction(() => {
        this.removing = null;
        this.fail(error, noteActionErrorCopy(error));
      });
      return false;
    } finally {
      runInAction(() => {
        this.busy = false;
      });
    }
  }

  setViewWidth(width: number) {
    this.viewWidth = Math.max(VIEW_WIDTH_MIN, Math.round(width));
  }

  dispose() {
    this.reset();
  }

  private async load(
    conversationId: ConversationId,
    target: { readonly name: string } | { readonly id: NoteId },
  ) {
    const generation = ++this.noteGeneration;
    this.leaveEditing();
    this.openName = "name" in target ? target.name : null;
    this.removed = false;
    this.noteError = null;
    this.noteLoading = true;
    try {
      const note = await this.ipc.call("notes.read", { conversationId, ...target });
      runInAction(() => {
        if (generation !== this.noteGeneration) return;
        this.note = note;
        this.openName = note.name;
      });
    } catch (error) {
      runInAction(() => {
        if (generation !== this.noteGeneration) return;
        this.note = null;
        if (isMissing(error)) {
          this.removed = true;
          this.openName = null;
        } else {
          this.noteError = noteOpenErrorCopy(error);
        }
      });
    } finally {
      runInAction(() => {
        if (generation === this.noteGeneration) this.noteLoading = false;
      });
    }
  }

  private fail(error: unknown, message: string) {
    if (isMissing(error)) {
      this.leaveEditing();
      this.note = null;
      this.openName = null;
      this.removed = true;
      this.actionError = NOTE_REMOVED;
      void this.refresh();
      return;
    }
    this.conflict = isConflict(error) && this.mode === "edit";
    this.actionError = message;
  }

  private leaveEditing() {
    this.mode = "view";
    this.draft = "";
    this.renameDraft = "";
    this.actionError = null;
    this.conflict = false;
  }

  private reset() {
    this.generation++;
    this.close();
    this.conversationId = null;
    this.notes = [];
    this.loading = false;
    this.loaded = false;
    this.error = null;
    this.removing = null;
  }
}
