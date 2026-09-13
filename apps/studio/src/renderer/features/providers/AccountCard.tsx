import { mdiAccountPlusOutline, mdiLinkVariantOff, mdiRefresh } from "@mdi/js";
import { observer } from "mobx-react-lite";
import { useState } from "react";
import type { AccountDto } from "@zvs/shared";
import Button from "../../ui/atoms/Button";
import Chip from "../../ui/atoms/Chip";
import Icon from "../../ui/atoms/Icon";
import StatusDot from "../../ui/atoms/StatusDot";
import { STATUS_TONE_TEXT } from "../../ui/atoms/statusTone";
import {
  accountInitials,
  accountStatusLine,
  accountStatusTone,
  accountTitle,
  adapterLabel,
  expiryLine,
  providersWord,
} from "./accountPresentation";

interface AccountCardProps {
  readonly account: AccountDto;
  readonly now: number;
  readonly checking: boolean;
  readonly unlinking: boolean;
  readonly onRelink: () => void;
  readonly onRecheck: () => void;
  readonly onUnlink: () => void;
  readonly onOpenSites: () => void;
}

function AccountCard({
  account,
  now,
  checking,
  unlinking,
  onRelink,
  onRecheck,
  onUnlink,
  onOpenSites,
}: AccountCardProps) {
  const [avatarBroken, setAvatarBroken] = useState(false);
  const needsRelink = account.status !== "linked";
  const tone = accountStatusTone(account, now);
  const expiry = expiryLine(account, now);
  const showAvatar = account.avatarUrl !== null && !avatarBroken;

  return (
    <div
      className={`flex min-w-0 flex-col gap-3 rounded-card border bg-main-900 p-4 ${
        needsRelink ? "border-warning-dark" : "border-main-700"
      }`}
    >
      <div className="flex min-w-0 items-start gap-3">
        {showAvatar ? (
          <img
            src={account.avatarUrl ?? ""}
            alt=""
            referrerPolicy="no-referrer"
            className="size-9 flex-none rounded-lg object-cover"
            onError={() => {
              setAvatarBroken(true);
            }}
          />
        ) : (
          <span className="flex size-9 flex-none items-center justify-center rounded-lg bg-main-700 text-[12px] font-semibold text-accent-medium">
            {accountInitials(account)}
          </span>
        )}

        <div className="flex min-w-0 flex-1 flex-col gap-1">
          <div className="flex min-w-0 items-center gap-2">
            <span className="truncate text-[13px] font-semibold text-main-50">
              {accountTitle(account)}
            </span>
            <Chip tone="accent">{adapterLabel(account.adapter)}</Chip>
          </div>
          <span className="truncate text-[11.5px] text-main-400">
            {account.emailMasked ?? "Почта не раскрыта вендором"}
          </span>
        </div>

        <span className="flex flex-none items-center pt-1.5">
          <StatusDot tone={tone} title={accountStatusLine(account)} />
        </span>
      </div>

      <div className="flex flex-col gap-1 text-[11.5px]">
        <span className={STATUS_TONE_TEXT[tone]}>{accountStatusLine(account)}</span>
        {expiry === null ? null : (
          <span className={STATUS_TONE_TEXT[expiry.tone]}>{expiry.text}</span>
        )}
        <span className="text-main-400">
          {account.linkedProvidersCount === 0
            ? "Подключения не используют этот аккаунт"
            : `Используется: ${providersWord(account.linkedProvidersCount)}`}
        </span>
      </div>

      <div className="flex flex-wrap items-center gap-2">
        {needsRelink ? (
          <Button type="button" tone="primary" disabled={unlinking} onClick={onRelink}>
            <Icon path={mdiAccountPlusOutline} size={15} />
            Перепривязать
          </Button>
        ) : (
          <Button
            type="button"
            tone="secondary"
            disabled={checking || unlinking}
            onClick={onRecheck}
          >
            <Icon path={mdiRefresh} size={15} />
            {checking ? "Проверяем…" : "Проверить сессию"}
          </Button>
        )}
        <Button
          type="button"
          tone="danger"
          disabled={checking || unlinking}
          needConfirm
          modalSetup={{
            title: "Отвязать аккаунт?",
            content: (
              <>
                {account.linkedProvidersCount === 0
                  ? "Ни одно подключение не использует этот аккаунт."
                  : `${providersWord(account.linkedProvidersCount)} перестанут работать и покажут «аккаунт не привязан».`}{" "}
                Настройки моделей и модель по умолчанию сохранятся — подключения не удаляются.
                Сессия {adapterLabel(account.adapter)} в браузере останется активной: выйти из неё
                можно только в разделе «Сайты и куки».
              </>
            ),
            tone: "danger",
            confirmLabel: "Отвязать",
          }}
          onClick={onUnlink}
        >
          <Icon path={mdiLinkVariantOff} size={15} />
          {unlinking ? "Отвязываем…" : "Отвязать"}
        </Button>
        <button
          type="button"
          className="text-[11.5px] text-main-400 hover:text-main-100"
          onClick={onOpenSites}
        >
          Сайты и куки
        </button>
      </div>
    </div>
  );
}

export default observer(AccountCard);
