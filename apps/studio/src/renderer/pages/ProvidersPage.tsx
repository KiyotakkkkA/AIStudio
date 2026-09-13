import { mdiLayersOutline, mdiPlus, mdiRefresh } from "@mdi/js";
import { observer } from "mobx-react-lite";
import { useEffect } from "react";
import Button from "../ui/atoms/Button";
import Chip from "../ui/atoms/Chip";
import Icon from "../ui/atoms/Icon";
import AccountLoginMenu from "../features/providers/AccountLoginMenu";
import AccountsTab from "../features/providers/AccountsTab";
import ProvidersWorkspace from "../features/providers/ProvidersWorkspace";
import { accountsWord } from "../features/providers/accountPresentation";
import { modelsWord } from "../features/providers/providerPresentation";
import { ACCOUNTS_TAB, PROVIDERS_TABS, TAB_LABELS } from "../features/providers/providerTabs";
import { useStore } from "../stores/useStore";
import PageShell from "../ui/templates/PageShell";

function ProvidersPage() {
  const { providers, accounts } = useStore();
  const onAccounts = providers.tab === ACCOUNTS_TAB;

  useEffect(() => {
    void accounts.load();
  }, [accounts]);

  return (
    <PageShell
      icon={mdiLayersOutline}
      title="AI-провайдеры"
      subtitle={
        onAccounts
          ? `${accountsWord(accounts.accounts.length)} привязано`
          : `${String(providers.summaries.length)} подключений · ${modelsWord(providers.discoveredModelCount)} найдено`
      }
      actions={
        onAccounts ? (
          <AccountLoginMenu />
        ) : (
          <>
            <Button
              type="button"
              tone="secondary"
              disabled={providers.refreshing}
              onClick={() => {
                void providers.refreshAll();
              }}
            >
              <Icon path={mdiRefresh} size={15} />
              {providers.refreshing ? "Обновляем…" : "Обновить все"}
            </Button>
            <Button type="button" tone="primary" onClick={providers.requestCreate}>
              <Icon path={mdiPlus} size={15} />
              Новое подключение
            </Button>
          </>
        )
      }
      toolbar={
        <>
          {PROVIDERS_TABS.map((tab) => {
            const active = providers.tab === tab;
            const count = tab === ACCOUNTS_TAB ? accounts.accounts.length : providers.counts[tab];
            return (
              <button
                key={tab}
                type="button"
                aria-current={active ? "page" : undefined}
                className={`inline-flex h-7.5 items-center gap-1.75 rounded-lg px-3 text-[12.5px] font-medium ${
                  active ? "bg-main-700 text-main-50" : "text-main-400 hover:text-main-100"
                }`}
                onClick={() => {
                  providers.requestTab(tab);
                }}
              >
                {TAB_LABELS[tab]}
                <Chip tone={active ? "raised" : "neutral"}>{String(count)}</Chip>
              </button>
            );
          })}
        </>
      }
    >
      {onAccounts ? <AccountsTab /> : <ProvidersWorkspace />}
    </PageShell>
  );
}

export default observer(ProvidersPage);
