import { isIpcError } from "@zvs/ipc";
import { AppErrorCode } from "@zvs/shared";

export const VOICE_INPUT = "Голосовой ввод";
export const VOICE_OPTIONS = "Параметры голосового ввода";
export const VOICE_DEVICE_ROW = {
  key: "voice.input.device",
  label: "Устройство записи",
  hint: "Микрофон, с которого идёт запись голоса.",
} as const;
export const VOICE_MODEL_ROW = {
  key: "voice.model",
  label: "Модель распознавания речи",
  hint: "Распознавание выполняется на этом компьютере, звук не отправляется в сеть. Другие модели доступны в «Загрузках».",
} as const;
export const VOICE_DEFAULT_DEVICE = "Как в системе";
export const VOICE_DEVICE_MISSING = "Устройство не подключено";
export const VOICE_REFRESH = "Обновить список устройств";
export const VOICE_SETUP = "Настроить";
export const VOICE_DEVICES_FAILED = "Не удалось получить список устройств записи.";
export const VOICE_SELECT_MISSING = "Устройство записи отключено. Обновите список устройств.";
export const VOICE_SELECT_FAILED = "Не удалось сохранить устройство записи.";

export const VOICE_NO_MODEL = "Не выбрана";
export const VOICE_NO_MODELS = "Нет установленных моделей";
export const VOICE_MODEL_MISSING = "Не установлена";
export const VOICE_RECOMMENDED = "Рекомендуется для этого компьютера";
export const VOICE_INSTALL = "Установить";
export const VOICE_CHOOSE = "Выбрать";
export const VOICE_DOWNLOADING = "Загрузка…";
export const VOICE_MODELS_FAILED = "Не удалось получить список моделей распознавания речи.";
export const VOICE_INSTALL_FAILED = "Не удалось начать загрузку модели.";
export const VOICE_UNSUPPORTED = "Распознавание речи доступно только в Windows.";
export const VOICE_MODEL_SELECT_FAILED = "Не удалось выбрать модель: она ещё не загружена.";
export const VOICE_NOT_READY = "Модель распознавания речи не установлена.";

export const VOICE_RECORD = "Начать запись";
export const VOICE_STOP = "Остановить запись";
export const VOICE_RECOGNIZING = "Распознавание…";
export const VOICE_RECORD_FAILED = "Не удалось начать запись с микрофона.";
export const VOICE_RECOGNITION_FAILED = "Не удалось распознать речь.";
export const VOICE_NOTHING_HEARD = "Речь не распознана.";
export const VOICE_CANCEL_HINT = "Esc — отменить запись";

export function systemDeviceLabel(defaultName: string | undefined): string {
  return defaultName === undefined
    ? VOICE_DEFAULT_DEVICE
    : `${VOICE_DEFAULT_DEVICE} · ${defaultName}`;
}

function codeOf(error: unknown): AppErrorCode | null {
  return isIpcError(error) ? error.code : null;
}

export function voiceSelectFailureCopy(error: unknown): string {
  return codeOf(error) === AppErrorCode.NOT_FOUND ? VOICE_SELECT_MISSING : VOICE_SELECT_FAILED;
}

export function installFailureCopy(error: unknown): string {
  return codeOf(error) === AppErrorCode.UNSUPPORTED_FORMAT
    ? VOICE_UNSUPPORTED
    : VOICE_INSTALL_FAILED;
}

export function startFailureCopy(error: unknown): string {
  return codeOf(error) === AppErrorCode.CONFLICT ? VOICE_NOT_READY : VOICE_RECORD_FAILED;
}
