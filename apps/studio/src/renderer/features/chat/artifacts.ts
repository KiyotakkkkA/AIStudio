import { ArtifactDto, AppErrorCode, type ChatToolCallDto } from "@zvs/shared";
import { isIpcError } from "@zvs/ipc";
import type { StatusTone } from "../../ui/atoms/statusTone";
import { formatBytes } from "../downloads/downloadPresentation";

const ToolArtifacts = ArtifactDto.array();

export const ARTIFACTS_LABEL = "Созданные файлы";
export const DOWNLOAD_ARTIFACT = "Скачать";
export const REVEAL_ARTIFACT = "Перейти";
export const OPEN_ARTIFACT = "Открыть";
export const OPEN_IN_VIEWER = "Открыть в просмотрщике";
export const OPEN_IN_DEFAULT_APP = "Открыть в приложении по умолчанию";
export const VIEWER_UNAVAILABLE = "Формат не поддерживается просмотрщиком";
export const OPEN_IN_BUILT_IN_BROWSER = "Открыть во встроенном браузере";
export const DASHBOARD_KIND = "Дашборд";

const PAGE_FORMATS = new Set(["html", "htm", "svg", "pdf", "png", "jpg", "jpeg"]);
const DASHBOARDS_PREFIX = "dashboards/";

export function opensAsPage(format: string): boolean {
  return PAGE_FORMATS.has(format.toLowerCase());
}

export function isDashboard(artifact: ArtifactDto): boolean {
  return artifact.id.startsWith(DASHBOARDS_PREFIX);
}

export function stepArtifacts(outputs: readonly unknown[]): ArtifactDto[] {
  const byId = new Map<string, ArtifactDto>();
  for (const output of outputs) {
    const listed =
      typeof output === "object" && output !== null && !Array.isArray(output)
        ? (output as Record<string, unknown>).artifacts
        : null;
    const parsed = ToolArtifacts.safeParse(listed);
    if (!parsed.success) continue;
    for (const artifact of parsed.data) {
      byId.delete(artifact.id);
      byId.set(artifact.id, artifact);
    }
  }
  return [...byId.values()];
}

export function openInApplication(name: string | null): string {
  return name === null ? OPEN_IN_DEFAULT_APP : `Открыть в ${name}`;
}
export const ARTIFACT_SAVED = "Файл сохранён";
export const UNKNOWN_ARTIFACT_ERROR = "Не удалось выполнить действие с файлом. Попробуйте ещё раз.";

export const ARTIFACT_ERROR_COPY: Partial<Record<AppErrorCode, string>> = {
  [AppErrorCode.PERMISSION_DENIED]: "Недостаточно прав для записи в выбранную папку.",
};

const HOST_WORDED = new Set<AppErrorCode>([
  AppErrorCode.NOT_FOUND,
  AppErrorCode.NOT_IMPLEMENTED,
  AppErrorCode.VALIDATION_FAILED,
]);

export function artifactErrorCopy(error: unknown): string {
  if (!isIpcError(error)) return UNKNOWN_ARTIFACT_ERROR;
  if (HOST_WORDED.has(error.code) && error.message.length > 0) return error.message;
  return ARTIFACT_ERROR_COPY[error.code] ?? UNKNOWN_ARTIFACT_ERROR;
}

export interface ArtifactKind {
  readonly label: string;
  readonly tone: StatusTone;
}

const KINDS: Record<string, ArtifactKind> = {
  pdf: { label: "Документ PDF", tone: "err" },
  docx: { label: "Документ Word", tone: "accent" },
  doc: { label: "Документ Word", tone: "accent" },
  odt: { label: "Текстовый документ", tone: "accent" },
  rtf: { label: "Текстовый документ", tone: "accent" },
  xlsx: { label: "Таблица Excel", tone: "ok" },
  xlsm: { label: "Таблица Excel", tone: "ok" },
  xls: { label: "Таблица Excel", tone: "ok" },
  ods: { label: "Таблица", tone: "ok" },
  csv: { label: "Таблица CSV", tone: "ok" },
  pptx: { label: "Презентация PowerPoint", tone: "warn" },
  ppt: { label: "Презентация PowerPoint", tone: "warn" },
  odp: { label: "Презентация", tone: "warn" },
  md: { label: "Документ Markdown", tone: "idle" },
  html: { label: "Страница HTML", tone: "idle" },
  png: { label: "Изображение", tone: "idle" },
  jpg: { label: "Изображение", tone: "idle" },
  jpeg: { label: "Изображение", tone: "idle" },
  svg: { label: "Изображение", tone: "idle" },
  zip: { label: "Архив", tone: "idle" },
};

export function artifactKind(format: string): ArtifactKind {
  return KINDS[format.toLowerCase()] ?? { label: "Файл", tone: "idle" };
}

export function artifactSubtitle(artifact: ArtifactDto): string {
  return [
    isDashboard(artifact) ? DASHBOARD_KIND : artifactKind(artifact.format).label,
    artifact.format.toUpperCase(),
    formatBytes(artifact.sizeBytes),
  ].join(" · ");
}

export function answerArtifacts(calls: readonly ChatToolCallDto[]): ArtifactDto[] {
  const byId = new Map<string, ArtifactDto>();
  for (const call of calls) {
    if (call.status !== "succeeded") continue;
    const meta = call.meta;
    const listed =
      typeof meta === "object" && meta !== null && !Array.isArray(meta) ? meta.artifacts : null;
    const parsed = ToolArtifacts.safeParse(listed);
    if (!parsed.success) continue;
    for (const artifact of parsed.data) {
      byId.delete(artifact.id);
      byId.set(artifact.id, artifact);
    }
  }
  return [...byId.values()];
}
