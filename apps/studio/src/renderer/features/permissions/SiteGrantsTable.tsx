import { mdiLinkVariantOff, mdiWeb } from "@mdi/js";
import { observer } from "mobx-react-lite";
import { useEffect } from "react";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import { useStore } from "../../stores/useStore";
import IconButton from "../../ui/atoms/buttons/IconButton";
import EmptyState from "../../ui/molecules/EmptyState";
import { PaneSkeleton } from "../../ui/organisms/PageSkeletons";
import {
  REVOKE,
  REVOKE_TITLE,
  SITES_DESCRIPTION,
  SITES_EMPTY_DESCRIPTION,
  SITES_EMPTY_TITLE,
  SITES_TITLE,
  SITE_GRANTED_COLUMN,
  grantedLabel,
  siteActionLabel,
  siteRevokeConsequence,
} from "./permissionsPresentation";

const HEAD = "text-[10px] font-semibold tracking-[0.07em] text-main-500 uppercase";

function SiteGrantsTable() {
  const { permissions } = useStore();

  useEffect(() => {
    void permissions.loadSiteGrants();
  }, [permissions]);

  if (!permissions.siteGrantsLoaded) return <PaneSkeleton />;

  const groups = permissions.siteGroups;

  if (groups.length === 0)
    return (
      <div className="flex min-w-0 flex-1 items-center justify-center p-4.5">
        <EmptyState icon={mdiWeb} title={SITES_EMPTY_TITLE} description={SITES_EMPTY_DESCRIPTION} />
      </div>
    );

  return (
    <ScrollArea className="flex min-h-0 min-w-0 flex-1 flex-col">
      <section aria-label={SITES_TITLE} className="flex w-full flex-col gap-3 px-6 py-5">
        <div className="flex flex-col gap-1">
          <h2 className="m-0 text-[14px] font-semibold text-main-50">{SITES_TITLE}</h2>
          <p className="m-0 text-[12.5px] leading-relaxed text-main-400">{SITES_DESCRIPTION}</p>
        </div>
        {groups.map((group) => (
          <section
            key={group.origin}
            aria-label={group.host}
            className="overflow-visible rounded-card border border-line"
          >
            <div className="flex items-center gap-4 border-b border-line bg-main-850 px-4 py-2.5">
              <span
                className="flex-1 truncate font-mono text-[12px] font-medium text-main-50"
                title={group.origin}
              >
                {group.host}
              </span>
              <span className={`w-56 flex-none ${HEAD}`}>{SITE_GRANTED_COLUMN}</span>
              <span className="w-8 flex-none" />
            </div>
            {group.grants.map((grant) => (
              <div
                key={grant.id}
                className="flex items-center gap-4 border-b border-line px-4 py-2 last:border-b-0"
              >
                <span className="flex-1 text-[13px] text-main-100">
                  {siteActionLabel(grant.tool)}
                </span>
                <span className="w-56 flex-none text-[12px] text-main-400">
                  {grantedLabel(grant)}
                </span>
                <span className="w-8 flex-none">
                  <IconButton
                    tone="danger"
                    path={mdiLinkVariantOff}
                    label={REVOKE}
                    tooltip="bottom-left"
                    disabled={permissions.revokingSiteGrant !== null}
                    needConfirm={true}
                    modalSetup={{
                      title: REVOKE_TITLE,
                      content: siteRevokeConsequence(grant),
                      confirmLabel: REVOKE,
                    }}
                    onClick={() => void permissions.revokeSiteGrant(grant.id)}
                  />
                </span>
              </div>
            ))}
          </section>
        ))}
      </section>
    </ScrollArea>
  );
}

export default observer(SiteGrantsTable);
