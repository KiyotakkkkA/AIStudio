import { makeAutoObservable, observableRef, runInAction } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import {
  TransferUploadInput,
  type Contract,
  type TransferCategory,
  type TransferPreviewDto,
} from "@zvs/shared";
import { DATA_CATEGORIES, DATA_INVALID_FILE, transferErrorCopy } from "./dataTransferCopy";

const CHUNK_BYTES = 512 * 1024;

function encode(bytes: Uint8Array): string {
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary);
}

export default class DataTransferStore {
  preview: TransferPreviewDto | null = null;
  file: File | null = null;
  selected = new Set<TransferCategory>(DATA_CATEGORIES);
  loading = false;
  uploading = false;
  working = false;
  uploadedBytes = 0;
  error: string | null = null;
  savedPath: string | null = null;
  imported = false;
  private session: string | null = null;
  private generation = 0;

  constructor(
    private readonly ipc: IpcClient<Contract>,
    private readonly onImported: () => Promise<void>,
  ) {
    makeAutoObservable<DataTransferStore, "ipc" | "onImported">(
      this,
      {
        ipc: false,
        onImported: false,
        file: observableRef,
        preview: observableRef,
      },
      { autoBind: true },
    );
  }

  get busy() {
    return this.loading || this.uploading || this.working;
  }
  get categories() {
    return DATA_CATEGORIES.filter((category) =>
      this.preview?.categories.some((item) => item.category === category),
    );
  }
  get selection() {
    return this.categories.filter(
      (category) => this.selected.has(category) && !this.alreadyImported(category),
    );
  }
  get hasDependencies() {
    return this.selection.some((category) =>
      this.dependencies(category).some((required) => !this.alreadyImported(required)),
    );
  }
  get selectedBytes() {
    return (
      this.preview?.categories.reduce(
        (sum, item) => sum + (this.selection.includes(item.category) ? item.bytes : 0),
        0,
      ) ?? 0
    );
  }
  get selectedCount() {
    return (
      this.preview?.categories.reduce(
        (sum, item) => sum + (this.selection.includes(item.category) ? item.count : 0),
        0,
      ) ?? 0
    );
  }
  get canSubmit() {
    return (
      !this.busy &&
      !this.imported &&
      this.preview !== null &&
      this.selection.length > 0 &&
      this.selectedCount > 0 &&
      this.selection.every((category) =>
        this.dependencies(category).every(
          (required) => this.selected.has(required) || this.alreadyImported(required),
        ),
      ) &&
      !this.preview.blockedCategories?.some((category) => this.selected.has(category))
    );
  }
  get progress() {
    return this.file === null || this.file.size === 0
      ? 0
      : Math.round((100 * this.uploadedBytes) / this.file.size);
  }

  alreadyImported(category: TransferCategory): boolean {
    return this.preview?.importedCategories?.includes(category) ?? false;
  }

  private dependencies(category: TransferCategory): readonly TransferCategory[] {
    return (
      this.preview?.categories.find((item) => item.category === category)?.requires ??
      (category === "conversations" || category === "stores" ? ["connections"] : [])
    );
  }

  toggle(category: TransferCategory, checked: boolean) {
    if (this.busy || this.imported || this.alreadyImported(category)) return;
    if (checked) {
      const pending = [category];
      while (pending.length > 0) {
        const item = pending.pop();
        if (item === undefined || this.alreadyImported(item)) continue;
        this.selected.add(item);
        for (const required of this.dependencies(item))
          if (!this.selected.has(required) && !this.alreadyImported(required))
            pending.push(required);
      }
    } else {
      this.selected.delete(category);
      let changed = true;
      while (changed) {
        changed = false;
        for (const item of this.selected) {
          if (
            this.dependencies(item).some(
              (required) => !this.selected.has(required) && !this.alreadyImported(required),
            )
          ) {
            this.selected.delete(item);
            changed = true;
          }
        }
      }
    }
    this.savedPath = null;
    this.error = null;
  }

  async load() {
    if (this.busy) return;
    const generation = ++this.generation;
    this.loading = true;
    this.error = null;
    this.savedPath = null;
    try {
      const preview = await this.ipc.call("dataTransfer.inventory", undefined);
      runInAction(() => {
        if (generation === this.generation) {
          this.preview = preview;
          this.selected = new Set(
            preview.categories
              .filter((item) => item.count > 0 && this.selected.has(item.category))
              .map((item) => item.category),
          );
        }
      });
    } catch (error) {
      runInAction(() => {
        if (generation === this.generation) this.error = transferErrorCopy(error);
      });
    } finally {
      runInAction(() => {
        if (generation === this.generation) this.loading = false;
      });
    }
  }

  private async release(id: string | null) {
    if (id === null) return;
    try {
      await this.ipc.call("dataTransfer.disposeImport", { id });
    } catch {
      return;
    }
  }

  async choose(file: File | null) {
    if (this.working) return;
    const generation = ++this.generation;
    const previous = this.session;
    this.session = null;
    this.file = file;
    this.preview = null;
    this.error = null;
    this.imported = false;
    this.uploadedBytes = 0;
    this.uploading = file !== null;
    await this.release(previous);
    if (file === null || generation !== this.generation) return;
    if (!TransferUploadInput.safeParse({ name: file.name, bytes: file.size }).success) {
      runInAction(() => {
        this.uploading = false;
        this.error = DATA_INVALID_FILE;
      });
      return;
    }
    let id: string | null = null;
    try {
      const session = await this.ipc.call("dataTransfer.beginImport", {
        name: file.name,
        bytes: file.size,
      });
      id = session.id;
      if (generation !== this.generation) {
        await this.release(id);
        return;
      }
      runInAction(() => {
        this.session = session.id;
      });
      for (let offset = 0; offset < file.size; offset += CHUNK_BYTES) {
        const bytes = new Uint8Array(await file.slice(offset, offset + CHUNK_BYTES).arrayBuffer());
        if (generation !== this.generation) {
          await this.release(id);
          return;
        }
        await this.ipc.call("dataTransfer.appendImport", { id, offset, data: encode(bytes) });
        runInAction(() => {
          if (generation === this.generation) this.uploadedBytes = offset + bytes.length;
        });
      }
      const preview = await this.ipc.call("dataTransfer.inspectImport", { id });
      runInAction(() => {
        if (generation !== this.generation) return;
        this.preview = preview;
        this.selected = new Set(
          preview.categories.filter((item) => item.count > 0).map((item) => item.category),
        );
      });
    } catch (error) {
      await this.release(id);
      runInAction(() => {
        if (generation !== this.generation) return;
        this.session = null;
        this.error = transferErrorCopy(error);
      });
    } finally {
      runInAction(() => {
        if (generation === this.generation) this.uploading = false;
      });
    }
  }

  async export() {
    if (!this.canSubmit) return;
    this.working = true;
    this.error = null;
    this.savedPath = null;
    try {
      const result = await this.ipc.call("dataTransfer.export", { categories: this.selection });
      runInAction(() => {
        this.savedPath = result?.path ?? null;
      });
    } catch (error) {
      runInAction(() => {
        this.error = transferErrorCopy(error);
      });
    } finally {
      runInAction(() => {
        this.working = false;
      });
    }
  }

  async import() {
    if (!this.canSubmit || this.session === null) return;
    this.working = true;
    this.error = null;
    try {
      await this.ipc.call("dataTransfer.import", { id: this.session, categories: this.selection });
      runInAction(() => {
        this.imported = true;
        this.session = null;
      });
      await this.onImported();
    } catch (error) {
      runInAction(() => {
        if (!this.imported) this.error = transferErrorCopy(error);
      });
    } finally {
      runInAction(() => {
        this.working = false;
      });
    }
  }

  dispose() {
    this.generation += 1;
    this.loading = false;
    this.uploading = false;
    const id = this.session;
    this.session = null;
    this.preview = null;
    this.file = null;
    this.savedPath = null;
    this.imported = false;
    this.error = null;
    void this.release(id);
  }
}
