import { useEffect } from "react";
import { mdiShieldOutline } from "@mdi/js";
import { observer } from "mobx-react-lite";
import type { AppGrantDto } from "@zvs/shared";
import StatusDot from "../../ui/atoms/StatusDot";
import EmptyState from "../../ui/molecules/EmptyState";
import EntityListPanel from "../../ui/molecules/EntityListPanel";
import ListRow from "../../ui/molecules/ListRow";
import { ListSkeleton } from "../../ui/organisms/PageSkeletons";
import { useStore } from "../../stores/useStore";
import AppIcon from "../computer/AppIcon";
import {
  ALWAYS_SECTION,
  APPS_EMPTY_DESCRIPTION,
  APPS_EMPTY_TITLE,
  APPS_SEARCH_LABEL,
  APPS_SEARCH_PLACEHOLDER,
  APPS_TAB,
  DIALOG_SECTION,
  NOTHING_FOUND,
  appLastUsedLabel,
} from "./permissionsPresentation";

function AppGrantList() {
  const { permissions } = useStore();

  useEffect(() => {
    void permissions.loadAppGrants();
  }, [permissions]);

  const section = (title: string, grants: readonly AppGrantDto[]) =>
    grants.length === 0 ? null : (
      <section aria-label={title} className="flex flex-col gap-1">
        <h3 className="px-1 pt-1 text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase">
          {title}
        </h3>
        {grants.map((grant) => (
          <ListRow
            key={grant.id}
            selected={permissions.selectedAppId === grant.id}
            onClick={() => {
              permissions.selectApp(grant.id);
            }}
          >
            <span className="flex min-w-0 items-center gap-2.5">
              <AppIcon icon={grant.icon} size={24} />
              <span className="flex min-w-0 flex-1 flex-col">
                <span className="flex min-w-0 items-center gap-2">
                  <StatusDot
                    tone={grant.scope === "always" ? "ok" : "accent"}
                    title={grant.scope === "always" ? ALWAYS_SECTION : DIALOG_SECTION}
                  />
                  <span className="truncate text-[12.5px] font-medium text-main-50">
                    {grant.app}
                  </span>
                </span>
                <span className="truncate text-[11px] text-main-400">
                  {grant.conversation === null ? null : `«${grant.conversation.title}» · `}
                  {appLastUsedLabel(grant, permissions.now)}
                </span>
              </span>
            </span>
          </ListRow>
        ))}
      </section>
    );

  const empty = permissions.appGrantsLoaded && permissions.appGrants.length === 0;

  return (
    <EntityListPanel
      heading={APPS_TAB}
      search={{
        value: permissions.appQuery,
        label: APPS_SEARCH_LABEL,
        placeholder: APPS_SEARCH_PLACEHOLDER,
        onChange: permissions.setAppQuery,
      }}
    >
      {permissions.appGrantsLoaded ? null : <ListSkeleton />}

      {empty ? (
        <div className="flex flex-1 items-center justify-center rounded-card bg-main-800 px-3">
          <EmptyState
            icon={mdiShieldOutline}
            title={APPS_EMPTY_TITLE}
            description={APPS_EMPTY_DESCRIPTION}
          />
        </div>
      ) : null}

      {!empty && permissions.appGrantsLoaded && permissions.visibleApps.length === 0 ? (
        <p className="rounded-card bg-main-800 px-3 py-4.5 text-center text-[12px] text-main-400">
          {NOTHING_FOUND}
        </p>
      ) : null}

      {section(ALWAYS_SECTION, permissions.globalApps)}
      {section(DIALOG_SECTION, permissions.dialogApps)}
    </EntityListPanel>
  );
}

export default observer(AppGrantList);
