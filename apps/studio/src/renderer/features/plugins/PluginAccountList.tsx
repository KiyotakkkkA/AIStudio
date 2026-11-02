import { observer } from "mobx-react-lite";
import { useNavigate } from "react-router-dom";
import Chip from "../../ui/atoms/Chip";
import StatusDot from "../../ui/atoms/StatusDot";
import { STATUS_TONE_TEXT } from "../../ui/atoms/statusTone";
import EntityListPanel from "../../ui/molecules/EntityListPanel";
import ListRow from "../../ui/molecules/ListRow";
import PluginMark from "./PluginMark";
import { accountPath } from "./pluginRoutes";
import type PluginStore from "./PluginStore";
import { signInTypeOf } from "./signInTypes";

export const ACCOUNTS_HEADING = "Аккаунты";
export const NEW_ACCOUNT_NAME = "Новый аккаунт";
export const NOT_SAVED = "Не сохранён";

function PluginAccountList({ store }: { readonly store: PluginStore }) {
  const navigate = useNavigate();
  const kind = store.kind;
  if (kind === null) return null;
  const accounts = store.accountsOf(kind);
  const draft = store.form?.isNew === true ? store.form : null;

  return (
    <EntityListPanel heading={ACCOUNTS_HEADING}>
      {accounts.map((account) => {
        const type = signInTypeOf(account);
        const state = store.stateOf(account);
        const tone = store.stateTone(account);
        const line =
          state === "running"
            ? (store.healthLine(account) ?? store.stateLabel(account))
            : store.stateLabel(account);
        return (
          <div key={account.id} className={state === "disabled" ? "opacity-55" : undefined}>
            <ListRow
              selected={store.accountId === account.id}
              leading={<PluginMark kind={kind} mark={type.mark} size="sm" />}
              onClick={() => {
                void navigate(accountPath(kind, account.id));
              }}
            >
              <span className="flex min-w-0 items-center gap-2">
                <StatusDot tone={tone} title={store.stateLabel(account)} />
                <span className="flex-1 truncate text-[12.5px] font-medium text-main-50">
                  {account.name}
                </span>
                <Chip>{type.chip}</Chip>
              </span>
              <span
                className={`truncate text-[11px] ${tone === "ok" || tone === "idle" ? "text-main-400" : STATUS_TONE_TEXT[tone]}`}
              >
                {line}
              </span>
            </ListRow>
          </div>
        );
      })}

      {draft === null ? null : (
        <ListRow selected leading={<PluginMark kind={kind} mark={draft.type.mark} size="sm" />}>
          <span className="flex min-w-0 items-center gap-2">
            <StatusDot tone="idle" />
            <span className="flex-1 truncate text-[12.5px] font-medium text-main-50">
              {draft.name.trim().length > 0 ? draft.name : NEW_ACCOUNT_NAME}
            </span>
            <Chip>{draft.type.chip}</Chip>
          </span>
          <span className="truncate text-[11px] text-main-400">{NOT_SAVED}</span>
        </ListRow>
      )}
    </EntityListPanel>
  );
}

export default observer(PluginAccountList);
