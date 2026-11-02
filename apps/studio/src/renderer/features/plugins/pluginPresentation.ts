import { mdiEmailOutline, mdiGitlab } from "@mdi/js";
import type { IntegrationAccountDto, IntegrationKind } from "@zvs/shared";
import { ACCOUNT_VENDOR_MARKS, type VendorMarkStyle } from "../providers/accountPresentation";

export const PLUGIN_KINDS: readonly IntegrationKind[] = ["mail", "github", "gitlab", "telegram"];

export const PLUGIN_LABELS: Record<IntegrationKind, string> = {
  mail: "Почта",
  telegram: "Telegram",
  github: "GitHub",
  gitlab: "GitLab",
};

export const PLUGIN_DESCRIPTIONS: Record<IntegrationKind, string> = {
  mail: "Поиск, чтение и отправка писем",
  telegram: "Сообщения бота и отправка файлов",
  github: "Файлы, задачи и запросы на слияние",
  gitlab: "Файлы, задачи и запросы на слияние",
};

const TELEGRAM_LOGO =
  "M9.04 15.6 8.9 19.7c.4 0 .6-.2.8-.4l2-1.9 4.1 3c.8.4 1.3.2 1.5-.7l2.7-12.7c.3-1.1-.4-1.6-1.1-1.3L3.1 11.4c-1.1.4-1 1-.2 1.3l4 1.2 9.3-5.9c.4-.3.8-.1.5.2z";

export const PLUGIN_MARKS: Record<IntegrationKind, VendorMarkStyle> = {
  mail: { logo: mdiEmailOutline, tile: "bg-accent-soft text-accent-medium" },
  telegram: { logo: TELEGRAM_LOGO, tile: "bg-[#229ed9] text-white" },
  github: { logo: ACCOUNT_VENDOR_MARKS.copilot.logo, tile: "bg-main-800 text-main-100" },
  gitlab: { logo: mdiGitlab, tile: "bg-[#fc6d26] text-white" },
};

export function pluginCommand(kind: IntegrationKind): string {
  return `/${kind}`;
}

export function pluginChipLabel(account: Pick<IntegrationAccountDto, "kind" | "name">): string {
  return `${PLUGIN_LABELS[account.kind]} · ${account.name}`;
}
