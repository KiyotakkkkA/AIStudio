import { mdiShieldCheckOutline, mdiShieldOutline } from "@mdi/js";
import { observer } from "mobx-react-lite";
import { useEffect } from "react";
import { useSearchParams } from "react-router-dom";
import AppPermissions from "../features/device/AppPermissions";
import SitePermissions from "../features/browser/SitePermissions";
import {
  ACCESS_TAB_ORDER,
  fromBrowserPreferences,
  fromDevicePreferences,
  requestedAccessTab,
} from "../features/access/accessNavigation";
import FolderGrantDetail from "../features/permissions/FolderGrantDetail";
import FolderGrantList from "../features/permissions/FolderGrantList";
import type { AccessTab } from "../features/permissions/PermissionsStore";
import {
  NOTHING_SELECTED_DESCRIPTION,
  NOTHING_SELECTED_TITLE,
  PAGE_TITLE,
} from "../features/permissions/permissionsPresentation";
import {
  ACCESS_TABS_LABEL,
  JUDGE_LABEL,
  MODE_LABEL,
  TAB_APPS,
  TAB_FOLDERS,
  TAB_RULES,
  TAB_SITES,
} from "../features/access/accessCopy";
import { EXEC_MODE_COPY, JUDGE_NONE } from "../features/execPolicy/execPolicyCopy";
import RuleList from "../features/execPolicy/RuleList";
import { useStore } from "../stores/useStore";
import EmptyState from "../ui/molecules/EmptyState";
import PageShell from "../ui/templates/PageShell";
import PageTabs from "../ui/molecules/PageTabs";
import Alert from "../ui/molecules/Alert";
import { PaneSkeleton } from "../ui/organisms/PageSkeletons";
import { awaitingData } from "../ui/molecules/LoadingSkeleton";

const TAB_LABELS: Record<AccessTab, string> = {
  folders: TAB_FOLDERS,
  sites: TAB_SITES,
  apps: TAB_APPS,
  rules: TAB_RULES,
};

const FoldersPane = observer(function FoldersPane() {
  const { permissions } = useStore();
  const selected = permissions.selected;
  return (
    <div className="flex min-h-0 flex-1">
      <FolderGrantList />
      <div className="flex min-w-0 flex-1">
        <div className="mx-auto flex min-w-0 max-w-240 flex-1">
          {selected === null && awaitingData(permissions) ? (
            <PaneSkeleton />
          ) : selected === null ? (
            <div className="flex min-w-0 flex-1 items-center justify-center p-4.5">
              <EmptyState
                icon={mdiShieldOutline}
                title={NOTHING_SELECTED_TITLE}
                description={NOTHING_SELECTED_DESCRIPTION}
              />
            </div>
          ) : (
            <FolderGrantDetail
              grant={selected}
              now={permissions.now}
              revoking={permissions.revoking}
              onRevoke={() => void permissions.revoke(selected.id)}
            />
          )}
        </div>
      </div>
    </div>
  );
});

function PermissionsPage() {
  const { permissions, execPolicy, browserSites, devicePermissions, ui } = useStore();
  const [params, setParams] = useSearchParams();
  const requestedTab = requestedAccessTab(params);
  const fromBrowser = fromBrowserPreferences(params);
  const fromDevice = fromDevicePreferences(params);
  useEffect(() => {
    if (requestedTab !== null) permissions.setTab(requestedTab);
  }, [permissions, requestedTab]);
  useEffect(() => {
    void permissions.load();
  }, [permissions]);

  const tab = requestedTab ?? permissions.tab;
  const policy = permissions.access?.policy ?? null;
  const counts = permissions.counts;

  return (
    <PageShell
      bleed={tab === "folders"}
      icon={mdiShieldCheckOutline}
      title={PAGE_TITLE}
      subtitle={
        policy === null
          ? undefined
          : `${MODE_LABEL}: ${EXEC_MODE_COPY[policy.mode].label} · ${JUDGE_LABEL}: ${
              policy.judge === null ? JUDGE_NONE.toLocaleLowerCase("ru-RU") : policy.judge.modelId
            }`
      }
      toolbar={
        <PageTabs
          label={ACCESS_TABS_LABEL}
          value={tab}
          tabs={ACCESS_TAB_ORDER.map((value) => ({
            value,
            label: TAB_LABELS[value],
            count:
              value === "sites" && browserSites.loaded
                ? browserSites.sites.sites.length
                : value === "apps" && devicePermissions.loaded
                  ? devicePermissions.grants.length
                  : counts[value],
          }))}
          onChange={(next) => {
            permissions.setTab(next);
            const nextParams = new URLSearchParams(params);
            nextParams.set("tab", next);
            setParams(nextParams);
            void permissions.load();
          }}
        />
      }
    >
      {permissions.error === null ? null : (
        <Alert
          className={tab === "folders" ? "mx-6 mt-3" : undefined}
          onDismiss={permissions.dismissError}
        >
          {permissions.error}
        </Alert>
      )}
      {tab === "folders" ? <FoldersPane /> : null}
      {tab === "sites" ? (
        <div className="mx-auto w-full max-w-240">
          <SitePermissions onBack={fromBrowser ? () => ui.openPreferences("browser") : undefined} />
        </div>
      ) : null}
      {tab === "apps" ? (
        <div className="mx-auto w-full max-w-240">
          <AppPermissions
            onBack={
              fromDevice
                ? () => {
                    devicePermissions.cancelAdding();
                    ui.openPreferences("device");
                  }
                : undefined
            }
          />
        </div>
      ) : null}
      {tab === "rules" ? (
        <div className="mx-auto w-full max-w-240">
          <RuleList store={execPolicy} />
        </div>
      ) : null}
    </PageShell>
  );
}

export default observer(PermissionsPage);
