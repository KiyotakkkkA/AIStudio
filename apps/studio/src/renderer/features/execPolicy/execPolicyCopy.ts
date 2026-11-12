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
    label: "Ручное подтверждение",
    hint: "Для действий, требующих разрешения, подтверждение запрашивается при каждом вызове.",
    icon: mdiShieldCheckOutline,
  },
  "safety-ask": {
    label: "Подтверждение опасных действий",
    hint: "Потенциально опасные действия требуют подтверждения. Остальные выполняются автоматически.",
    icon: mdiShieldHalfFull,
  },
  "auto-bypass": {
    label: "Автоматическая проверка",
    hint: "Действия проверяются по правилам и оцениваются моделью-арбитром. При необходимости запрашивается подтверждение.",
    icon: mdiShieldSearch,
  },
  "no-restrictions": {
    label: "Без подтверждений",
    hint: "Действия выполняются без подтверждения с учётом установленных ограничений.",
    icon: mdiShieldOffOutline,
  },
};

export const JUDGE_OPTIONAL =
  "Модель-арбитр не выбрана. Непроверенные действия требуют подтверждения.";

export const JUDGE_SETUP = "Настроить";

export const NO_RESTRICTIONS_CONFIRM = "Отключить подтверждения?";

export const NO_RESTRICTIONS_CONSEQUENCE =
  "Удаление и изменение файлов, отправка данных и запуск программ будут выполняться без подтверждения с правами пользователя.";

export const NO_RESTRICTIONS_ACCEPT = "Отключить";

export const JUDGE_ROW = {
  key: "execPolicy.judgeModel",
  label: "Модель-арбитр",
  hint: "Оценивает действия, не определённые правилами, в режиме «Автоматическая проверка».",
} as const;

export const JUDGE_PROVIDER = "Провайдер";
export const JUDGE_MODEL = "Модель";
export const JUDGE_PROVIDER_LABEL = "Провайдер модели-арбитра";
export const JUDGE_MODEL_LABEL = "Модель-арбитр";
export const JUDGE_NONE = "Не выбрана";
export const JUDGE_CLEAR = "Сбросить";
export const JUDGE_NO_PROVIDERS = "Нет доступных провайдеров текстовых моделей.";
