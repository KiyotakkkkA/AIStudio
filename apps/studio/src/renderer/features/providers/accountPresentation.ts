import type { AccountDto, AccountFamily, AccountStatus } from "@zvs/shared";
import { plural } from "../connections/connectionPresentation";
import type { StatusTone } from "../../ui/atoms/statusTone";

/** Mirrors the host's rotation window, so an expiring session reads like an expiring secret. */
export const EXPIRY_WARNING_WINDOW_MS = 14 * 24 * 60 * 60 * 1000;

export const ACCOUNT_ADAPTER_LABELS: Record<AccountFamily, string> = {
  openai: "OpenAI",
  copilot: "GitHub Copilot",
  "qwen-web": "Qwen",
  "zai-web": "Z.ai",
};

export const ACCOUNT_STATUS_LABELS: Record<AccountStatus, string> = {
  linked: "Аккаунт привязан",
  "needs-relink": "Нужен повторный вход",
  revoked: "Доступ отозван",
};

export const ACCOUNT_STATUS_SHORT: Record<AccountStatus, string> = {
  linked: "Привязан",
  "needs-relink": "Нужен вход",
  revoked: "Отозван",
};

export interface VendorMarkStyle {
  readonly logo: string;
  readonly tile: string;
}

export const ACCOUNT_VENDOR_MARKS: Record<AccountFamily, VendorMarkStyle> = {
  copilot: {
    logo: "M12 .5a12 12 0 0 0-3.8 23.4c.6.1.8-.3.8-.6v-2.2c-3.3.7-4-1.4-4-1.4-.5-1.4-1.3-1.8-1.3-1.8-1.1-.7.1-.7.1-.7 1.2.1 1.8 1.2 1.8 1.2 1.1 1.8 2.8 1.3 3.5 1 .1-.8.4-1.3.8-1.6-2.7-.3-5.5-1.3-5.5-5.9 0-1.3.5-2.4 1.2-3.2-.1-.3-.5-1.6.1-3.2 0 0 1-.3 3.3 1.2a11.4 11.4 0 0 1 6 0c2.3-1.5 3.3-1.2 3.3-1.2.6 1.6.2 2.9.1 3.2.8.8 1.2 1.9 1.2 3.2 0 4.6-2.8 5.6-5.5 5.9.4.4.8 1.1.8 2.2v3.3c0 .3.2.7.8.6A12 12 0 0 0 12 .5Z",
    tile: "bg-main-800 text-main-100",
  },
  openai: {
    logo: "M22.2819 9.8211a5.9847 5.9847 0 0 0-.5157-4.9108 6.0462 6.0462 0 0 0-6.5098-2.9A6.0651 6.0651 0 0 0 4.9807 4.1818a5.9847 5.9847 0 0 0-3.9977 2.9 6.0462 6.0462 0 0 0 .7427 7.0966 5.98 5.98 0 0 0 .511 4.9107 6.051 6.051 0 0 0 6.5146 2.9001A5.9847 5.9847 0 0 0 13.2599 24a6.0557 6.0557 0 0 0 5.7718-4.2058 5.9894 5.9894 0 0 0 3.9977-2.9001 6.0557 6.0557 0 0 0-.7475-7.0729zm-9.022 12.6081a4.4755 4.4755 0 0 1-2.8764-1.0408l.1419-.0804 4.7783-2.7582a.7948.7948 0 0 0 .3927-.6813v-6.7369l2.02 1.1686a.071.071 0 0 1 .038.052v5.5826a4.504 4.504 0 0 1-4.4945 4.4944zm-9.6607-4.1254a4.4708 4.4708 0 0 1-.5346-3.0137l.142.0852 4.783 2.7582a.7712.7712 0 0 0 .7806 0l5.8428-3.3685v2.3324a.0804.0804 0 0 1-.0332.0615L9.74 19.9502a4.4992 4.4992 0 0 1-6.1408-1.6464zM2.3408 7.8956a4.485 4.485 0 0 1 2.3655-1.9728V11.6a.7664.7664 0 0 0 .3879.6765l5.8144 3.3543-2.0201 1.1685a.0757.0757 0 0 1-.071 0l-4.8303-2.7865A4.504 4.504 0 0 1 2.3408 7.872zm16.5963 3.8558L13.1038 8.364 15.1192 7.2a.0757.0757 0 0 1 .071 0l4.8303 2.7913a4.4944 4.4944 0 0 1-.6765 8.1042v-5.6772a.79.79 0 0 0-.407-.667zm2.0107-3.0231l-.142-.0852-4.7735-2.7818a.7759.7759 0 0 0-.7854 0L9.409 9.2297V6.8974a.0662.0662 0 0 1 .0284-.0615l4.8303-2.7866a4.4992 4.4992 0 0 1 6.6802 4.66zM8.3065 12.863l-2.02-1.1638a.0804.0804 0 0 1-.038-.0567V6.0742a4.4992 4.4992 0 0 1 7.3757-3.4537l-.142.0805L8.704 5.459a.7948.7948 0 0 0-.3927.6813zm1.0976-2.3654l2.602-1.4998 2.6069 1.4998v2.9994l-2.5974 1.4997-2.6067-1.4997Z",
    tile: "bg-main-800 text-main-100",
  },
  "qwen-web": {
    logo: "M19.713 2.825h-5.907c0-.251-.066-.503-.196-.728L12.819.728A1.46 1.46 0 0 0 11.559 0h-1.58c-.52 0-1 .275-1.26.728l-2.42 4.19a1.45 1.45 0 0 0 0 1.453l.791 1.37c.26.448.74.727 1.26.727h11.36c.52 0 1-.275 1.26-.728l.79-1.37a1.45 1.45 0 0 0 0-1.451l-.79-1.37a1.46 1.46 0 0 0-1.26-.728zM.195 9.905l2.951 5.115a1.46 1.46 0 0 0-.531.531l-.791 1.37a1.45 1.45 0 0 0 0 1.452l.79 1.37c.26.448.74.727 1.26.727h4.84c.52 0 1-.275 1.26-.728l.79-1.37a1.45 1.45 0 0 0 0-1.451L5.086 7.083a1.46 1.46 0 0 0-1.26-.728h-1.58c-.52 0-1 .276-1.26.728l-.79 1.37a1.45 1.45 0 0 0 0 1.452m15.888 13.367 2.951-5.115c.217.125.469.196.728.196h1.582c.52 0 1-.275 1.26-.728l.79-1.37a1.45 1.45 0 0 0 0-1.451l-2.42-4.191a1.46 1.46 0 0 0-1.259-.728h-1.582c-.52 0-1 .276-1.259.728l-5.678 9.838a1.45 1.45 0 0 0 0 1.452l.79 1.37c.26.448.74.727 1.26.727h1.582c.52 0 1-.275 1.26-.728z",
    tile: "bg-accent-soft text-accent-medium",
  },
  "zai-web": {
    logo: "M12.105 2L9.927 4.953H.653L2.83 2h9.276zM23.254 19.048L21.078 22h-9.242l2.174-2.952h9.244zM24 2L9.264 22H0L14.736 2H24z",
    tile: "bg-info-soft text-info",
  },
};

export function adapterLabel(adapter: AccountFamily): string {
  return ACCOUNT_ADAPTER_LABELS[adapter];
}

export function accountTitle(account: AccountDto): string {
  const named = account.displayName ?? account.emailMasked;
  return named === null || named.length === 0 ? adapterLabel(account.adapter) : named;
}

export function accountProviderLabel(account: AccountDto): string {
  const vendor = adapterLabel(account.adapter);
  return account.adapter === "openai"
    ? `${vendor} · ${accountTitle(account)}`.slice(0, 128)
    : vendor;
}

export interface ExpiryLine {
  readonly text: string;
  readonly tone: StatusTone;
}

export function expiryLine(account: AccountDto, now: number): ExpiryLine | null {
  if (account.expiresAt === null) return null;
  const left = account.expiresAt - now;
  if (left <= 0) return { text: "Истекла", tone: "err" };
  const days = Math.max(1, Math.round(left / 86_400_000));
  const text = `Истекает через ${daysWord(days)}`;
  return { text, tone: left <= EXPIRY_WARNING_WINDOW_MS ? "warn" : "idle" };
}

export function accountStatusTone(account: AccountDto, now: number): StatusTone {
  if (account.status === "revoked") return "err";
  if (account.status === "needs-relink") return "warn";
  return expiryLine(account, now)?.tone === "warn" ? "warn" : "ok";
}

export function accountStatusLine(account: AccountDto): string {
  if (account.detail !== null && account.detail.length > 0) return account.detail;
  return ACCOUNT_STATUS_LABELS[account.status];
}

export function providersWord(count: number): string {
  return plural(count, "подключение", "подключения", "подключений");
}

export function accountsWord(count: number): string {
  return plural(count, "аккаунт", "аккаунта", "аккаунтов");
}

export function secondsLeftLabel(elapsedMs: number, totalMs: number): string | null {
  if (totalMs <= 0) return null;
  const seconds = Math.max(0, Math.round((totalMs - elapsedMs) / 1000));
  return `Осталось ${String(seconds)} с`;
}

function daysWord(days: number): string {
  return plural(days, "день", "дня", "дней");
}
