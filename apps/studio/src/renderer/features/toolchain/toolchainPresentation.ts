import { mdiFileDocumentMultipleOutline, mdiLanguagePython, mdiNodejs } from "@mdi/js";
import {
  AppErrorCode,
  ToolchainRuntime,
  type ToolchainDto,
  type ToolchainSource,
  type ToolchainStatus,
} from "@zvs/shared";
import { isIpcError } from "@zvs/ipc";
import type { StatusTone } from "../../ui/atoms/statusTone";

export type ToolchainAction = "install" | "update" | "scan" | "select" | "remove";

export const TOOLCHAIN_ORDER: readonly ToolchainRuntime[] = ["node", "uv", "libreoffice"];

export const RUNTIME_ICONS: Record<ToolchainRuntime, string> = {
  uv: mdiLanguagePython,
  node: mdiNodejs,
  libreoffice: mdiFileDocumentMultipleOutline,
};

export const RUNTIME_HINTS: Record<ToolchainRuntime, string> = {
  uv: "Нужен MCP-серверам, которые запускаются через uvx и uv.",
  node: "Нужен MCP-серверам, которые запускаются через npx и node.",
  libreoffice: "Преобразует документы DOCX, XLSX и PPTX в PDF для точного просмотра.",
};

export const STATUS_LABELS: Record<ToolchainStatus, string> = {
  missing: "Не подключено",
  installing: "Загрузка",
  ready: "Подключено",
  broken: "Недоступно",
};

export const STATUS_TONES: Record<ToolchainStatus, StatusTone> = {
  missing: "idle",
  installing: "accent",
  ready: "ok",
  broken: "err",
};

export const SOURCE_LABELS: Record<ToolchainSource, string> = {
  managed: "Загружено приложением",
  path: "Найдено в PATH",
  manual: "Указано вручную",
  system: "Установлено в системе",
};

export const ACTION_LABELS: Record<ToolchainAction, string> = {
  install: "Загрузить",
  update: "Обновить",
  scan: "Найти в PATH",
  select: "Указать файл",
  remove: "Удалить",
};

export const SCAN_SYSTEM_LABEL = "Найти в системе";
export const DISCONNECT_LABEL = "Отключить";
export const INSTALLING_HINT = "Ход загрузки отображается в разделе «Загрузки».";
export const UNKNOWN_TOOLCHAIN_ERROR = "Не удалось выполнить операцию. Попробуйте ещё раз.";

export const TOOLCHAIN_ERROR_COPY: Partial<Record<AppErrorCode, string>> = {
  [AppErrorCode.PROVIDER_UNREACHABLE]: "Сервер выпусков недоступен. Проверьте подключение к сети.",
  [AppErrorCode.PERMISSION_DENIED]: "Недостаточно прав для записи в каталог приложения.",
  [AppErrorCode.DB_ERROR]: "Сбой локальной базы данных.",
};

const HOST_WORDED = new Set<AppErrorCode>([
  AppErrorCode.CONFLICT,
  AppErrorCode.NOT_FOUND,
  AppErrorCode.VALIDATION_FAILED,
  AppErrorCode.UNSUPPORTED_FORMAT,
  AppErrorCode.RATE_LIMITED,
  AppErrorCode.NOT_IMPLEMENTED,
]);

export function toolchainErrorCopy(error: unknown): string {
  if (!isIpcError(error)) return UNKNOWN_TOOLCHAIN_ERROR;
  if (HOST_WORDED.has(error.code) && error.message.length > 0) return error.message;
  return TOOLCHAIN_ERROR_COPY[error.code] ?? UNKNOWN_TOOLCHAIN_ERROR;
}

export function toolchainRuntimeOf(ref: string): ToolchainRuntime | undefined {
  const parsed = ToolchainRuntime.safeParse(/^toolchain:([a-z]+)$/.exec(ref)?.[1]);
  return parsed.success ? parsed.data : undefined;
}

export function isToolchainRef(ref: string): boolean {
  return ref.startsWith("toolchain:");
}

export function removalLabel(dto: Pick<ToolchainDto, "source">): string {
  return dto.source === undefined || dto.source === "managed"
    ? ACTION_LABELS.remove
    : DISCONNECT_LABEL;
}

export function actionLabel(action: ToolchainAction, runtime: ToolchainRuntime): string {
  return action === "scan" && runtime === "libreoffice" ? SCAN_SYSTEM_LABEL : ACTION_LABELS[action];
}

export function removalCopy(displayName: string, runtime?: ToolchainRuntime): string {
  const removed = `${displayName} будет отключён, а копии, загруженные приложением, удалены с диска.`;
  return runtime === "libreoffice"
    ? `${removed} Документы DOCX, XLSX и PPTX, ещё не преобразованные в PDF, будут отображаться приближённо.`
    : `${removed} MCP-серверы, которые используют ${displayName}, не запустятся до подключения другой копии.`;
}

export function versionLine(dto: ToolchainDto): string {
  const minimum = `минимальная ${dto.minimumVersion}`;
  return dto.version === undefined ? minimum : `${dto.version} · ${minimum}`;
}

export function updateLine(dto: ToolchainDto): string | undefined {
  if (!dto.updateAvailable || dto.latestVersion === undefined) return undefined;
  return `Доступна версия ${dto.latestVersion}`;
}

export function availableActions(dto: ToolchainDto): readonly ToolchainAction[] {
  if (dto.status === "installing") return [];
  const actions: ToolchainAction[] = [];
  if (dto.status === "ready" && dto.updateAvailable) actions.push("update");
  if (dto.status !== "ready") actions.push("install");
  actions.push("scan", "select");
  if (dto.source !== undefined || dto.managedVersions.length > 0) actions.push("remove");
  return actions;
}
