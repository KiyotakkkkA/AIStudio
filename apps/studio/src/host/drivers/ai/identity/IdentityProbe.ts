import { AppError, AppErrorCode, type AccountFamily, type AccountIdentity } from "@zvs/shared";
import { httpFailure, isSignedOutResponse, networkFailure, sessionExpired } from "../errors.ts";
import type { AccountToken } from "../transport/AccountTransport.ts";
import type { SessionGateway } from "../transport/SessionGateway.ts";

export const IDENTITY_TIMEOUT_SECONDS = 20;

export interface ProbeResult {
  readonly identity: AccountIdentity;
  readonly credential: AccountToken | null;
}

export interface IdentityProbe {
  readonly family: AccountFamily;
  readonly endpoint: string;
  readonly loginUrl: string;
  probe(session: SessionGateway, signal: AbortSignal): Promise<ProbeResult>;
}

export function malformedIdentity(family: AccountFamily): AppError {
  return new AppError(AppErrorCode.VALIDATION_FAILED, "Вендор вернул неизвестный формат профиля", {
    details: { family },
  });
}

export async function fetchIdentity(
  family: AccountFamily,
  endpoint: string,
  session: SessionGateway,
  signal: AbortSignal,
  credential: AccountToken | null = null,
): Promise<unknown> {
  const url = new URL(endpoint);
  const timeout = AbortSignal.timeout(IDENTITY_TIMEOUT_SECONDS * 1000);
  const combined = AbortSignal.any([signal, timeout]);
  let response: Response;
  try {
    response = await session.fetch(url.toString(), {
      method: "GET",
      headers: {
        accept: "application/json",
        "user-agent": session.userAgent(),
        origin: url.origin,
        referer: `${url.origin}/`,
        ...(credential === null
          ? {}
          : { authorization: `${credential.tokenType} ${credential.token}` }),
      },
      signal: combined,
    });
  } catch (error: unknown) {
    throw networkFailure(error);
  }

  const headers = collect(response.headers);
  const body = await response.text().catch(() => "");
  if (isSignedOutResponse(response.status, headers)) {
    throw sessionExpired({ family, status: response.status, partition: session.partition });
  }
  if (!response.ok) {
    throw httpFailure({ status: response.status, headers, body, authMode: "account" });
  }
  try {
    return JSON.parse(body) as unknown;
  } catch {
    throw malformedIdentity(family);
  }
}

function collect(headers: Headers): Record<string, string> {
  const result: Record<string, string> = {};
  headers.forEach((value, key) => {
    result[key.toLowerCase()] = value;
  });
  return result;
}
