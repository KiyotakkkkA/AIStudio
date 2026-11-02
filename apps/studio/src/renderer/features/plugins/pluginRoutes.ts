import { IntegrationKind } from "@zvs/shared";
import { ROUTES } from "../../app/routes";

export function pluginPath(kind: IntegrationKind): string {
  return `${ROUTES.plugins}/${kind}`;
}

export function accountPath(kind: IntegrationKind, accountId: string): string {
  return `${pluginPath(kind)}/${encodeURIComponent(accountId)}`;
}

export function parseKind(value: string | undefined): IntegrationKind | null {
  const parsed = IntegrationKind.safeParse(value);
  return parsed.success ? parsed.data : null;
}
