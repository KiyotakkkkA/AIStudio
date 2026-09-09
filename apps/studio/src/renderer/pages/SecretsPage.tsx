import { mdiEyeOutline, mdiHistory } from "@mdi/js";
import { observer } from "mobx-react-lite";
import { useState } from "react";
import SecretsHeaderActions from "../features/secrets/SecretsHeaderActions";
import SecretsWorkspace from "../features/secrets/SecretsWorkspace";
import useStore from "../stores/useStore";
import EmptyState from "../ui/molecules/EmptyState";
import PageShell from "../ui/templates/PageShell";

const TABS = [
  { value: "secrets", label: "Секреты" },
  { value: "shared", label: "Общая и публичная информация" },
  { value: "access", label: "Журнал доступа" },
] as const;

type TabValue = (typeof TABS)[number]["value"];

function SecretsPage() {
  const { secrets } = useStore();
  const [tab, setTab] = useState<TabValue>("secrets");

  return (
    <PageShell
      title="Секреты"
      subtitle={`${String(secrets.total)} секретов`}
      actions={<SecretsHeaderActions />}
      toolbar={TABS.map((entry) => (
        <button
          key={entry.value}
          type="button"
          onClick={() => {
            setTab(entry.value);
          }}
          className={`inline-flex h-[30px] items-center gap-[7px] rounded-[7px] px-[12px] text-[12.5px] font-medium ${
            tab === entry.value ? "bg-main-700 text-main-50" : "text-main-400 hover:text-main-200"
          }`}
        >
          {entry.label}
        </button>
      ))}
    >
      {tab === "secrets" ? <SecretsWorkspace /> : null}

      {tab === "shared" ? (
        <div className="flex flex-1 items-center justify-center rounded-[10px] border border-dashed border-main-600 bg-main-800/40">
          <EmptyState
            icon={mdiEyeOutline}
            title="Раздел ещё не описан"
            description="Что показывать про общие и публичные данные, пока не решено — задача в бэклоге не заведена."
          />
        </div>
      ) : null}

      {tab === "access" ? (
        <div className="flex flex-1 items-center justify-center rounded-[10px] border border-dashed border-main-600 bg-main-800/40">
          <EmptyState
            icon={mdiHistory}
            title="Журнал доступа ещё не описан"
            description="История обращений к секретам появится вместе с ядром исполнения — задача в бэклоге не заведена."
          />
        </div>
      ) : null}
    </PageShell>
  );
}

export default observer(SecretsPage);
