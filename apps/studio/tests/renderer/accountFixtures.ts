import type { AccountDto, AccountId, StreamId } from "@zvs/shared";

export const QWEN_ACCOUNT = "0199bb11-4444-7111-8111-000000000001" as AccountId;
export const DEEPSEEK_ACCOUNT = "0199bb11-4444-7111-8111-000000000002" as AccountId;
export const LINK_STREAM = "0199bb11-5555-7111-8111-000000000001" as StreamId;

export function account(overrides: Partial<AccountDto> = {}): AccountDto {
  return {
    id: QWEN_ACCOUNT,
    adapter: "qwen-web",
    emailMasked: "k***@example.com",
    displayName: "Кирилл",
    avatarUrl: null,
    status: "linked",
    detail: null,
    expiresAt: null,
    lastCheckedAt: 1_700_000_000_000,
    linkedProvidersCount: 1,
    ...overrides,
  };
}

export function deferred<T>(): { promise: Promise<T>; resolve: (value: T) => void } {
  let resolve: (value: T) => void = () => undefined;
  const promise = new Promise<T>((settle) => {
    resolve = settle;
  });
  return { promise, resolve };
}
