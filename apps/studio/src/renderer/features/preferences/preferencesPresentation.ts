import { APP_CHANNEL, APP_VERSION, CLI_CHANNEL, CLI_VERSION } from "../../app/appEnv";
import type { PaletteMode } from "../../app/palettes";
import {
  DEFAULT_FONT_SIZE,
  FONT_SIZES,
  type FontFamily,
  type FontSize,
  type FxLevel,
  type FxPreset,
  type PreferenceSection,
  type ThemeMode,
} from "../../stores/UiStore";
import { VOICE_INPUT } from "../voice/voiceCopy";
import type { AppSettingKey } from "./AppSettingsStore";

export const APP_NAME = "ZVS AI Studio";
export const APP_VERSION_LABEL = `${APP_CHANNEL} v${APP_VERSION}`;
export const APP_EDITION = "Локальная установка";

export const CLI_VERSION_LABEL = `${CLI_CHANNEL} v${CLI_VERSION}`;

export const PREFERENCES_TITLE = "Параметры";
export const SEARCH_LABEL = "Поиск параметров";
export const SEARCH_PLACEHOLDER = "Поиск";
export const SEARCH_EMPTY = "Ничего не найдено";
export const PREFERENCES_GROUP = "Приложение";
export const CLOSE_PREFERENCES = "Закрыть параметры";

export const SECTION_LABELS: Record<PreferenceSection, string> = {
  general: "Общие",
  profile: "Профиль",
  appearance: "Внешний вид",
  notifications: "Уведомления",
  memory: "Память",
  data: "Данные",
  about: "О системе",
  components: "Компоненты",
  voice: VOICE_INPUT,
  cli: "CLI",
};

export const EXTENSIONS_GROUP = "Расширения";

export const PREFERENCE_GROUPS: readonly {
  readonly label: string;
  readonly sections: readonly PreferenceSection[];
}[] = [
  {
    label: PREFERENCES_GROUP,
    sections: ["general", "profile", "appearance", "notifications", "memory", "data", "about"],
  },
  { label: EXTENSIONS_GROUP, sections: ["components", "voice", "cli"] },
];

export const SECTION_HINTS: Record<PreferenceSection, string> = {
  general: "Запуск, работа в фоне, ограничения генерации и проверка действий.",
  profile: "Стиль ответов модели и сведения о вас.",
  appearance: "Тема, шрифт, размер текста и эффекты интерфейса.",
  notifications: "Системные уведомления о событиях приложения.",
  memory: "Факты о пользователе и диалогах, которые модель получает в каждом ходе.",
  data: "Место на диске, занятое данными приложения, и очистка кэша.",
  about: "Версия приложения и сведения о консольной версии.",
  components: "Среды выполнения, которые нужны MCP-серверам.",
  cli: "Настройка утилиты командной строки.",
  voice: "Настройка распознавания голоса и управление голосовым вводом.",
};

export const TOGGLE_ON = "Включено";
export const TOGGLE_OFF = "Выключено";

export interface ToggleRowCopy {
  readonly key: AppSettingKey;
  readonly label: string;
  readonly hint: string;
}

export const GENERAL_ROWS: readonly ToggleRowCopy[] = [
  {
    key: "app.autostart",
    label: "Запускать вместе с системой",
    hint: "Открывать приложение при входе в Windows.",
  },
  {
    key: "app.tray",
    label: "Значок в области уведомлений",
    hint: "При закрытии окна приложение продолжает работать в области уведомлений.",
  },
];

export const NOTIFICATIONS_ROW: ToggleRowCopy = {
  key: "notifications.enabled",
  label: "Уведомления",
  hint: "Системные уведомления о событиях приложения.",
};

export const NOTIFICATION_KIND_ROWS: readonly ToggleRowCopy[] = [
  {
    key: "notifications.approval",
    label: "Требуется подтверждение",
    hint: "Инструмент ожидает разрешения на выполнение.",
  },
  {
    key: "notifications.turn",
    label: "Ответ готов",
    hint: "Модель завершила ответ в диалоге.",
  },
  {
    key: "notifications.download",
    label: "Загрузка завершена",
    hint: "Модель или компонент загружены.",
  },
];

export const AUTOSTART_FAILED = "Не удалось изменить автозапуск.";
export const SETTING_SAVE_FAILED = "Не удалось сохранить настройку.";

export const THEME_LABEL = "Тема";
export const THEME_HINT = "Светлая, тёмная или как в настройках системы.";
export const THEME_OPTIONS: readonly { value: ThemeMode; label: string }[] = [
  { value: "light", label: "Светлая" },
  { value: "dark", label: "Тёмная" },
  { value: "system", label: "Как в системе" },
];

export const PALETTES_LABEL = "Расширенные настройки темы";
export const PALETTES_HINT =
  "Палитра для каждой темы. Выбранная палитра применяется, когда включена её тема.";
export const PALETTE_GROUPS: readonly { mode: PaletteMode; label: string }[] = [
  { mode: "dark", label: "Тёмная" },
  { mode: "light", label: "Светлая" },
];

export const RAIL_PICKER_LABEL = "Переключатель темы в панели навигации";
export const RAIL_PICKER_HINT = "Показывать выбор темы над профилем в левой панели.";

export const FONT_FAMILY_LABEL = "Шрифт";
export const FONT_FAMILY_HINT = "Гарнитура текста интерфейса.";
export const FONT_FAMILY_OPTIONS: readonly { value: FontFamily; label: string }[] = [
  { value: "onest", label: "Onest" },
  { value: "system", label: "Системный" },
  { value: "mono", label: "JetBrains Mono" },
];

export const FONT_SIZE_LABEL = "Размер шрифта";
export const FONT_SIZE_HINT = "Масштаб текста и элементов интерфейса.";

export const FX_LABEL = "Эффекты";
export const FX_HINT = "Анимации переходов и состояний.";
export const FX_OS_NOTE = "Уровень выбран по настройке анимаций Windows.";
export const FX_OPTIONS: readonly { value: FxPreset; label: string }[] = [
  { value: "none", label: "Нет" },
  { value: "reduced", label: "Меньше" },
  { value: "full", label: "Все" },
  { value: "custom", label: "Пользовательские" },
];

export const FX_LEVEL_OPTIONS: readonly { value: FxLevel; label: string }[] = [
  { value: "none", label: "Нет" },
  { value: "reduced", label: "Меньше" },
  { value: "full", label: "Все" },
];

export const FX_MOTION_LABEL = "Анимации";
export const FX_MOTION_HINT =
  "Появление, раскрытие и перемещение элементов, индикаторы выполнения и ожидания.";
export const FX_EFFECTS_LABEL = "Визуальные эффекты";
export const FX_EFFECTS_HINT =
  "Тени и глубина, подсветка активных элементов, размытие под окнами, акцентные рамки.";

const FONT_STACKS: Record<FontFamily, string | null> = {
  onest: null,
  system: '"Segoe UI Variable Text", "Segoe UI", system-ui, sans-serif',
  mono: "var(--font-mono)",
};

export function fontStack(family: FontFamily): string | null {
  return FONT_STACKS[family];
}

export function interfaceZoom(size: FontSize): number {
  return size / DEFAULT_FONT_SIZE;
}

export function fontSizeLabel(size: FontSize): string {
  return size === DEFAULT_FONT_SIZE ? `${String(size)} px · обычный` : `${String(size)} px`;
}

export const FONT_SIZE_OPTIONS = FONT_SIZES.map((size) => ({
  value: String(size),
  label: String(size),
}));

export interface InfoRow {
  readonly label: string;
  readonly value: string;
}

export const APP_ROW_LABELS = {
  name: "Приложение",
  version: "Версия",
  edition: "Сборка",
} as const;

export function appRows(): InfoRow[] {
  return [
    { label: APP_ROW_LABELS.version, value: APP_VERSION_LABEL },
  ];
}

export const CLI_ROW_LABELS = {
  version: "Версия",
} as const;

export function cliRows(): InfoRow[] {
  return [
    { label: CLI_ROW_LABELS.version, value: CLI_VERSION_LABEL },
  ];
}
