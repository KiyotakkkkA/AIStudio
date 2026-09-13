import { mdiAccountCircleOutline } from "@mdi/js";
import { Loader, ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import { observer } from "mobx-react-lite";
import { useEffect } from "react";
import type { AccountFamily } from "@zvs/shared";
import Button from "../../ui/atoms/Button";
import EmptyState from "../../ui/molecules/EmptyState";
import ListRowSkeleton from "../../ui/molecules/ListRowSkeleton";
import useAppNavigation from "../../hooks/useAppNavigation";
import { ROUTES } from "../../app/routes";
import useStore from "../../stores/useStore";
import AccountCard from "./AccountCard";
import AccountLoginMenu from "./AccountLoginMenu";
import { adapterLabel, secondsLeftLabel } from "./accountPresentation";

function AccountsTab() {
  const { accounts } = useStore();
  const navigation = useAppNavigation();
  const linking = accounts.linking;
  const now = Date.now();

  useEffect(() => {
    void accounts.load();
    const timer = setInterval(() => void accounts.load(), 30_000);
    return () => clearInterval(timer);
  }, [accounts]);

  const openSites = (): void => {
    navigation.go(ROUTES.browser);
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3">
      {accounts.accounts.length > 0 ? (
        <p className="text-[11.5px] text-main-400">
          Сессии проверяются автоматически; новые данные входа сохраняются, когда вендор их выдаёт.
        </p>
      ) : null}
      {accounts.error === null ? null : (
        <div
          role="alert"
          className="flex flex-none items-center gap-3 rounded-[6px] border border-err-border bg-main-800 px-3 py-2.25 text-[12px] text-err"
        >
          <span className="flex-1">{accounts.error}</span>
          <button type="button" className="text-main-400" onClick={accounts.dismissError}>
            Скрыть
          </button>
        </div>
      )}

      {linking === null || linking.phase === "cancelled" ? null : (
        <div className="flex flex-none flex-col gap-2.5 rounded-card border border-main-600 bg-main-750 px-4 py-3.5">
          {linking.phase === "waiting" ? (
            <>
              <div className="flex items-center gap-3">
                <Loader className="size-4 flex-none text-accent-dark" />
                <span className="flex-1 text-[12.5px] text-main-100">
                  Войдите в {adapterLabel(linking.adapter)} в браузере — мы ждём, пока вендор
                  подтвердит вход.
                </span>
                <Button
                  type="button"
                  tone="secondary"
                  onClick={() => {
                    void accounts.cancelLink();
                  }}
                >
                  Отменить
                </Button>
              </div>
              <span className="text-[11.5px] text-main-400">
                Пароль вводится только на сайте вендора, в обычной вкладке браузера.{" "}
                {secondsLeftLabel(linking.elapsedMs, linking.totalMs) ?? ""}
              </span>
            </>
          ) : null}

          {linking.phase === "timeout" ? (
            <div className="flex items-center gap-3">
              <span className="flex-1 text-[12.5px] text-warn">
                Вход не обнаружен. {adapterLabel(linking.adapter)} не подтвердил сессию за
                отведённое время.
              </span>
              <Button
                type="button"
                tone="secondary"
                onClick={() => {
                  void accounts.link(linking.adapter);
                }}
              >
                Попробовать снова
              </Button>
              <Button type="button" tone="ghost" onClick={accounts.dismissLinking}>
                Закрыть
              </Button>
            </div>
          ) : null}

          {linking.phase === "success" ? (
            <div className="flex items-center gap-3">
              <span className="flex-1 text-[12.5px] text-ok">
                {adapterLabel(linking.adapter)} привязан.
                {linking.detail === null ? "" : ` ${linking.detail}`}
              </span>
              <Button type="button" tone="ghost" onClick={accounts.dismissLinking}>
                Закрыть
              </Button>
            </div>
          ) : null}
        </div>
      )}

      <ScrollArea className="flex min-h-0 flex-1 flex-col gap-3 pr-0.5">
        {accounts.loading && accounts.accounts.length === 0 ? (
          <>
            <ListRowSkeleton />
            <ListRowSkeleton />
          </>
        ) : null}

        {accounts.isEmpty ? (
          <div className="flex flex-1 items-center justify-center rounded-card border border-dashed border-main-600 bg-main-800/40 px-3">
            <EmptyState
              icon={mdiAccountCircleOutline}
              title="Аккаунтов пока нет"
              description={`В режиме аккаунта ключ не нужен: вы входите на сайт вендора в интегрированном браузере, и приложение обращается к вендору с той же сессией. Поддерживаются ${vendorList(accounts.families)}. Не все способы входа поддерживаются, поэтому иногда придётся отказаться от входа через соцсети или Google.`}
              action={<AccountLoginMenu />}
            />
          </div>
        ) : null}

        <div className="grid gap-3 xl:grid-cols-2">
          {accounts.accounts.map((account) => (
            <AccountCard
              key={account.id}
              account={account}
              now={now}
              checking={accounts.refreshingId === account.id}
              unlinking={accounts.unlinkingId === account.id}
              onRelink={() => {
                void accounts.link(account.adapter);
              }}
              onRecheck={() => {
                void accounts.refresh(account.id);
              }}
              onUnlink={() => {
                void accounts.unlink(account.id);
              }}
              onOpenSites={openSites}
            />
          ))}
        </div>
      </ScrollArea>
    </div>
  );
}

function vendorList(families: readonly AccountFamily[]): string {
  if (families.length === 0) return "вендоры, у которых есть адаптер";
  return families.map((family) => adapterLabel(family)).join(", ");
}

export default observer(AccountsTab);
