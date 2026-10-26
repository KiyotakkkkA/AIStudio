import type { AccountDto, AccountFamily, AccountStatus } from "@zvs/shared";
import type { StatusTone } from "../../ui/atoms/statusTone";

/** Mirrors the host's rotation window, so an expiring session reads like an expiring secret. */
export const EXPIRY_WARNING_WINDOW_MS = 14 * 24 * 60 * 60 * 1000;

export const ACCOUNT_ADAPTER_LABELS: Record<AccountFamily, string> = {
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
  const tail = count % 100;
  const last = count % 10;
  if (tail >= 11 && tail <= 14) return `${String(count)} подключений`;
  if (last === 1) return `${String(count)} подключение`;
  if (last >= 2 && last <= 4) return `${String(count)} подключения`;
  return `${String(count)} подключений`;
}

export function accountsWord(count: number): string {
  const tail = count % 100;
  const last = count % 10;
  if (tail >= 11 && tail <= 14) return `${String(count)} аккаунтов`;
  if (last === 1) return `${String(count)} аккаунт`;
  if (last >= 2 && last <= 4) return `${String(count)} аккаунта`;
  return `${String(count)} аккаунтов`;
}

export function secondsLeftLabel(elapsedMs: number, totalMs: number): string | null {
  if (totalMs <= 0) return null;
  const seconds = Math.max(0, Math.round((totalMs - elapsedMs) / 1000));
  return `Осталось ${String(seconds)} с`;
}

function daysWord(days: number): string {
  const tail = days % 100;
  const last = days % 10;
  if (tail >= 11 && tail <= 14) return `${String(days)} дней`;
  if (last === 1) return `${String(days)} день`;
  if (last >= 2 && last <= 4) return `${String(days)} дня`;
  return `${String(days)} дней`;
}
