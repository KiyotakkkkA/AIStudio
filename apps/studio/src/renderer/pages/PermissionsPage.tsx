import { mdiShieldCheckOutline, mdiShieldOutline } from "@mdi/js";
import { observer } from "mobx-react-lite";
import { useEffect } from "react";
import { useSearchParams } from "react-router-dom";
import FolderGrantDetail from "../features/permissions/FolderGrantDetail";
import FolderGrantList from "../features/permissions/FolderGrantList";
import ToolDefaultsTable from "../features/permissions/ToolDefaultsTable";
import SiteGrantsTable from "../features/permissions/SiteGrantsTable";
import {
  BROWSER_TAB,
  BROWSER_TAB_PARAM,
  FOLDERS_TAB,
  NOTHING_SELECTED_DESCRIPTION,
  NOTHING_SELECTED_TITLE,
  PAGE_TITLE,
  TOOLS_TAB,
  TOOLS_TAB_PARAM,
  grantCountLabel,
  siteCountLabel,
} from "../features/permissions/permissionsPresentation";
import { useStore } from "../stores/useStore";
import SegmentedControl from "../ui/atoms/SegmentedControl";
import EmptyState from "../ui/molecules/EmptyState";
import PageShell from "../ui/templates/PageShell";
import Alert from "../ui/molecules/Alert";
import { PaneSkeleton } from "../ui/organisms/PageSkeletons";
import { InlineSkeleton, PAGE_LOADING, awaitingData } from "../ui/molecules/LoadingSkeleton";

const TABS = [
  { value: "folders", label: FOLDERS_TAB },
  { value: TOOLS_TAB_PARAM, label: TOOLS_TAB },
  { value: BROWSER_TAB_PARAM, label: BROWSER_TAB },
] as const;

type TabValue = (typeof TABS)[number]["value"];

function PermissionsPage() {
  const { permissions } = useStore();
  const [params, setParams] = useSearchParams();
  const tab: TabValue =
    TABS.find((option) => option.value === params.get("tab"))?.value ?? "folders";
  const setTab = (value: TabValue) => {
    setParams(value === "folders" ? {} : { tab: value }, { replace: true });
  };

  useEffect(() => {
    void permissions.load();
  }, [permissions]);

  const selected = permissions.selected;

  return (
    <PageShell
      bleed={true}
      icon={mdiShieldCheckOutline}
      title={PAGE_TITLE}
      subtitle={
        tab === BROWSER_TAB_PARAM ? (
          permissions.siteGrantsLoaded ? (
            siteCountLabel(permissions.siteGroups.length)
          ) : (
            <InlineSkeleton label={PAGE_LOADING} className="w-28" />
          )
        ) : awaitingData(permissions) ? (
          <InlineSkeleton label={PAGE_LOADING} className="w-28" />
        ) : (
          grantCountLabel(permissions.grants.length)
        )
      }
      toolbar={
        <SegmentedControl
          label="Вид разрешений"
          value={tab}
          options={TABS}
          onChange={setTab}
          ghost={true}
        />
      }
    >
      {permissions.error === null ? null : (
        <Alert className="mx-6 mt-3" onDismiss={permissions.dismissError}>
          {permissions.error}
        </Alert>
      )}
      {tab === TOOLS_TAB_PARAM ? (
        <ToolDefaultsTable />
      ) : tab === BROWSER_TAB_PARAM ? (
        <SiteGrantsTable />
      ) : (
        <div className="flex min-h-0 flex-1">
          <FolderGrantList />
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
      )}
    </PageShell>
  );
}

export default observer(PermissionsPage);
