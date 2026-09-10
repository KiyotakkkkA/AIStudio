import { mdiAccountPlusOutline, mdiChevronDown } from "@mdi/js";
import { observer } from "mobx-react-lite";
import { useEffect, useRef, useState } from "react";
import type { AccountFamily } from "@zvs/shared";
import Button from "../../ui/atoms/Button";
import Icon from "../../ui/atoms/Icon";
import ConfirmDialog from "../../ui/molecules/ConfirmDialog";
import useStore from "../../stores/useStore";
import { adapterLabel } from "./accountPresentation";

export interface AccountLoginMenuProps {
  readonly tone?: "primary" | "secondary";
}

function AccountLoginMenu({ tone = "primary" }: AccountLoginMenuProps) {
  const { accounts } = useStore();
  const [open, setOpen] = useState(false);
  const [replacing, setReplacing] = useState<AccountFamily | null>(null);
  const root = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!open) return undefined;
    const close = (event: MouseEvent): void => {
      if (root.current?.contains(event.target as Node) === true) return;
      setOpen(false);
    };
    document.addEventListener("mousedown", close);
    return () => {
      document.removeEventListener("mousedown", close);
    };
  }, [open]);

  const start = (adapter: AccountFamily): void => {
    setOpen(false);
    if (accounts.accountFor(adapter) === null) {
      void accounts.link(adapter);
      return;
    }
    setReplacing(adapter);
  };

  return (
    <div ref={root} className="relative flex-none">
      <Button
        type="button"
        tone={tone}
        disabled={accounts.busy || accounts.families.length === 0}
        aria-expanded={open}
        onClick={() => {
          setOpen((current) => !current);
        }}
      >
        <Icon path={mdiAccountPlusOutline} size={15} />
        Войти
        <Icon path={mdiChevronDown} size={14} />
      </Button>

      {open ? (
        <div
          role="menu"
          aria-label="Выберите вендора"
          className="absolute top-full right-0 z-20 mt-1.5 flex w-56 flex-col gap-1 rounded-card border border-main-600 bg-main-800 p-1.5"
        >
          {accounts.families.map((family) => (
            <button
              key={family}
              type="button"
              role="menuitem"
              className="flex flex-col rounded-[6px] px-2.5 py-1.75 text-left hover:bg-main-700"
              onClick={() => {
                start(family);
              }}
            >
              <span className="text-[12.5px] font-medium text-main-100">
                {adapterLabel(family)}
              </span>
              <span className="text-[11px] text-main-400">
                {accounts.accountFor(family) === null
                  ? "Откроется сайт вендора"
                  : "Аккаунт уже привязан"}
              </span>
            </button>
          ))}
        </div>
      ) : null}

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
