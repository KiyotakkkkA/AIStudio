import { mdiAccountPlusOutline, mdiChevronDown } from "@mdi/js";
import { Dropdown } from "@kiyotakkkka/zvs-uikit-lib";
import { observer } from "mobx-react-lite";
import { useState } from "react";
import type { AccountFamily } from "@zvs/shared";
import Icon from "../../ui/atoms/Icon";
import ConfirmDialog from "../../ui/molecules/ConfirmDialog";
import { useStore } from "../../stores/useStore";
import VendorMark from "./VendorMark";
import { adapterLabel, adapterDescription } from "./accountPresentation";

const TRIGGER = [
  "h-8 w-auto justify-center gap-1.75 rounded-control border-0 px-3.25",
  "bg-accent-dark text-[13px] font-semibold text-on-accent hover:bg-accent-medium",
  "disabled:cursor-not-allowed disabled:opacity-50",
].join(" ");

function AccountLoginMenu() {
  const { accounts } = useStore();
  const [replacing, setReplacing] = useState<AccountFamily | null>(null);
  const disabled = accounts.busy || accounts.families.length === 0;

  const start = (adapter: AccountFamily): void => {
    if (adapter === "openai" || accounts.accountFor(adapter) === null) {
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
          <span className="flex items-center gap-1.75 leading-none">
            <Icon path={mdiAccountPlusOutline} size={15} />
            Войти
          </span>
        </Dropdown.Trigger>

        <Dropdown.Menu
          role="menu"
          aria-label="Выберите вендора"
          rounded=""
          className="rounded-card border-line"
        >
          {accounts.families.map((family) => (
            <Dropdown.Item
              key={family}
              role="menuitem"
              rounded="rounded-lg"
              className="text-[13px] text-main-100"
              onClick={() => {
                start(family);
              }}
            >
              <span className="flex w-full items-center gap-2.5">
                <VendorMark family={family} size="sm" />
                <span className="flex min-w-0 flex-1 flex-col gap-0.5">
                  <span className="font-medium">{adapterLabel(family)}</span>
                  <span className="text-[11px] text-main-400">
                    {adapterDescription(family, accounts.accountFor(family) !== null)}
                  </span>
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
