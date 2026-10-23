import type { DeviceProfileDto } from "@zvs/shared";
import {
  DEFAULT_FONT_SIZE,
  FONT_SIZES,
  type FontFamily,
  type FontSize,
  type PreferenceSection,
  type ThemeMode,
} from "../../stores/UiStore";
import { formatBytes } from "../downloads/downloadPresentation";
import type { AppSettingKey } from "./AppSettingsStore";

export const APP_NAME = "ZVS AI Studio";
export const APP_VERSION = "0.1.0";
export const APP_EDITION = "Локальная установка";

export const PREFERENCES_TITLE = "Параметры";
export const PREFERENCES_GROUP = "Приложение";
export const CLOSE_PREFERENCES = "Закрыть параметры";

export const SECTION_LABELS: Record<PreferenceSection, string> = {
  general: "Общие",
  appearance: "Внешний вид",
  notifications: "Уведомления",
  about: "О системе",
};

export const SECTION_HINTS: Record<PreferenceSection, string> = {
  general: "Запуск и работа в фоне.",
  appearance: "Тема, шрифт и размер текста интерфейса.",
  notifications: "Системные уведомления о событиях приложения.",
  about: "Версия приложения и характеристики компьютера.",
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

const PLATFORMS: Record<string, string> = {
  win32: "Windows",
  darwin: "macOS",
  linux: "Linux",
};

export interface InfoRow {
  readonly label: string;
  readonly value: string;
}

export function appRows(): InfoRow[] {
  return [
    { label: "Приложение", value: APP_NAME },
    { label: "Версия", value: APP_VERSION },
    { label: "Сборка", value: APP_EDITION },
  ];
}

export function deviceRows(device: DeviceProfileDto): InfoRow[] {
  const gpu =
    device.gpu === null
      ? "Не обнаружена"
      : [
          device.gpu.model,
          ...(device.gpu.vramBytes === null ? [] : [formatBytes(device.gpu.vramBytes)]),
        ].join(" · ");
  return [
    {
      label: "Операционная система",
      value: `${PLATFORMS[device.platform] ?? device.platform} · ${device.arch}`,
    },
    {
      label: "Процессор",
      value: `${device.cpuModel} · ядер: ${device.cpuCores.toLocaleString("ru-RU")}`,
    },
    {
      label: "Оперативная память",
      value: `${formatBytes(device.totalMemoryBytes)} · свободно ${formatBytes(device.freeMemoryBytes)}`,
    },
    { label: "Видеокарта", value: gpu },
    {
      label: "Свободно на диске",
      value: device.freeDiskBytes === 0 ? "Не измерено" : formatBytes(device.freeDiskBytes),
    },
  ];
}

export const DEVICE_LOADING = "Получение сведений о компьютере…";
export const DEVICE_UNAVAILABLE = "Не удалось получить сведения о компьютере.";
