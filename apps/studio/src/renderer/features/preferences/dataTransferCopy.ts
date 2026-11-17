import { isIpcError } from "@zvs/ipc";
import { AppErrorCode, type TransferCategory } from "@zvs/shared";
import { plural } from "../connections/connectionPresentation";

export const DATA_EXPORT_ID = "data.export";
export const DATA_IMPORT_ID = "data.import";
export const DATA_EXPORT_TITLE = "Экспорт данных";
export const DATA_IMPORT_TITLE = "Импорт данных";
export const DATA_EXPORT_HINT = "Сохранение выбранных данных в файл .dbox";
export const DATA_IMPORT_HINT = "Добавление данных из файла .dbox";
export const DATA_EXPORT_GUIDANCE =
  "В архив включаются выбранные категории данных. Учётные данные и сеансы входа не экспортируются.";
export const DATA_IMPORT_GUIDANCE =
  "Данные добавляются к существующим, выбранные настройки заменяются. Провайдеры, коннекторы, сценарии и навыки импортируются отключёнными; хранилищам требуется переиндексация.";
export const DATA_EXPORT_ACTION = "Экспортировать";
export const DATA_IMPORT_ACTION = "Импортировать";
export const DATA_SELECT_FILE = "Выбрать файл";
export const DATA_CONTENTS = "Содержимое архива";
export const DATA_LOADING = "Загрузка сведений…";
export const DATA_UPLOADING = "Загрузка архива…";
export const DATA_INSPECTING = "Проверка архива…";
export const DATA_EXPORTING = "Создание архива…";
export const DATA_IMPORTING = "Импорт данных…";
export const DATA_DROP_TITLE = "Файл .dbox";
export const DATA_DROP_HINT = "Перетаскивание или выбор файла";
export const DATA_FILE_SELECTED = "Файл выбран";
export const DATA_REMOVE_FILE = "Очистить выбор";
export const DATA_CONNECTION_DEPENDENCY = "Связанные категории включаются автоматически.";
export const DATA_INVALID_FILE = "Недопустимый формат или размер файла.";
export const DATA_FAILED = "Не удалось выполнить перенос данных.";
export const DATA_BUSY = "Перенос данных недоступен во время выполнения задач.";
export const DATA_DUPLICATE = "Выбранные категории уже импортированы.";
export const DATA_MEMORY_LIMIT = "Превышен лимит записей памяти.";
export const DATA_DEPENDENCIES = "Отсутствуют связанные провайдеры или диалоги.";
export const DATA_CORRUPT = "Архив повреждён или имеет неподдерживаемый формат.";
export const DATA_CHANGED = "Данные изменились во время экспорта.";
export const DATA_DESTINATION = "Архив нельзя сохранять в папке данных приложения.";
export const DATA_EXTENSION = "Требуется расширение .dbox.";
export const DATA_EXPORTED = "Архив сохранён";
export const DATA_NO_SPACE = "Недостаточно места на диске.";
export const DATA_MISSING_FILES = "Связанные файлы недоступны для экспорта.";
export const DATA_TOO_LARGE = `Объём данных превышает ${(8).toLocaleString("ru-RU")} ГБ.`;
export const DATA_ALREADY_IMPORTED = "Уже импортировано";
export const DATA_IMPORTED = "Данные импортированы";
export const DATA_EMPTY = "Данные для переноса отсутствуют";
export const DATA_ARCHIVE_DATE = "Дата создания";
export const DATA_REFRESH = "Обновить";
export const DATA_SELECTED = "Выбрано";

export const DATA_CATEGORIES: readonly TransferCategory[] = [
  "conversations",
  "stores",
  "scenarios",
  "skills",
  "memory",
  "connections",
  "settings",
];

export const DATA_CATEGORY_COPY: Record<TransferCategory, { label: string; hint: string }> = {
  conversations: { label: "Диалоги", hint: "Сообщения, вложения, заметки и созданные файлы" },
  stores: { label: "Хранилища", hint: "Настройки и проиндексированные документы" },
  scenarios: { label: "Сценарии", hint: "Версии и черновики" },
  skills: { label: "Навыки", hint: "Инструкции и файлы" },
  memory: { label: "Память", hint: "Общая память и память диалогов" },
  connections: {
    label: "Провайдеры и коннекторы",
    hint: "Параметры провайдеров, MCP и интеграций",
  },
  settings: { label: "Настройки приложения", hint: "Оформление, профиль и уведомления" },
};

export function dataObjectCount(count: number): string {
  return plural(count, "объект", "объекта", "объектов");
}

export function transferErrorCopy(error: unknown): string {
  if (!isIpcError(error)) return DATA_FAILED;
  if (error.code === AppErrorCode.UNSUPPORTED_FORMAT) return DATA_CORRUPT;
  const reason = error.details?.reason;
  if (reason === "space") return DATA_NO_SPACE;
  if (reason === "missingFiles") return DATA_MISSING_FILES;
  if (reason === "size") return DATA_TOO_LARGE;
  if (reason === "busy") return DATA_BUSY;
  if (reason === "duplicate") return DATA_DUPLICATE;
  if (reason === "memoryLimit") return DATA_MEMORY_LIMIT;
  if (reason === "dependencies") return DATA_DEPENDENCIES;
  if (reason === "changed") return DATA_CHANGED;
  if (reason === "destination") return DATA_DESTINATION;
  if (reason === "extension") return DATA_EXTENSION;
  return DATA_FAILED;
}
