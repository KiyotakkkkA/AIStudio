import { z } from "zod";
import type { AccountIdentity } from "@zvs/shared";
import { sessionExpired } from "../errors.ts";
import { DEFAULT_TOKEN_TYPE } from "../transport/AccountTransport.ts";
import type { SessionGateway } from "../transport/SessionGateway.ts";
import {
  fetchIdentity,
  malformedIdentity,
  type IdentityProbe,
  type ProbeResult,
} from "./IdentityProbe.ts";

export const DEEPSEEK_IDENTITY_ENDPOINT = "https://chat.deepseek.com/api/v0/users/current";

const text = z
  .string()
  .nullish()
  .transform((value) => {
    const trimmed = value?.trim();
    return trimmed === undefined || trimmed.length === 0 ? null : trimmed;
  });
const code = z.number().int().nullish();

const DeepSeekProfile = z.object({
  name: text,
  picture: text,
});

const DeepSeekBizData = z.object({
  id: z.union([z.string(), z.number()]).transform(String),
  email: text,
  token: text,
  id_profile: DeepSeekProfile.nullish(),
});

export const DeepSeekIdentityPayload = z.object({
  code,
  msg: text,
  data: z
    .object({
      biz_code: code,
      biz_msg: text,
      biz_data: DeepSeekBizData.nullish(),
    })
    .nullish(),
});

export function mapDeepSeekIdentity(payload: unknown): ProbeResult {
  const parsed = DeepSeekIdentityPayload.safeParse(payload);
  if (!parsed.success) throw malformedIdentity("deepseek-web");
  const { code: outer, data } = parsed.data;
  const inner = data?.biz_code;
  if ((outer != null && outer !== 0) || (inner != null && inner !== 0)) {
    throw sessionExpired({ family: "deepseek-web", code: outer ?? 0, bizCode: inner ?? 0 });
  }
  const biz = data?.biz_data;
  if (biz == null) throw sessionExpired({ family: "deepseek-web", reason: "no-profile" });
  const externalId = biz.id.trim();
  if (externalId.length === 0) throw malformedIdentity("deepseek-web");

  const identity: AccountIdentity = {
    externalId,
    ...(biz.email == null ? {} : { emailMasked: biz.email }),
    ...(biz.id_profile?.name == null ? {} : { displayName: biz.id_profile.name }),
    ...(biz.id_profile?.picture == null ? {} : { avatarUrl: biz.id_profile.picture }),
  };
  return {
    identity,
    credential: biz.token == null ? null : { token: biz.token, tokenType: DEFAULT_TOKEN_TYPE },
  };
}

export const deepSeekIdentityProbe: IdentityProbe = {
  family: "deepseek-web",
  endpoint: DEEPSEEK_IDENTITY_ENDPOINT,
  async probe(session: SessionGateway, signal: AbortSignal): Promise<ProbeResult> {
    return mapDeepSeekIdentity(
      await fetchIdentity("deepseek-web", DEEPSEEK_IDENTITY_ENDPOINT, session, signal),
    );
  },
};
