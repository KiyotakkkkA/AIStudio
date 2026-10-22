import { mdiShieldOutline } from "@mdi/js";
import { observer } from "mobx-react-lite";
import type { FolderGrantDto } from "@zvs/shared";
import StatusDot from "../../ui/atoms/StatusDot";
import EmptyState from "../../ui/molecules/EmptyState";
import EntityListPanel from "../../ui/molecules/EntityListPanel";
import ListRow from "../../ui/molecules/ListRow";
import ListRowSkeleton from "../../ui/molecules/ListRowSkeleton";
import { useStore } from "../../stores/useStore";
import {
  ALWAYS_SECTION,
  DIALOG_SECTION,
  EMPTY_DESCRIPTION,
  EMPTY_TITLE,
  FOLDERS_TAB,
  NOTHING_FOUND,
  boundLabel,
  lastUsedLabel,
} from "./permissionsPresentation";

function FolderGrantList() {
  const { permissions } = useStore();

  const section = (title: string, grants: readonly FolderGrantDto[]) =>
    grants.length === 0 ? null : (
      <section aria-label={title} className="flex flex-col gap-1">
        <h3 className="px-1 pt-1 text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase">
          {title}
        </h3>
        {grants.map((grant) => (
          <ListRow
            key={grant.id}
            selected={permissions.selectedId === grant.id}
            onClick={() => {
              permissions.select(grant.id);
            }}
          >
            <span className="flex min-w-0 items-center gap-2">
              <StatusDot
                tone={grant.scope === "always" ? "ok" : "accent"}
                title={grant.scope === "always" ? ALWAYS_SECTION : DIALOG_SECTION}
              />
              <span
                className="flex-1 truncate font-mono text-[12px] font-medium text-main-50"
                title={grant.path}
              >
                {grant.path}
              </span>
            </span>
            <span className="truncate text-[11px] text-main-400">
              {grant.conversation === null ? null : `«${grant.conversation.title}» · `}
              {boundLabel(grant)} · {lastUsedLabel(grant, permissions.now)}
            </span>
          </ListRow>
        ))}
      </section>
    );

  return (
    <EntityListPanel
      heading={FOLDERS_TAB}
      search={{
        value: permissions.query,
        label: "Поиск папок",
        placeholder: "Поиск папок…",
        onChange: permissions.setQuery,
      }}
    >
      {permissions.loading && !permissions.loaded ? (
        <>
          <ListRowSkeleton />
          <ListRowSkeleton />
        </>
      ) : null}

      {permissions.isEmpty ? (
        <div className="flex flex-1 items-center justify-center rounded-card bg-main-800 px-4">
          <EmptyState icon={mdiShieldOutline} title={EMPTY_TITLE} description={EMPTY_DESCRIPTION} />
        </div>
      ) : null}

      {!permissions.isEmpty && permissions.loaded && permissions.visible.length === 0 ? (
        <p className="rounded-card bg-main-800 px-3 py-4.5 text-center text-[12px] text-main-400">
          {NOTHING_FOUND}
        </p>
      ) : null}

      {section(ALWAYS_SECTION, permissions.always)}
      {section(DIALOG_SECTION, permissions.dialog)}
    </EntityListPanel>
  );
}

export default observer(FolderGrantList);
