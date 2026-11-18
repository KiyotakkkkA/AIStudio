import { mdiAccountCircleOutline, mdiCancel, mdiShieldSyncOutline } from "@mdi/js";
import { Loader, ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import { observer } from "mobx-react-lite";
import { useEffect } from "react";
import type { AccountDto, AccountFamily } from "@zvs/shared";
import Button from "../../ui/atoms/buttons/Button";
import Icon from "../../ui/atoms/Icon";
import EmptyState from "../../ui/molecules/EmptyState";
import { useAppNavigation } from "../../hooks/useAppNavigation";
import { ROUTES } from "../../app/routes";
import { useStore } from "../../stores/useStore";
import { BROWSER_ENABLED_ID } from "../browser/browserSettingsCopy";
import AccountCard from "./AccountCard";
import AccountLoginMenu from "./AccountLoginMenu";
import { accountNeedsBrowser, adapterLabel, secondsLeftLabel } from "./accountPresentation";
import Alert from "../../ui/molecules/Alert";
import { ListSkeleton } from "../../ui/organisms/PageSkeletons";
import { awaitingData } from "../../ui/molecules/LoadingSkeleton";

function AccountsTab() {
  const { accounts, browserSettings, ui } = useStore();
  const navigation = useAppNavigation();
  const linking = accounts.linking;
  const now = Date.now();

  useEffect(() => {
    void accounts.load();
    const timer = setInterval(() => void accounts.load(), 30_000);
    return () => clearInterval(timer);
  }, [accounts]);

  const openSites = (): void => {
    if (browserSettings.enabled) navigation.go(ROUTES.browser);
    else ui.openPreferences("browser");
  };

  const renderAccount = (account: AccountDto) => (
    <AccountCard
      key={account.id}
      account={account}
      now={now}
      checking={accounts.refreshingId === account.id}
      unlinking={accounts.unlinkingId === account.id}
      onRelink={() => {
        if (accountNeedsBrowser(account.adapter) && !browserSettings.enabled) {
          ui.revealPreference("browser", BROWSER_ENABLED_ID);
        } else {
          void accounts.link(account.adapter, account.id);
        }
      }}
      onRecheck={() => {
        void accounts.refresh(account.id);
      }}
      onUnlink={() => {
        void accounts.unlink(account.id);
      }}
      onOpenSites={openSites}
    />
  );

  const independentAccounts = accounts.accounts.filter(
    (account) => !accountNeedsBrowser(account.adapter),
  );
  const browserAccounts = accounts.accounts.filter((account) =>
    accountNeedsBrowser(account.adapter),
  );

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3">
      {accounts.accounts.length > 0 ? (
        <p className="flex items-center gap-2 text-[12px] text-main-400">
          <Icon path={mdiShieldSyncOutline} size={15} className="flex-none text-main-500" />
          Сессии проверяются автоматически; новые данные входа сохраняются, когда вендор их выдаёт.
        </p>
      ) : null}
      {accounts.error === null ? null : (
        <Alert onDismiss={accounts.dismissError}>{accounts.error}</Alert>
      )}

      {linking === null || linking.phase === "cancelled" ? null : (
        <div className="flex flex-none flex-col gap-2.5 rounded-card border border-line bg-main-850 px-4 py-3.5">
          {linking.phase === "waiting" ? (
            <>
              <div className="flex items-center gap-3">
                <Loader className="size-4 flex-none text-accent-dark" />
                <span className="flex-1 text-[13px] text-main-100">
                  Войдите в {adapterLabel(linking.adapter)} в браузере — мы ждём, пока вендор
                  подтвердит вход.
                </span>
                <Button
                  tone="secondary"
                  onClick={() => {
                    void accounts.cancelLink();
                  }}
                >
                  Отменить
                </Button>
              </div>
              <span className="text-[12px] text-main-400">
                Пароль вводится только на сайте вендора, в обычной вкладке браузера.{" "}
                {secondsLeftLabel(linking.elapsedMs, linking.totalMs) ?? ""}
              </span>
            </>
          ) : null}

          {linking.phase === "timeout" ? (
            <div className="flex items-center gap-3">
              <span className="flex-1 text-[13px] text-warn">
                Вход не обнаружен. {adapterLabel(linking.adapter)} не подтвердил сессию за
                отведённое время.
              </span>
              <Button
                tone="secondary"
                onClick={() => {
                  void accounts.link(linking.adapter);
                }}
              >
                Попробовать снова
              </Button>
              <Button tone="ghost" onClick={accounts.dismissLinking}>
                Закрыть
              </Button>
            </div>
          ) : null}

          {linking.phase === "success" ? (
            <div className="flex items-center gap-3">
              <span className="flex-1 text-[13px] text-ok">
                {adapterLabel(linking.adapter)} привязан.
                {linking.detail === null ? "" : ` ${linking.detail}`}
              </span>
              <Button tone="ghost" onClick={accounts.dismissLinking}>
                Закрыть
              </Button>
            </div>
          ) : null}
        </div>
      )}

      <ScrollArea className="flex min-h-0 flex-1 flex-col gap-3 pr-0.5">
        {awaitingData(accounts) ? <ListSkeleton rows={3} /> : null}

        {independentAccounts.length > 0 ? (
          <div className="grid gap-3 xl:grid-cols-2">{independentAccounts.map(renderAccount)}</div>
        ) : null}

        {!browserSettings.enabled ? (
          <div className="flex flex-1 items-center justify-center rounded-card bg-main-800 px-3">
            <EmptyState
              icon={mdiCancel}
              title="Встроенный браузер выключен"
              description="Для управления аккаунтами Qwen и Z.ai включите встроенный браузер."
              action={
                <Button
                  tone="ghost"
                  onClick={() => ui.revealPreference("browser", BROWSER_ENABLED_ID)}
                >
                  Перейти к настройке браузера
                </Button>
              }
            />
          </div>
        ) : null}

        {accounts.isEmpty && browserSettings.enabled ? (
          <div className="flex flex-1 items-center justify-center rounded-card bg-main-800 px-3">
            <EmptyState
              icon={mdiAccountCircleOutline}
              title="Аккаунтов пока нет"
              description={`Привяжите аккаунт для работы без API-ключа. OpenAI запрашивает разрешение на использование подписки ChatGPT в системном браузере. Остальные аккаунты используют встроенный браузер. Поддерживаются ${vendorList(accounts.families)}.`}
              action={<AccountLoginMenu />}
            />
          </div>
        ) : null}

        {browserSettings.enabled && browserAccounts.length > 0 ? (
          <div className="grid gap-3 xl:grid-cols-2">{browserAccounts.map(renderAccount)}</div>
        ) : null}
      </ScrollArea>
    </div>
  );
}

function vendorList(families: readonly AccountFamily[]): string {
  if (families.length === 0) return "вендоры, у которых есть адаптер";
  return families.map((family) => adapterLabel(family)).join(", ");
}

export default observer(AccountsTab);
