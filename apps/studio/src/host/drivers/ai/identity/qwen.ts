import { z } from "zod";
import type { AccountIdentity } from "@zvs/shared";
import { DEFAULT_TOKEN_TYPE } from "../transport/AccountTransport.ts";
import type { SessionGateway } from "../transport/SessionGateway.ts";
import {
  fetchIdentity,
  malformedIdentity,
  type IdentityProbe,
  type ProbeResult,
} from "./IdentityProbe.ts";

export const QWEN_IDENTITY_ENDPOINT = "https://chat.qwen.ai/api/v1/auths/";
export const QWEN_LOGIN_URL = "https://chat.qwen.ai/";

const text = z
  .string()
  .nullish()
  .transform((value) => {
    const trimmed = value?.trim();
    return trimmed === undefined || trimmed.length === 0 ? null : trimmed;
  });

export const QwenIdentityPayload = z.object({
  id: z.union([z.string(), z.number()]).transform(String),
  email: text,
  name: text,
  profile_image_url: text,
  token: text,
  token_type: text,
  expires_at: z.number().int().positive().nullish(),
});

export function mapQwenIdentity(payload: unknown): ProbeResult {
  const parsed = QwenIdentityPayload.safeParse(payload);
  if (!parsed.success) throw malformedIdentity("qwen-web");
  const data = parsed.data;
  const externalId = data.id.trim();
  if (externalId.length === 0) throw malformedIdentity("qwen-web");

  const identity: AccountIdentity = {
    externalId,
    ...(data.email == null ? {} : { emailMasked: data.email }),
    ...(data.name == null ? {} : { displayName: data.name }),
    ...(data.profile_image_url == null ? {} : { avatarUrl: data.profile_image_url }),
    ...(data.expires_at == null ? {} : { expiresAt: data.expires_at }),
  };
  return {
    identity,
    credential:
      data.token == null
        ? null
        : { token: data.token, tokenType: data.token_type ?? DEFAULT_TOKEN_TYPE },
  };
}

export const qwenIdentityProbe: IdentityProbe = {
  family: "qwen-web",
  endpoint: QWEN_IDENTITY_ENDPOINT,
  loginUrl: QWEN_LOGIN_URL,
  async probe(session: SessionGateway, signal: AbortSignal): Promise<ProbeResult> {
    return mapQwenIdentity(
      await fetchIdentity("qwen-web", QWEN_IDENTITY_ENDPOINT, session, signal),
    );
  },
};
