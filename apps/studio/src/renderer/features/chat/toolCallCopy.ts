import {
  mdiAccountCheckOutline,
  mdiAlertCircleOutline,
  mdiBookOpenPageVariantOutline,
  mdiBrain,
  mdiCameraOutline,
  mdiCodeBraces,
  mdiCogPlayOutline,
  mdiCommentQuestionOutline,
  mdiConsoleLine,
  mdiCursorDefaultClickOutline,
  mdiDatabaseArrowDownOutline,
  mdiDatabaseOutline,
  mdiDatabaseSearchOutline,
  mdiDownloadOutline,
  mdiFileDocumentEditOutline,
  mdiFileDocumentMultipleOutline,
  mdiFileFind,
  mdiFileTreeOutline,
  mdiFolderOpenOutline,
  mdiFormTextbox,
  mdiMagnify,
  mdiPuzzleEditOutline,
  mdiPuzzleOutline,
  mdiScaleBalance,
  mdiScriptTextPlayOutline,
  mdiSendOutline,
  mdiShieldCheckOutline,
  mdiShieldKeyOutline,
  mdiTextBoxSearchOutline,
  mdiWeb,
  mdiWrenchOutline,
} from "@mdi/js";
import {
  EXEC_MODES,
  type ChatToolCallDto,
  type ExecMode,
  type ToolAuthorisationDto,
} from "@zvs/shared";
import { EXEC_MODE_COPY } from "../execPolicy/execPolicyCopy";

export type ToolCategory =
  "knowledge" | "files" | "process" | "skills" | "dialog" | "data" | "other";

export interface ToolPresentation {
  readonly title: string;
  readonly icon: string;
  readonly category: ToolCategory;
}

export const TOOL_PRESENTATION: Readonly<Record<string, ToolPresentation>> = {
  "knowledge.search": {
    title: "Поиск в хранилищах",
    icon: mdiDatabaseSearchOutline,
    category: "knowledge",
  },
  "knowledge.read": {
    title: "Чтение документа",
    icon: mdiBookOpenPageVariantOutline,
    category: "knowledge",
  },
  "knowledge.list": {
    title: "Подключённые хранилища",
    icon: mdiDatabaseOutline,
    category: "knowledge",
  },
  "vector.search": {
    title: "Семантический поиск",
    icon: mdiTextBoxSearchOutline,
    category: "knowledge",
  },
  "vector.list": { title: "Список хранилищ", icon: mdiDatabaseOutline, category: "knowledge" },
  "vector.index": {
    title: "Индексация хранилища",
    icon: mdiDatabaseArrowDownOutline,
    category: "data",
  },
  "fs.list": { title: "Просмотр папки", icon: mdiFolderOpenOutline, category: "files" },
  "fs.read": { title: "Чтение файлов", icon: mdiFileDocumentMultipleOutline, category: "files" },
  "fs.search": { title: "Поиск по файлам", icon: mdiFileFind, category: "files" },
  "fs.apply": { title: "Изменение файлов", icon: mdiFileDocumentEditOutline, category: "files" },
  "code.run": { title: "Выполнение программы", icon: mdiCodeBraces, category: "process" },
  "shell.run": { title: "Команда терминала", icon: mdiConsoleLine, category: "process" },
  "job.run": { title: "Фоновое задание", icon: mdiCogPlayOutline, category: "process" },
  "skill.load": { title: "Загрузка навыка", icon: mdiPuzzleOutline, category: "skills" },
  "skill.read": { title: "Файл навыка", icon: mdiPuzzleOutline, category: "skills" },
  "chat.file": { title: "Рабочая папка диалога", icon: mdiFolderOpenOutline, category: "files" },
  "skill.save": { title: "Сохранение навыка", icon: mdiPuzzleEditOutline, category: "skills" },
  "skill.script": { title: "Скрипт навыка", icon: mdiScriptTextPlayOutline, category: "skills" },
  "download.fetch": { title: "Загрузка файла", icon: mdiDownloadOutline, category: "data" },
  "chat.ask": { title: "Вопрос пользователю", icon: mdiCommentQuestionOutline, category: "dialog" },
  invalid_tool_call: {
    title: "Некорректный вызов инструмента",
    icon: mdiAlertCircleOutline,
    category: "other",
  },
  "chat.memory": { title: "Память", icon: mdiBrain, category: "dialog" },
  "browser.navigate": { title: "Переход по адресу", icon: mdiWeb, category: "other" },
  "browser.read_page": { title: "Чтение страницы", icon: mdiFileTreeOutline, category: "other" },
  "browser.screenshot": { title: "Снимок страницы", icon: mdiCameraOutline, category: "other" },
  "browser.find": { title: "Поиск на странице", icon: mdiMagnify, category: "other" },
  "browser.click": {
    title: "Нажатие на элемент",
    icon: mdiCursorDefaultClickOutline,
    category: "other",
  },
  "browser.type": { title: "Ввод текста", icon: mdiFormTextbox, category: "other" },
  "browser.submit": { title: "Отправка формы", icon: mdiSendOutline, category: "other" },
  "browser.download": { title: "Загрузка файла", icon: mdiDownloadOutline, category: "other" },
};

const OTHER: Omit<ToolPresentation, "title"> = { icon: mdiWrenchOutline, category: "other" };

export function toolPresentation(key: string): ToolPresentation {
  return TOOL_PRESENTATION[key] ?? { ...OTHER, title: key };
}

export function toolTitle(key: string): string {
  return toolPresentation(key).title;
}

export function isBuiltinTool(key: string): boolean {
  return key in TOOL_PRESENTATION;
}

export const CATEGORY_BADGE: Record<ToolCategory, string> = {
  knowledge:
    "bg-accent-soft text-accent-medium fx-on:shadow-[inset_0_0_0_1px_var(--color-accent-line)]",
  files: "bg-info-soft text-info fx-on:shadow-[inset_0_0_0_1px_var(--color-info-line)]",
  process: "bg-main-100 text-main-900",
  skills: "bg-ok-soft text-ok fx-on:shadow-[inset_0_0_0_1px_var(--color-ok-line)]",
  dialog: "bg-warn-soft text-warn fx-on:shadow-[inset_0_0_0_1px_var(--color-warn-line)]",
  data: "bg-main-700 text-main-200 fx-on:shadow-[inset_0_0_0_1px_var(--color-line-strong)]",
  other: "bg-main-700 text-main-300",
};

export const CATEGORY_RAIL: Record<ToolCategory, string> = {
  knowledge: "bg-accent-dark",
  files: "bg-info",
  process: "bg-main-300",
  skills: "bg-ok",
  dialog: "bg-warn",
  data: "bg-main-400",
  other: "bg-main-500",
};

export const TOOL_CALLS_LABEL = "Вызовы инструментов";
export const TOOL_KEY_LABEL = "Инструмент";
export const ARGUMENTS_TITLE = "Аргументы";
export const RAW_ARGUMENTS = "Аргументы JSON";
export const RESULT_TITLE = "Результат";
export const RESULT_SHORTENED = "сокращён";
export const COPY_RESULT = "Копировать результат";
export const RESULT_COPIED = "Результат скопирован";
export const FOUND_TITLE = "Найденные фрагменты";
export const NOTHING_FOUND = "Подходящих фрагментов нет.";
export const QUERY_TITLE = "Запрос";
export const FOLDER_FIELD = "Папка";
export const WORKING_FOLDER = "рабочая папка";
export const RECURSIVE_FLAG = "с вложенными папками";
export const REGEX_FLAG = "регулярное выражение";
export const IGNORE_CASE_FLAG = "без учёта регистра";
export const FILTER_FIELD = "Фильтр";
export const LIMIT_FIELD = "Не более";
export const STORE_FIELD = "Хранилище";
export const ALL_STORES = "все подключённые";
export const DOCUMENT_FIELD = "Документ";
export const RANGE_FIELD = "Фрагменты";
export const SECTION_FIELD = "Раздел";
export const FILES_TITLE = "Файлы";
export const WHOLE_FILE = "целиком";
export const SKILL_FIELD = "Навык";
export const STDIN_FIELD = "Стандартный ввод";
export const APPROVAL_PROMPT = "Модель запрашивает вызов";
export const APPROVAL_QUESTION = "Разрешить?";
export const ALLOW_ONCE = "Разрешить один раз";
export const FORBID = "Запретить";

export function lineRange(start: number | null, end: number | null): string {
  if (start === null && end === null) return WHOLE_FILE;
  if (end === null) return `строки ${String(start)}–конец`;
  return `строки ${String(start ?? 1)}–${String(end)}`;
}

export function scorePercent(score: number): string {
  return `${Math.round(score * 100).toLocaleString("ru-RU")}%`;
}

export function runningPhase(call: Pick<ChatToolCallDto, "key">): string {
  return toolTitle(call.key);
}

export const AUTHORISATION_TITLE = "Основание запуска";

const AUTHORISATION_LABEL: Record<ToolAuthorisationDto["kind"], string> = {
  auto: "Разрешено уровнем инструмента",
  granted: "Разрешено выданным разрешением",
  approved: "Подтверждено пользователем",
  policy: "Разрешено режимом подтверждения",
  judge: "Разрешено автопроверкой",
};

const AUTHORISATION_ICON: Record<ToolAuthorisationDto["kind"], string> = {
  auto: mdiShieldCheckOutline,
  granted: mdiShieldKeyOutline,
  approved: mdiAccountCheckOutline,
  policy: mdiShieldCheckOutline,
  judge: mdiScaleBalance,
};

function isExecMode(value: string): value is ExecMode {
  return (EXEC_MODES as readonly string[]).includes(value);
}

export interface AuthorisationView {
  readonly label: string;
  readonly icon: string;
  readonly source: string | null;
  readonly reason: string | null;
}

export function authorisationView(authorisation: ToolAuthorisationDto): AuthorisationView {
  const source = authorisation.source ?? null;
  const mode = authorisation.kind === "policy" && source !== null && isExecMode(source);
  return {
    label: mode
      ? `${AUTHORISATION_LABEL.policy} «${EXEC_MODE_COPY[source].label}»`
      : AUTHORISATION_LABEL[authorisation.kind],
    icon: AUTHORISATION_ICON[authorisation.kind],
    source: mode ? null : source,
    reason: authorisation.reason ?? null,
  };
}
