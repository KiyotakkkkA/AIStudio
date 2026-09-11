import type { AccountEntity } from "../../../data/schema/account.ts";
import type { SecretResolver } from "../ProviderRegistry.ts";
import type { SessionGateway } from "../transport/SessionGateway.ts";
import { identityProbe } from "./registry.ts";

/** Use the saved credential after a restart until the vendor page supplies a newer one. */
export async function accountSession(
  account: Pick<AccountEntity, "adapter" | "tokenSecretId">,
  session: SessionGateway,
  secrets: SecretResolver,
): Promise<SessionGateway> {
  const origin = new URL(identityProbe(account.adapter).endpoint).origin;
  const token =
    account.tokenSecretId === null ? null : await secrets.resolve(account.tokenSecretId);
  return {
    partition: session.partition,
    userAgent: () => session.userAgent(),
    fetch: (url, request) => session.fetch(url, request),
    accountToken: (url) => {
      if (new URL(url).origin !== origin) return null;
      return (
        session.accountToken?.(url) ?? (token === null ? null : { token, tokenType: "Bearer" })
      );
    },
  };
}
