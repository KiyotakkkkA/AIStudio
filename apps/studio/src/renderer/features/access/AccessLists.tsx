import type { ReactNode } from "react";
import { observer } from "mobx-react-lite";
import { mdiOpenInNew, mdiShieldRemoveOutline, mdiTrashCanOutline } from "@mdi/js";
import { BROWSER_SITE_ACTIONS, type BrowserSiteDto } from "@zvs/shared";
import { useStore } from "../../stores/useStore";
import { useAppNavigation } from "../../hooks/useAppNavigation";
import { ROUTES } from "../../app/routes";
import Button from "../../ui/atoms/buttons/Button";
import IconButton from "../../ui/atoms/buttons/IconButton";
import Icon from "../../ui/atoms/Icon";
import AppIcon from "../computer/AppIcon";
import { appRevokeConsequence } from "../device/deviceCopy";
import {
  SITE_ACTION_LABELS,
  SITE_DECISION_LABELS,
  SITE_REMOVE_TITLE,
  siteHost,
  siteRemoveConsequence,
} from "../browser/browserSitesCopy";
import { TIER_HINTS, TIER_TEXT } from "../tools/toolPresentation";
import { grantedLabel, lastUsedLabel } from "../permissions/permissionsPresentation";
import {
  APPS_EMPTY,
  APP_REVOKE,
  APP_REVOKE_TITLE,
  SITES_EMPTY,
  SITE_DEFAULTS,
  SITE_REVOKE,
  TOOLS_EMPTY,
  TOOLS_NOTE,
  TOOLS_OPEN,
  appScope,
  toolScope,
} from "./accessCopy";

function Rows({
  empty,
  children,
}: {
  readonly empty: string | null;
  readonly children: ReactNode;
}) {
  if (empty !== null) return <p className="m-0 text-[12.5px] text-main-500">{empty}</p>;
  return <ul className="m-0 flex list-none flex-col gap-1 p-0">{children}</ul>;
}

function Row({
  lead,
  title,
  detail,
  action,
}: {
  readonly lead?: ReactNode;
  readonly title: ReactNode;
  readonly detail: ReactNode;
  readonly action?: ReactNode;
}) {
  return (
    <li className="flex items-center gap-3 rounded-control bg-main-800 px-3 py-2">
      {lead}
      <div className="min-w-0 flex-1">
        <p className="m-0 text-[12.5px] wrap-break-word text-main-100">{title}</p>
        <p className="m-0 text-[11.5px] text-main-500">{detail}</p>
      </div>
      {action}
    </li>
  );
}

function siteRules(site: BrowserSiteDto): string {
  return BROWSER_SITE_ACTIONS.flatMap((action) => {
    const decision = site.rules[action];
    return decision === undefined
      ? []
      : [
          `${SITE_ACTION_LABELS[action]}: ${SITE_DECISION_LABELS[decision].toLocaleLowerCase("ru-RU")}`,
        ];
  }).join(" · ");
}

export const SiteAccessList = observer(function SiteAccessList() {
  const { permissions: store } = useStore();
  const defaults = store.access?.sites.defaults;
  return (
    <div className="flex flex-col gap-3">
      {defaults === undefined ? null : (
        <Rows empty={null}>
          <Row
            title={SITE_DEFAULTS}
            detail={BROWSER_SITE_ACTIONS.map(
              (action) =>
                `${SITE_ACTION_LABELS[action]}: ${SITE_DECISION_LABELS[defaults[action]].toLocaleLowerCase("ru-RU")}`,
            ).join(" · ")}
          />
        </Rows>
      )}
      <Rows empty={store.sites.length === 0 ? SITES_EMPTY : null}>
        {store.sites.map((site) => (
          <Row
            key={site.origin}
            title={siteHost(site.origin)}
            detail={siteRules(site)}
            action={
              <IconButton
                path={mdiTrashCanOutline}
                label={SITE_REVOKE}
                tone="danger"
                size="sm"
                disabled={store.revoking}
                needConfirm
                modalSetup={{
                  title: SITE_REMOVE_TITLE,
                  content: siteRemoveConsequence(siteHost(site.origin)),
                  confirmLabel: SITE_REVOKE,
                }}
                onClick={() => void store.removeSite(site.origin)}
              />
            }
          />
        ))}
      </Rows>
    </div>
  );
});

export const AppAccessList = observer(function AppAccessList() {
  const { permissions: store } = useStore();
  return (
    <Rows empty={store.apps.length === 0 ? APPS_EMPTY : null}>
      {store.apps.map((grant) => (
        <Row
          key={grant.id}
          lead={<AppIcon icon={grant.icon} size={24} />}
          title={grant.app}
          detail={`${appScope(grant.conversation?.title ?? null)} · ${lastUsedLabel(grant, store.now)}`}
          action={
            <IconButton
              path={mdiShieldRemoveOutline}
              label={APP_REVOKE}
              tone="danger"
              size="sm"
              disabled={store.revoking}
              needConfirm
              modalSetup={{
                title: APP_REVOKE_TITLE,
                content: appRevokeConsequence(grant),
                confirmLabel: APP_REVOKE,
              }}
              onClick={() => void store.revokeApp(grant.id)}
            />
          }
        />
      ))}
    </Rows>
  );
});

export const ToolAccessList = observer(function ToolAccessList() {
  const { permissions: store } = useStore();
  const navigation = useAppNavigation();
  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center gap-3">
        <p className="m-0 min-w-0 flex-1 text-[12.5px] text-main-400">{TOOLS_NOTE}</p>
        <Button size="sm" tone="secondary" onClick={() => navigation.go(ROUTES.tools)}>
          <Icon path={mdiOpenInNew} size={14} />
          {TOOLS_OPEN}
        </Button>
      </div>
      <Rows empty={store.tools.length === 0 ? TOOLS_EMPTY : null}>
        {store.tools.map((grant) => (
          <Row
            key={grant.id}
            title={grant.displayName}
            detail={
              <>
                {toolScope(grant.scope)} ·{" "}
                <span className={TIER_TEXT[grant.tier]}>{TIER_HINTS[grant.tier]}</span> ·{" "}
                {grantedLabel(grant)}
              </>
            }
          />
        ))}
      </Rows>
    </div>
  );
});
