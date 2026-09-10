import type { AccountDto, AccountFamily, AccountStatus } from "@zvs/shared";
import type { StatusTone } from "../../ui/atoms/statusTone";

/** Mirrors the host's rotation window, so an expiring session reads like an expiring secret. */
export const EXPIRY_WARNING_WINDOW_MS = 14 * 24 * 60 * 60 * 1000;

export const ACCOUNT_ADAPTER_LABELS: Record<AccountFamily, string> = {
  "qwen-web": "Qwen",
  "deepseek-web": "DeepSeek",
};

export const ACCOUNT_STATUS_LABELS: Record<AccountStatus, string> = {
  linked: "Аккаунт привязан",
  "needs-relink": "Нужен повторный вход",
  revoked: "Доступ отозван",
};

export function adapterLabel(adapter: AccountFamily): string {
  return ACCOUNT_ADAPTER_LABELS[adapter];
}

export function accountTitle(account: AccountDto): string {
  const named = account.displayName ?? account.emailMasked;
  return named === null || named.length === 0 ? adapterLabel(account.adapter) : named;
}

export function accountInitials(account: AccountDto): string {
  const source = account.displayName ?? account.emailMasked ?? adapterLabel(account.adapter);
  const words = source
    .split(/[\s\-_.@]+/u)
    .map((word) => word.replace(/[^\p{L}\p{N}]/gu, ""))
    .filter((word) => word.length > 0);
  if (words.length >= 2) return `${words[0]?.[0] ?? ""}${words[1]?.[0] ?? ""}`.toUpperCase();
  return (words[0] ?? source).slice(0, 2).toUpperCase();
}

export interface ExpiryLine {
  readonly text: string;
  readonly tone: StatusTone;
}

export function expiryLine(account: AccountDto, now: number): ExpiryLine | null {
  if (account.expiresAt === null) return null;
  const left = account.expiresAt - now;
  if (left <= 0) return { text: "Сессия истекла", tone: "err" };
  const days = Math.max(1, Math.round(left / 86_400_000));
  const text = `Сессия истекает через ${daysWord(days)}`;
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
