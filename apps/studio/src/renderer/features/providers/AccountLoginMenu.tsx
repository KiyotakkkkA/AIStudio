import { mdiAccountPlusOutline, mdiChevronDown } from "@mdi/js";
import { Dropdown } from "@kiyotakkkka/zvs-uikit-lib";
import { observer } from "mobx-react-lite";
import { useState } from "react";
import type { AccountFamily } from "@zvs/shared";
import Icon from "../../ui/atoms/Icon";
import ConfirmDialog from "../../ui/molecules/ConfirmDialog";
import { useStore } from "../../stores/useStore";
import { adapterLabel } from "./accountPresentation";

const TRIGGER = [
  "h-8 w-auto justify-center gap-1.75 rounded-[6px] border-0 px-3.25",
  "bg-accent-dark text-[12.5px] font-semibold text-main-900 hover:bg-accent-medium",
  "disabled:cursor-not-allowed disabled:opacity-50",
].join(" ");

function AccountLoginMenu() {
  const { accounts } = useStore();
  const [replacing, setReplacing] = useState<AccountFamily | null>(null);
  const disabled = accounts.busy || accounts.families.length === 0;

  const start = (adapter: AccountFamily): void => {
    if (accounts.accountFor(adapter) === null) {
      void accounts.link(adapter);
      return;
    }
    setReplacing(adapter);
  };

  return (
    <div className="flex-none">
      <Dropdown disabled={disabled} menuWidth={240} menuPlacement="bottom-right">
        <Dropdown.Trigger
          className={TRIGGER}
          rounded=""
          disabled={disabled}
          icon={<Icon path={mdiChevronDown} size={14} />}
        >
          <span className="inline-flex items-center gap-1.75">
            <Icon path={mdiAccountPlusOutline} size={15} />
            Войти
          </span>
        </Dropdown.Trigger>

        <Dropdown.Menu
          role="menu"
          aria-label="Выберите вендора"
          rounded=""
          className="rounded-card border-main-600"
        >
          {accounts.families.map((family) => (
            <Dropdown.Item
              key={family}
              role="menuitem"
              rounded="rounded-md"
              className="text-[12.5px] text-main-100"
              onClick={() => {
                start(family);
              }}
            >
              <span className="flex w-full items-baseline justify-between gap-2">
                <span className="font-medium">{adapterLabel(family)}</span>
                <span className="text-[11px] text-main-400">
                  {accounts.accountFor(family) === null ? "Откроется сайт вендора" : "Уже привязан"}
                </span>
              </span>
            </Dropdown.Item>
          ))}
        </Dropdown.Menu>
      </Dropdown>

      <ConfirmDialog
        open={replacing !== null}
        title="Заменить привязанный аккаунт?"
        confirmLabel="Войти заново"
        cancelLabel="Отмена"
        onCancel={() => {
          setReplacing(null);
        }}
        onConfirm={() => {
          const adapter = replacing;
          setReplacing(null);
          if (adapter !== null) void accounts.link(adapter);
        }}
      >
        {replacing === null ? null : (
          <>
            Вход под другим пользователем заменит привязанный аккаунт {adapterLabel(replacing)} —
            сейчас поддерживается только одна сессия на вендора. Подключения, использующие прежний
            аккаунт, останутся без него, пока вы не выберете новый.
          </>
        )}
      </ConfirmDialog>
    </div>
  );
}

export default observer(AccountLoginMenu);
