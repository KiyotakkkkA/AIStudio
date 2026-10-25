import {
  mdiCalendarOutline,
  mdiChatOutline,
  mdiClockOutline,
  mdiFolderOutline,
  mdiFolderRemoveOutline,
  mdiShieldCheckOutline,
} from "@mdi/js";
import { observer } from "mobx-react-lite";
import type { FolderGrantDto } from "@zvs/shared";
import IconButton from "../../ui/atoms/buttons/IconButton";
import InfoTable from "../../ui/molecules/InfoTable";
import {
  REVOKE,
  REVOKE_TITLE,
  boundLabel,
  grantedLabel,
  lastUsedLabel,
  revokeConsequence,
  scopeLabel,
} from "./permissionsPresentation";

export interface FolderGrantDetailProps {
  readonly grant: FolderGrantDto;
  readonly now: number;
  readonly revoking: boolean;
  readonly onRevoke: () => void;
}

function FolderGrantDetail({ grant, now, revoking, onRevoke }: FolderGrantDetailProps) {
  const chats = grant.boundConversations.map((chat) => chat.title).join(", ");
  return (
    <div className="flex min-w-0 flex-1 flex-col gap-4 p-5">
      <div className="flex min-w-0 items-start gap-3">
        <div className="min-w-0 flex-1">
          <h2
            className="font-mono text-[14px] font-semibold break-all text-main-50"
            title={grant.path}
          >
            {grant.path}
          </h2>
          <p className="mt-1 text-[12px] text-main-400">{scopeLabel(grant)}</p>
        </div>
        <IconButton
          tone="danger"
          size="md"
          path={mdiFolderRemoveOutline}
          label={REVOKE}
          disabled={revoking}
          needConfirm={true}
          modalSetup={{
            title: REVOKE_TITLE,
            content: revokeConsequence(grant),
            confirmLabel: REVOKE,
          }}
          onClick={onRevoke}
        />
      </div>
      <InfoTable
        label="Сведения о разрешении"
        rows={[
          {
            key: "path",
            icon: mdiFolderOutline,
            label: "Папка",
            value: grant.path,
            hint: "Вместе со всеми вложенными папками",
            mono: true,
          },
          {
            key: "scope",
            icon: mdiShieldCheckOutline,
            label: "Действует",
            value: grant.conversation === null ? "Во всех диалогах" : scopeLabel(grant),
          },
          {
            key: "chats",
            icon: mdiChatOutline,
            label: "Диалоги с рабочей папкой здесь",
            value: chats.length === 0 ? "Нет" : chats,
            hint: chats.length === 0 ? undefined : boundLabel(grant),
            muted: chats.length === 0,
          },
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
            value: lastUsedLabel(grant, now),
            muted: grant.lastUsedAt === null,
          },
        ]}
      />
    </div>
  );
}

export default observer(FolderGrantDetail);
