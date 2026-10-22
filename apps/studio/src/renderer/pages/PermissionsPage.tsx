import { mdiShieldCheckOutline, mdiShieldOutline } from "@mdi/js";
import { observer } from "mobx-react-lite";
import { useEffect, useState } from "react";
import FolderGrantDetail from "../features/permissions/FolderGrantDetail";
import FolderGrantList from "../features/permissions/FolderGrantList";
import {
  FOLDERS_TAB,
  NOTHING_SELECTED_DESCRIPTION,
  NOTHING_SELECTED_TITLE,
  PAGE_TITLE,
  grantCountLabel,
} from "../features/permissions/permissionsPresentation";
import { useStore } from "../stores/useStore";
import SegmentedControl from "../ui/atoms/SegmentedControl";
import EmptyState from "../ui/molecules/EmptyState";
import PageShell from "../ui/templates/PageShell";

const TABS = [{ value: "folders", label: FOLDERS_TAB }] as const;

type TabValue = (typeof TABS)[number]["value"];

function PermissionsPage() {
  const { permissions } = useStore();
  const [tab, setTab] = useState<TabValue>("folders");

  useEffect(() => {
    void permissions.load();
  }, [permissions]);

  const selected = permissions.selected;

  return (
    <PageShell
      bleed={true}
      icon={mdiShieldCheckOutline}
      title={PAGE_TITLE}
      subtitle={grantCountLabel(permissions.grants.length)}
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
        <div
          role="alert"
          className="mx-6 mt-3 flex flex-none items-center gap-3 rounded-control bg-err-soft px-3 py-2.25 text-[12px] text-err"
        >
          <span className="flex-1">{permissions.error}</span>
          <button type="button" className="text-main-400" onClick={permissions.dismissError}>
            Скрыть
          </button>
        </div>
      )}
      <div className="flex min-h-0 flex-1">
        <FolderGrantList />
        {selected === null ? (
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
    </PageShell>
  );
}

export default observer(PermissionsPage);
