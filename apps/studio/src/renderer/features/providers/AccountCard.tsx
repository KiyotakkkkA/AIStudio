import {
  mdiAccountPlusOutline,
  mdiAlertCircleOutline,
  mdiClockOutline,
  mdiCookieOutline,
  mdiLinkVariant,
  mdiLinkVariantOff,
  mdiRefresh,
} from "@mdi/js";
import { observer } from "mobx-react-lite";
import { useState, type ReactNode } from "react";
import type { AccountDto } from "@zvs/shared";
import Button from "../../ui/atoms/buttons/Button";
import IconButton from "../../ui/atoms/buttons/IconButton";
import Chip from "../../ui/atoms/Chip";
import Icon from "../../ui/atoms/Icon";
import StatusDot from "../../ui/atoms/StatusDot";
import { STATUS_TONE_TEXT, type StatusTone } from "../../ui/atoms/statusTone";
import VendorMark from "./VendorMark";
import {
  ACCOUNT_STATUS_SHORT,
  accountInitials,
  accountStatusLine,
  accountStatusTone,
  accountTitle,
  adapterLabel,
  expiryLine,
  providersWord,
} from "./accountPresentation";

const PILL_TONES: Record<StatusTone, string> = {
  ok: "bg-ok-soft text-ok",
  warn: "bg-warn-soft text-warn",
  err: "bg-err-soft text-err",
  idle: "bg-main-700 text-main-400",
  accent: "bg-accent-soft text-accent-medium",
};

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

function DetailRow({
  icon,
  label,
  tone = "idle",
  children,
}: {
  readonly icon: string;
  readonly label: string;
  readonly tone?: StatusTone;
  readonly children: ReactNode;
}) {
  return (
    <div className="flex min-w-0 items-center gap-2.5 px-3 py-2">
      <Icon path={icon} size={15} className="flex-none text-main-500" />
      <span className="w-24 flex-none text-[12px] text-main-400">{label}</span>
      <span
        className={`min-w-0 flex-1 truncate text-[12px] ${tone === "idle" ? "text-main-200" : STATUS_TONE_TEXT[tone]}`}
      >
        {children}
      </span>
    </div>
  );
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
  const detail = account.detail !== null && account.detail.length > 0 ? account.detail : null;

  return (
    <article
      className={`flex min-w-0 flex-col rounded-card border bg-main-900 shadow-card ${
        needsRelink ? "border-warn-line" : "border-line"
      }`}
    >
      <header className="flex min-w-0 items-center gap-3 px-4 pt-4 pb-3">
        <span className="relative flex-none">
          {showAvatar ? (
            <img
              src={account.avatarUrl ?? ""}
              alt=""
              referrerPolicy="no-referrer"
              className="size-10 rounded-control object-cover"
              onError={() => {
                setAvatarBroken(true);
              }}
            />
          ) : (
            <span className="flex size-10 items-center justify-center rounded-control bg-main-700 text-[13px] font-semibold text-main-200">
              {accountInitials(account)}
            </span>
          )}
          <VendorMark
            family={account.adapter}
            size="xs"
            className="absolute -right-1.5 -bottom-1.5 border-2 border-main-900"
          />
        </span>

        <div className="flex min-w-0 flex-1 flex-col gap-0.5">
          <div className="flex min-w-0 items-center gap-2">
            <span className="truncate text-[13.5px] font-semibold text-main-50">
              {accountTitle(account)}
            </span>
            <Chip tone="accent">{adapterLabel(account.adapter)}</Chip>
          </div>
          <span className="truncate text-[12px] text-main-400">
            {account.emailMasked ?? "Почта не раскрыта вендором"}
          </span>
        </div>

        <span
          className={`inline-flex h-6 flex-none items-center gap-1.5 rounded-full px-2.5 text-[11.5px] font-medium ${PILL_TONES[tone]}`}
          title={accountStatusLine(account)}
        >
          <StatusDot tone={tone} />
          {ACCOUNT_STATUS_SHORT[account.status]}
        </span>
      </header>

      <div className="mx-4 flex flex-col divide-y divide-line rounded-control bg-main-800">
        {detail === null ? null : (
          <DetailRow icon={mdiAlertCircleOutline} label="Состояние" tone={tone}>
            {detail}
          </DetailRow>
        )}
        <DetailRow icon={mdiClockOutline} label="Сессия" tone={expiry?.tone ?? "idle"}>
          {expiry === null ? "Срок не сообщается" : expiry.text}
        </DetailRow>
        <DetailRow icon={mdiLinkVariant} label="Подключения">
          {account.linkedProvidersCount === 0
            ? "Не используется"
            : `Используется: ${providersWord(account.linkedProvidersCount)}`}
        </DetailRow>
      </div>

      <footer className="mt-3 flex items-start gap-2 border-t border-line px-4 py-3">
        <div className="flex min-w-0 flex-1 flex-wrap items-center gap-2">
          {needsRelink ? (
            <Button tone="primary" disabled={unlinking} onClick={onRelink}>
              <Icon path={mdiAccountPlusOutline} size={15} />
              Перепривязать
            </Button>
          ) : (
            <Button tone="secondary" disabled={checking || unlinking} onClick={onRecheck}>
              <Icon path={mdiRefresh} size={15} className={checking ? "animate-spin" : undefined} />
              {checking ? "Проверка…" : "Проверить сессию"}
            </Button>
          )}
          <IconButton
            tone="danger"
            size="md"
            path={mdiLinkVariantOff}
            label={unlinking ? "Отвязка…" : "Отвязать"}
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
          />
        </div>
        <Button tone="ghost" onClick={onOpenSites}>
          <Icon path={mdiCookieOutline} size={15} />
          Сайты и куки
        </Button>
      </footer>
    </article>
  );
}

export default observer(AccountCard);
