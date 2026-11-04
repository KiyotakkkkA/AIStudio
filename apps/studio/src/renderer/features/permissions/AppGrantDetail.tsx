import {
  mdiApplicationOutline,
  mdiCalendarOutline,
  mdiChatOutline,
  mdiClockOutline,
  mdiFolderOutline,
  mdiShieldCheckOutline,
  mdiShieldRemoveOutline,
} from "@mdi/js";
import { observer } from "mobx-react-lite";
import type { AppGrantDto } from "@zvs/shared";
import IconButton from "../../ui/atoms/buttons/IconButton";
import InfoTable from "../../ui/molecules/InfoTable";
import AppIcon from "../computer/AppIcon";
import {
  APP_PATH_UNKNOWN,
  REVOKE,
  REVOKE_TITLE,
  appLastUsedLabel,
  appRevokeConsequence,
  appScopeLabel,
  grantedLabel,
} from "./permissionsPresentation";

export interface AppGrantDetailProps {
  readonly grant: AppGrantDto;
  readonly now: number;
  readonly revoking: boolean;
  readonly onRevoke: (id: string) => void;
}

function AppGrantDetail({ grant, now, revoking, onRevoke }: AppGrantDetailProps) {
  const path = grant.path.length > 0 ? grant.path : APP_PATH_UNKNOWN;
  return (
    <div className="flex min-w-0 flex-1 flex-col gap-4 p-5">
      <div className="flex min-w-0 items-start gap-3">
        <AppIcon icon={grant.icon} size={40} />
        <div className="min-w-0 flex-1">
          <h2 className="text-[15px] font-semibold break-all text-main-50">{grant.app}</h2>
          <p className="mt-1 text-[12px] text-main-400">{appScopeLabel(grant)}</p>
        </div>
        <IconButton
          tone="danger"
          size="md"
          path={mdiShieldRemoveOutline}
          label={REVOKE}
          disabled={revoking}
          needConfirm={true}
          modalSetup={{
            title: REVOKE_TITLE,
            content: appRevokeConsequence(grant),
            confirmLabel: REVOKE,
          }}
          onClick={() => onRevoke(grant.id)}
        />
      </div>
      <InfoTable
        label="Сведения о разрешении"
        rows={[
          {
            key: "app",
            icon: mdiApplicationOutline,
            label: "Приложение",
            value: grant.app,
            mono: true,
          },
          {
            key: "path",
            icon: mdiFolderOutline,
            label: "Путь",
            value: path,
            mono: grant.path.length > 0,
            muted: grant.path.length === 0,
          },
          {
            key: "scope",
            icon: mdiShieldCheckOutline,
            label: "Действует",
            value: appScopeLabel(grant),
          },
          ...(grant.conversation === null
            ? []
            : [
                {
                  key: "chat",
                  icon: mdiChatOutline,
                  label: "Диалог",
                  value: grant.conversation.title,
                },
              ]),
          {
            key: "granted",
            icon: mdiCalendarOutline,
            label: "Выдано",
            value: grantedLabel(grant),
          },
          {
            key: "used",
            icon: mdiClockOutline,
            label: "Последнее использование",
            value: appLastUsedLabel(grant, now),
            muted: grant.lastUsedAt === null,
          },
        ]}
      />
    </div>
  );
}

export default observer(AppGrantDetail);
