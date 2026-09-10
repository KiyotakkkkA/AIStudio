import { z } from "zod";
import { AccountFamily, AccountStatus } from "./ai.js";
import { AccountId } from "./primitives/branded.js";

export const AccountDto = z.object({
  id: AccountId,
  adapter: AccountFamily,
  emailMasked: z.string().nullable(),
  displayName: z.string().nullable(),
  avatarUrl: z.string().nullable(),
  status: AccountStatus,
  detail: z.string().nullable(),
  expiresAt: z.number().nullable(),
  lastCheckedAt: z.number().nullable(),
  linkedProvidersCount: z.number().int().nonnegative(),
});
export type AccountDto = z.infer<typeof AccountDto>;

export const AccountLinkInput = z.object({ adapter: AccountFamily });
export const AccountRef = z.object({ id: AccountId });
export const AccountLinkResult = z.discriminatedUnion("status", [
  z.object({
    status: z.literal("linked"),
    account: AccountDto,
    replacedPreviousIdentity: z.boolean(),
    detail: z.string().nullable(),
  }),
  z.object({ status: z.enum(["cancelled", "timeout"]), detail: z.string() }),
]);
export type AccountLinkResult = z.infer<typeof AccountLinkResult>;
