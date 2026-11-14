import type { CliChangeDto, CliConsentDto } from "@zvs/shared";
import { EXEC_MODE_COPY } from "../execPolicy/execPolicyCopy";
import { CLI_CLIENT_NO_IMAGE } from "./cliCopy";

export const CLI_CONSENT_TITLE = "Изменение доступа из консоли";
export const CLI_CONSENT_ALLOW = "Разрешить";
export const CLI_CONSENT_DECLINE = "Отклонить";
export const CLI_CONSENT_FAILED = "Не удалось передать ответ.";
export const CLI_CONSENT_JUDGE = "Смена модели-арбитра.";
export const CLI_CONSENT_RULE_FOLDER = "Разрешающее правило для рабочей папки.";
export const CLI_CONSENT_RULE_ANYWHERE = "Разрешающее правило для всех папок.";
export const CLI_CONSENT_ALWAYS_APPROVAL = "Постоянное разрешение действия.";
export const CLI_CONSENT_ALWAYS_FOLDER = "Постоянный доступ к папке.";
export const CLI_CONSENT_SOURCE_RECOGNISED = "Источник: консольная версия.";
export const CLI_CONSENT_SOURCE_UNKNOWN =
  "Источник не распознан как установленная консольная версия";

export function cliChangeText(change: CliChangeDto): string {
  switch (change.kind) {
    case "mode":
      return `Режим подтверждения: «${EXEC_MODE_COPY[change.from].label}» → «${EXEC_MODE_COPY[change.to].label}».`;
    case "judge":
      return CLI_CONSENT_JUDGE;
    case "allowRule":
      return change.scope === "folder" ? CLI_CONSENT_RULE_FOLDER : CLI_CONSENT_RULE_ANYWHERE;
    case "alwaysApproval":
      return CLI_CONSENT_ALWAYS_APPROVAL;
    case "alwaysFolder":
      return CLI_CONSENT_ALWAYS_FOLDER;
  }
}

export function cliSourceText(consent: CliConsentDto): string {
  if (consent.recognised) return CLI_CONSENT_SOURCE_RECOGNISED;
  return `${CLI_CONSENT_SOURCE_UNKNOWN}: ${consent.image ?? CLI_CLIENT_NO_IMAGE}.`;
}
