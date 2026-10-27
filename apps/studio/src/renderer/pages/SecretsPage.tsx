import { mdiHistory, mdiKeyOutline } from "@mdi/js";
import { observer } from "mobx-react-lite";
import { useState } from "react";
import SecretsHeaderActions from "../features/secrets/SecretsHeaderActions";
import SecretsWorkspace from "../features/secrets/SecretsWorkspace";
import { useStore } from "../stores/useStore";
import EmptyState from "../ui/molecules/EmptyState";
import PageShell from "../ui/templates/PageShell";
import PageTabs from "../ui/molecules/PageTabs";
import { InlineSkeleton, PAGE_LOADING, awaitingData } from "../ui/molecules/LoadingSkeleton";

const TABS = [
  { value: "secrets", label: "Секреты" },
  { value: "access", label: "Журнал доступа" },
] as const;

type TabValue = (typeof TABS)[number]["value"];

function SecretsPage() {
  const { secrets } = useStore();
  const [tab, setTab] = useState<TabValue>("secrets");

  return (
    <PageShell
      bleed={tab === "secrets"}
      icon={mdiKeyOutline}
      title="Секреты"
      subtitle={
        awaitingData(secrets) ? (
          <InlineSkeleton label={PAGE_LOADING} className="w-24" />
        ) : (
          `${String(secrets.total)} секретов`
        )
      }
      actions={<SecretsHeaderActions />}
      toolbar={<PageTabs label="Область видимости" value={tab} tabs={TABS} onChange={setTab} />}
    >
      {tab === "secrets" ? <SecretsWorkspace /> : null}

      {tab === "access" ? (
        <div className="flex flex-1 items-center justify-center">
          <EmptyState
            icon={mdiHistory}
            title="Журнал доступа недоступен"
            description="История обращений к секретам пока не ведётся."
          />
        </div>
      ) : null}
    </PageShell>
  );
}

export default observer(SecretsPage);
