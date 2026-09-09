import type { AccountEntity } from "../../../data/schema/index.ts";
import type { SecretResolver } from "../ProviderRegistry.ts";
import { sessionExpired } from "../errors.ts";
import type { AccountCredentials } from "../transport/AccountTransport.ts";
import type { SessionGateway } from "../transport/SessionGateway.ts";
import type { IdentityProbe } from "./IdentityProbe.ts";
import { identityProbe } from "./registry.ts";

export function probeAccountCredentials(
  account: AccountEntity,
  session: SessionGateway,
  secrets: SecretResolver,
  probe: IdentityProbe = identityProbe(account.adapter),
): AccountCredentials {
  const read = async (signal: AbortSignal) => {
    const result = await probe.probe(session, signal);
    signal.throwIfAborted();
    if (result.identity.externalId !== account.externalId) throw sessionExpired();
    if (result.credential !== null) return result.credential;
    return account.tokenSecretId === null
      ? null
      : { token: await secrets.resolve(account.tokenSecretId), tokenType: "Bearer" };
  };
  return { current: read, refresh: read };
}
