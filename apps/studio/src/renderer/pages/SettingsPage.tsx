import { mdiPuzzleOutline } from "@mdi/js";
import { useState } from "react";
import HostCheck from "../features/diagnostics/HostCheck";
import ComponentsTab from "../features/toolchain/ComponentsTab";
import PageTabs from "../ui/molecules/PageTabs";
import PageShell from "../ui/templates/PageShell";

const TABS = [{ value: "components", label: "Компоненты" }] as const;

type TabValue = (typeof TABS)[number]["value"];

export default function SettingsPage() {
  const [tab, setTab] = useState<TabValue>("components");

  return (
    <PageShell
      icon={mdiPuzzleOutline}
      title="Расширения"
      subtitle="Компоненты и среды выполнения"
      toolbar={<PageTabs label="Раздел расширений" value={tab} tabs={TABS} onChange={setTab} />}
    >
      {tab === "components" ? <ComponentsTab /> : null}
      <HostCheck />
    </PageShell>
  );
}
