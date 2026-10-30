import { makeAutoObservable, runInAction } from "mobx";
import { isIpcError, type IpcClient } from "@zvs/ipc";
import {
  AppErrorCode,
  type Contract,
  type ConversationExportFormat,
  type ConversationId,
} from "@zvs/shared";

export const EXPORT = "Экспорт";
export const EXPORT_JSON = "Экспорт в JSON";
export const EXPORT_MARKDOWN = "Экспорт в Markdown";
export const EXPORT_SAVED = "Диалог сохранён";
export const EXPORT_FAILED = "Не удалось экспортировать диалог.";
export const EXPORT_NO_ACCESS = "Недостаточно прав для записи в выбранную папку.";

export const EXPORT_FORMATS: readonly {
  readonly format: ConversationExportFormat;
  readonly label: string;
}[] = [
  { format: "json", label: EXPORT_JSON },
  { format: "markdown", label: EXPORT_MARKDOWN },
];

const HOST_WORDED = new Set<AppErrorCode>([AppErrorCode.NOT_FOUND, AppErrorCode.NOT_IMPLEMENTED]);

export function exportFailureCopy(error: unknown): string {
  if (!isIpcError(error)) return EXPORT_FAILED;
  if (error.code === AppErrorCode.PERMISSION_DENIED) return EXPORT_NO_ACCESS;
  if (HOST_WORDED.has(error.code) && error.message.length > 0) return error.message;
  return EXPORT_FAILED;
}

export interface ExportNotice {
  readonly tone: "info" | "error";
  readonly title: string;
  readonly detail?: string;
}

export default class ConversationExport {
  busy = false;
  notice: ExportNotice | null = null;

  constructor(private readonly ipc: IpcClient<Contract>) {
    makeAutoObservable<ConversationExport, "ipc">(this, { ipc: false }, { autoBind: true });
  }

  async export(id: ConversationId, format: ConversationExportFormat): Promise<void> {
    if (this.busy) return;
    this.busy = true;
    this.notice = null;
    try {
      const result = await this.ipc.call("chat.conversations.export", { id, format });
      runInAction(() => {
        if (result.path !== undefined) {
          this.notice = { tone: "info", title: EXPORT_SAVED, detail: result.path };
        }
      });
    } catch (error) {
      runInAction(() => {
        this.notice = { tone: "error", title: exportFailureCopy(error) };
      });
    } finally {
      runInAction(() => {
        this.busy = false;
      });
    }
  }

  dismiss(): void {
    this.notice = null;
  }
}
