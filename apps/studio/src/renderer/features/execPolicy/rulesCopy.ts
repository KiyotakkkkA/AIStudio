import type { PolicyRuleEffect } from "@zvs/shared";
import { plural } from "../connections/connectionPresentation";

export const ALWAYS_IN_FOLDER = "Всегда разрешать в этой папке";
export const ALWAYS_ANYWHERE = "Всегда разрешать";
export const ALWAYS_CHANGES_IN_FOLDER = "Всегда разрешать такие изменения в папке";
export const ALWAYS_THESE_FILES = "Всегда разрешать для этих файлов";

export const RULES_ROW = {
  key: "execPolicy.rules",
  label: "Правила",
  hint: "Постоянные разрешения, данные в ходе работы в приложении",
} as const;

export const RULES_EMPTY =
  "Правил нет. Постоянные разрешения создаются при выборе «Всегда разрешать».";
export const RULES_LOADING = "Загрузка правил…";
export const RULES_FAILED = "Не удалось загрузить правила.";
export const RULE_REMOVE_LABEL = "Удалить правило";
export const RULE_INACTIVE = "не действует";
export const PACK_CONFIRM = "Подтвердить";
export const PACK_CONFIRM_TITLE = "Подтвердить изменение правил?";
export const PACK_CONFIRM_CONSEQUENCE =
  "Выбранные правила вступят в силу немедленно";

export const RULE_EFFECT_LABEL: Record<PolicyRuleEffect, string> = {
  allow: "Разрешение",
  review: "Проверка",
  ask: "Подтверждение",
  deny: "Запрет",
};

export function unconfirmedPack(pack: string, count: number): string {
  const rules = plural(count, "правило", "правила", "правил");
  return `Файл ${pack} изменён вне приложения. Разрешающие правила (${rules}) не действуют до подтверждения.`;
}

export function ruleCreated(createdAt: number): string {
  return `Создано ${new Date(createdAt).toLocaleString("ru-RU", { dateStyle: "short", timeStyle: "short" })}`;
}
