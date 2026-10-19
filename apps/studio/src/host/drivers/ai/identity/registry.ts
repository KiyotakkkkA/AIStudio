import { AppError, AppErrorCode, type AccountFamily } from "@zvs/shared";
import type { IdentityProbe } from "./IdentityProbe.ts";
import { qwenIdentityProbe } from "./qwen.ts";

export const IDENTITY_PROBES: Readonly<Record<AccountFamily, IdentityProbe>> = {
  "qwen-web": qwenIdentityProbe,
};

export function identityProbe(family: AccountFamily): IdentityProbe {
  const probe = IDENTITY_PROBES[family];
  if (probe === undefined) {
    throw new AppError(AppErrorCode.VALIDATION_FAILED, "У семейства нет проверки личности", {
      details: { family },
    });
  }
  return probe;
}
