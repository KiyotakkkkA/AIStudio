import { mdiAt, mdiEmailOutline, mdiGitlab, mdiGoogle, mdiServerNetworkOutline } from "@mdi/js";
import type { IntegrationAccountDto, IntegrationKind } from "@zvs/shared";
import { ACCOUNT_VENDOR_MARKS, type VendorMarkStyle } from "../providers/accountPresentation";
import { plural } from "../connections/connectionPresentation";
import type { StatusTone } from "../../ui/atoms/statusTone";

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

const YANDEX_LOGO =
  "M14.6 4h-2.3C9.1 4 7.3 5.7 7.3 8.3c0 2.1 1 3.3 2.8 4.5L7 20h2.6l3.3-6.9h1.7V20H17V4zm0 7.2H13c-1.8 0-2.9-.9-2.9-2.9 0-2 1.2-2.6 2.9-2.6h1.6z";

export const PLUGIN_MARKS: Record<IntegrationKind, VendorMarkStyle> = {
  mail: { logo: mdiEmailOutline, tile: "bg-accent-soft text-accent-medium" },
  telegram: { logo: TELEGRAM_LOGO, tile: "bg-[#229ed9] text-white" },
  github: { logo: ACCOUNT_VENDOR_MARKS.copilot.logo, tile: "bg-main-800 text-main-100" },
  gitlab: { logo: mdiGitlab, tile: "bg-[#fc6d26] text-white" },
};

export type MailPreset = "google" | "yandex" | "mailru" | "imap";

export const MAIL_PRESET_MARKS: Record<MailPreset, VendorMarkStyle> = {
  google: { logo: mdiGoogle, tile: "bg-white text-[#4285f4] ring-1 ring-inset ring-black/5" },
  yandex: { logo: YANDEX_LOGO, tile: "bg-[#fc3f1d] text-white" },
  mailru: { logo: mdiAt, tile: "bg-[#005ff9] text-[#ff9e00]" },
  imap: { logo: mdiServerNetworkOutline, tile: "bg-main-700 text-main-200" },
};

export interface PluginSection {
  readonly id: string;
  readonly label: string;
  readonly kinds: readonly IntegrationKind[];
}

export const PLUGIN_SECTIONS: readonly PluginSection[] = [
  { id: "messaging", label: "Почта и сообщения", kinds: ["mail", "telegram"] },
  { id: "development", label: "Разработка", kinds: ["github", "gitlab"] },
];

export const CATALOGUE_TITLE = "Плагины";
export const CATALOGUE_SUBTITLE =
  "Подключите плагины, чтобы использовать их в диалогах и сценариях";
export const SEARCH_PLACEHOLDER = "Поиск плагинов";

export type CatalogueTab = "all" | "connected";

export const CATALOGUE_TABS: readonly { readonly value: CatalogueTab; readonly label: string }[] = [
  { value: "all", label: "Все" },
  { value: "connected", label: "Подключённые" },
];

export const CONNECT_ACCOUNT = "Подключить аккаунт";
export const SIGNED_IN = "Вход выполнен";

export function pluginCommand(kind: IntegrationKind): string {
  return `/${kind}`;
}

export function pluginChipLabel(account: Pick<IntegrationAccountDto, "kind" | "name">): string {
  return `${PLUGIN_LABELS[account.kind]} · ${account.name}`;
}

export function accountsWord(count: number): string {
  return plural(count, "аккаунт", "аккаунта", "аккаунтов");
}

export function eventsWord(count: number): string {
  return plural(count, "событие", "события", "событий");
}

export type AccountState =
  | "disabled"
  | "signing"
  | "needsSignIn"
  | "stopped"
  | "starting"
  | "running"
  | "retrying"
  | "failed";

export const ACCOUNT_STATE_LABELS: Record<AccountState, string> = {
  disabled: "Выключен",
  signing: "Выполняется вход",
  needsSignIn: "Нужен вход",
  stopped: "Остановлен",
  starting: "Запуск",
  running: "Работает",
  retrying: "Повторное подключение",
  failed: "Ошибка",
};

export const ACCOUNT_STATE_TONES: Record<AccountState, StatusTone> = {
  disabled: "idle",
  signing: "accent",
  needsSignIn: "warn",
  stopped: "idle",
  starting: "accent",
  running: "ok",
  retrying: "warn",
  failed: "err",
};

export function formatCountdown(ms: number): string {
  const seconds = Math.max(0, Math.ceil(ms / 1000));
  const minutes = Math.floor(seconds / 60);
  return `${String(minutes)}:${String(seconds % 60).padStart(2, "0")}`;
}

export function formatWait(ms: number): string {
  const seconds = Math.max(1, Math.ceil(ms / 1000));
  if (seconds < 60) return `${seconds.toLocaleString("ru-RU")} с`;
  return `${Math.ceil(seconds / 60).toLocaleString("ru-RU")} мин`;
}

export function defaultAccountName(
  kind: IntegrationKind,
  accounts: readonly Pick<IntegrationAccountDto, "name">[],
): string {
  const label = PLUGIN_LABELS[kind];
  const taken = new Set(accounts.map((account) => account.name.trim().toLocaleLowerCase("ru-RU")));
  if (!taken.has(label.toLocaleLowerCase("ru-RU"))) return label;
  for (let index = 2; ; index += 1) {
    const candidate = `${label} ${String(index)}`;
    if (!taken.has(candidate.toLocaleLowerCase("ru-RU"))) return candidate;
  }
}

export function matchesQuery(kind: IntegrationKind, query: string): boolean {
  const needle = query.trim().toLocaleLowerCase("ru-RU");
  if (needle.length === 0) return true;
  return [PLUGIN_LABELS[kind], PLUGIN_DESCRIPTIONS[kind]].some((text) =>
    text.toLocaleLowerCase("ru-RU").includes(needle),
  );
}
