import {
  mdiShieldCheckOutline,
  mdiShieldHalfFull,
  mdiShieldOffOutline,
  mdiShieldSearch,
} from "@mdi/js";
import type { ExecMode } from "./ExecPolicyStore";

export interface ExecModeCopy {
  readonly label: string;
  readonly hint: string;
  readonly icon: string;
}

export const EXEC_POLICY_LABEL = "Подтверждение действий";

export const EXEC_MODE_COPY: Record<ExecMode, ExecModeCopy> = {
  "always-ask": {
    label: "Всегда спрашивать",
    hint: "Перед каждым вызовом инструмента с уровнем ask запрашивается подтверждение. Инструменты с уровнем auto выполняются автоматически.",
    icon: mdiShieldCheckOutline,
  },
  "safety-ask": {
    label: "Только опасные действия",
    hint: "Подтверждение требуется для удаления и перезаписи файлов, перемещения с заменой, отправки форм, загрузки файлов и запуска программ. Остальные разрешённые действия выполняются автоматически.",
    icon: mdiShieldHalfFull,
  },
  "auto-bypass": {
    label: "Автопроверка",
    hint: "Опасные действия с файлами и запуск программ проходят проверку по правилам и при необходимости оцениваются моделью-арбитром. Если решение не принято, запрашивается подтверждение. Отправка форм и загрузка файлов требуют подтверждения.",
    icon: mdiShieldSearch,
  },
  "no-restrictions": {
    label: "Без ограничений",
    hint: "Разрешённые инструменты выполняются без подтверждения, включая удаление файлов и запуск программ с правами пользователя. Запросы доступа к файлам и папкам сохраняются. Отключённые инструменты, запрещённые папки, поля учётных данных и оплаты остаются недоступны.",
    icon: mdiShieldOffOutline,
  },
};

export const JUDGE_MISSING = "Модель-арбитр не выбрана.";
export const JUDGE_SETUP = "Настроить";

export const NO_RESTRICTIONS_CONFIRM = "Отключить подтверждения?";
export const NO_RESTRICTIONS_CONSEQUENCE =
  "Модель сможет удалять и перезаписывать файлы, отправлять формы и запускать программы без подтверждения. Программы выполняются с правами пользователя и без изоляции.";
export const NO_RESTRICTIONS_ACCEPT = "Отключить";

export const JUDGE_ROW = {
  key: "execPolicy.judgeModel",
  label: "Модель-арбитр",
  hint: "Оценивает опасные действия в режиме «Автопроверка».",
} as const;

export const JUDGE_PROVIDER = "Провайдер";
export const JUDGE_MODEL = "Модель";
export const JUDGE_PROVIDER_LABEL = "Провайдер модели-арбитра";
export const JUDGE_MODEL_LABEL = "Модель-арбитр";
export const JUDGE_NONE = "Не выбрана";
export const JUDGE_CLEAR = "Сбросить";
export const JUDGE_NO_PROVIDERS = "Нет включённых провайдеров текстовых моделей.";
