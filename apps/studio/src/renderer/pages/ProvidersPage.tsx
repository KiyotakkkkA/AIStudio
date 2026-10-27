import { mdiLayersOutline, mdiPlus, mdiRefresh } from "@mdi/js";
import { observer } from "mobx-react-lite";
import { useEffect } from "react";
import Button from "../ui/atoms/buttons/Button";
import Icon from "../ui/atoms/Icon";
import AccountLoginMenu from "../features/providers/AccountLoginMenu";
import AccountsTab from "../features/providers/AccountsTab";
import ProvidersWorkspace from "../features/providers/ProvidersWorkspace";
import { accountsWord } from "../features/providers/accountPresentation";
import { modelsWord } from "../features/providers/providerPresentation";
import { ACCOUNTS_TAB, PROVIDERS_TABS, TAB_LABELS } from "../features/providers/providerTabs";
import { useStore } from "../stores/useStore";
import PageShell from "../ui/templates/PageShell";
import PageTabs from "../ui/molecules/PageTabs";
import { InlineSkeleton, PAGE_LOADING, awaitingData } from "../ui/molecules/LoadingSkeleton";

function ProvidersPage() {
  const { providers, accounts } = useStore();
  const onAccounts = providers.tab === ACCOUNTS_TAB;

  useEffect(() => {
    void accounts.load();
  }, [accounts]);

  return (
    <PageShell
      bleed={!onAccounts}
      icon={mdiLayersOutline}
      title="AI-провайдеры"
      subtitle={
        awaitingData(onAccounts ? accounts : providers) ? (
          <InlineSkeleton label={PAGE_LOADING} className="w-48" />
        ) : onAccounts ? (
          `${accountsWord(accounts.accounts.length)} привязано`
        ) : (
          `${String(providers.summaries.length)} подключений · ${modelsWord(providers.discoveredModelCount)} найдено`
        )
      }
      actions={
        onAccounts ? (
          <AccountLoginMenu />
        ) : (
          <>
            <Button
              tone="secondary"
              disabled={providers.refreshing}
              onClick={() => {
                void providers.refreshAll();
              }}
            >
              <Icon path={mdiRefresh} size={15} />
              {providers.refreshing ? "Обновляем…" : "Обновить все"}
            </Button>
            <Button tone="primary" onClick={providers.requestCreate}>
              <Icon path={mdiPlus} size={15} />
              Новое подключение
            </Button>
          </>
        )
      }
      toolbar={
        <PageTabs
          label="Раздел провайдеров"
          value={providers.tab}
          onChange={(tab) => {
            providers.requestTab(tab);
          }}
          tabs={PROVIDERS_TABS.map((tab) => {
            const source = tab === ACCOUNTS_TAB ? accounts : providers;
            return {
              value: tab,
              label: TAB_LABELS[tab],
              count: awaitingData(source)
                ? null
                : tab === ACCOUNTS_TAB
                  ? accounts.accounts.length
                  : providers.counts[tab],
            };
          })}
        />
      }
    >
      {onAccounts ? <AccountsTab /> : <ProvidersWorkspace />}
    </PageShell>
  );
}

export default observer(ProvidersPage);
