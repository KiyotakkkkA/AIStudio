import { mdiShieldCheckOutline, mdiShieldOutline } from "@mdi/js";
import { observer } from "mobx-react-lite";
import { useEffect } from "react";
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
  TAB_TOOLS,
} from "../features/access/accessCopy";
import { AppAccessList, SiteAccessList, ToolAccessList } from "../features/access/AccessLists";
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
  tools: TAB_TOOLS,
  rules: TAB_RULES,
};

const TAB_ORDER: readonly AccessTab[] = ["folders", "sites", "apps", "tools", "rules"];

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
  const { permissions, execPolicy } = useStore();
  useEffect(() => {
    void permissions.load();
  }, [permissions]);

  const tab = permissions.tab;
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
          tabs={TAB_ORDER.map((value) => ({
            value,
            label: TAB_LABELS[value],
            count: counts[value],
          }))}
          onChange={(next) => {
            permissions.setTab(next);
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
      {tab === "sites" ? <SiteAccessList /> : null}
      {tab === "apps" ? <AppAccessList /> : null}
      {tab === "tools" ? <ToolAccessList /> : null}
      {tab === "rules" ? <RuleList store={execPolicy} /> : null}
    </PageShell>
  );
}

export default observer(PermissionsPage);
