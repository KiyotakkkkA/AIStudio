import {
  mdiApplicationOutline,
  mdiDockWindow,
  mdiMagnifyPlusOutline,
  mdiMonitorScreenshot,
  mdiCheckCircleOutline,
  mdiFlagCheckered,
  mdiFormatListChecks,
  mdiAlertCircleOutline,
  mdiBookOpenPageVariantOutline,
  mdiBrain,
  mdiCameraOutline,
  mdiCogPlayOutline,
  mdiStopCircleOutline,
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
  mdiRobotOutline,
  mdiScriptTextPlayOutline,
  mdiSendOutline,
  mdiShieldKeyOutline,
  mdiTextBoxSearchOutline,
  mdiWeb,
  mdiWrenchOutline,
} from "@mdi/js";
import {
  GOAL_PLAN_TOOL,
  GOAL_PLAN_TOOL_TITLE,
  GOAL_STEP_TOOL,
  GOAL_STEP_TOOL_TITLE,
  GOAL_TOOL,
  GOAL_TOOL_TITLE,
} from "./goalCopy";

const SUBAGENT_TOOL_TITLE = "Субагент";

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
  "shell.run": {
    title: "Выполнить команду в терминале",
    icon: mdiConsoleLine,
    category: "process",
  },
  "shell.list": {
    title: "Просмотр запущенных фоновых процессов",
    icon: mdiConsoleLine,
    category: "process",
  },
  "shell.output": {
    title: "Вывод фонового процесса",
    icon: mdiTextBoxSearchOutline,
    category: "process",
  },
  "shell.kill": {
    title: "Остановка фонового процесса",
    icon: mdiStopCircleOutline,
    category: "process",
  },
  "job.run": { title: "Фоновое задание", icon: mdiCogPlayOutline, category: "process" },
  "chat.file": { title: "Рабочая папка диалога", icon: mdiFolderOpenOutline, category: "files" },
  "skill.load": { title: "Загрузка навыка", icon: mdiPuzzleOutline, category: "skills" },
  "skill.read": { title: "Файл навыка", icon: mdiPuzzleOutline, category: "skills" },
  "skill.save": { title: "Сохранение навыка", icon: mdiPuzzleEditOutline, category: "skills" },
  "skill.script": { title: "Скрипт навыка", icon: mdiScriptTextPlayOutline, category: "skills" },
  "download.fetch": { title: "Загрузка файла", icon: mdiDownloadOutline, category: "data" },
  "chat.ask": { title: "Вопрос пользователю", icon: mdiCommentQuestionOutline, category: "dialog" },
  "chat.memory": { title: "Память", icon: mdiBrain, category: "dialog" },
  [GOAL_TOOL]: { title: GOAL_TOOL_TITLE, icon: mdiFlagCheckered, category: "dialog" },
  "agent.delegate": { title: SUBAGENT_TOOL_TITLE, icon: mdiRobotOutline, category: "dialog" },
  [GOAL_PLAN_TOOL]: { title: GOAL_PLAN_TOOL_TITLE, icon: mdiFormatListChecks, category: "dialog" },
  [GOAL_STEP_TOOL]: {
    title: GOAL_STEP_TOOL_TITLE,
    icon: mdiCheckCircleOutline,
    category: "dialog",
  },
  invalid_tool_call: {
    title: "Некорректный вызов инструмента",
    icon: mdiAlertCircleOutline,
    category: "other",
  },
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
  "computer.screenshot": { title: "Снимок экрана", icon: mdiMonitorScreenshot, category: "other" },
  "computer.act": {
    title: "Действия мышью и клавиатурой",
    icon: mdiCursorDefaultClickOutline,
    category: "other",
  },
  "computer.zoom": {
    title: "Увеличение области экрана",
    icon: mdiMagnifyPlusOutline,
    category: "other",
  },
  "computer.inspect": { title: "Элементы окна", icon: mdiFileTreeOutline, category: "other" },
  "computer.apps": { title: "Список окон", icon: mdiDockWindow, category: "other" },
  "computer.open_app": {
    title: "Запуск приложения",
    icon: mdiApplicationOutline,
    category: "other",
  },
  "computer.request_access": {
    title: "Доступ к приложениям",
    icon: mdiShieldKeyOutline,
    category: "other",
  },
};

const OTHER: Omit<ToolPresentation, "title"> = { icon: mdiWrenchOutline, category: "other" };

export function toolPresentation(key: string): ToolPresentation {
  return TOOL_PRESENTATION[key] ?? { ...OTHER, title: key };
}
