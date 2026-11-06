import { mdiShieldCheckOutline, mdiShieldOutline } from "@mdi/js";
import { observer } from "mobx-react-lite";
import { useEffect } from "react";
import FolderGrantDetail from "../features/permissions/FolderGrantDetail";
import FolderGrantList from "../features/permissions/FolderGrantList";
import {
  NOTHING_SELECTED_DESCRIPTION,
  NOTHING_SELECTED_TITLE,
  PAGE_TITLE,
} from "../features/permissions/permissionsPresentation";
import { useStore } from "../stores/useStore";
import EmptyState from "../ui/molecules/EmptyState";
import PageShell from "../ui/templates/PageShell";
import Alert from "../ui/molecules/Alert";
import { PaneSkeleton } from "../ui/organisms/PageSkeletons";
import { awaitingData } from "../ui/molecules/LoadingSkeleton";

function PermissionsPage() {
  const { permissions } = useStore();
  useEffect(() => {
    void permissions.load();
  }, [permissions]);

  const selected = permissions.selected;

  return (
    <PageShell
      bleed={true}
      icon={mdiShieldCheckOutline}
      title={PAGE_TITLE}
      subtitle="Управление доступом к файлам и папкам"
    >
      {permissions.error === null ? null : (
        <Alert className="mx-6 mt-3" onDismiss={permissions.dismissError}>
          {permissions.error}
        </Alert>
      )}
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
    </PageShell>
  );
}

export default observer(PermissionsPage);
