import { mdiTuneVariant } from "@mdi/js";
import { useState } from "react";
import HostCheck from "../features/diagnostics/HostCheck";
import ComponentsTab from "../features/toolchain/ComponentsTab";
import SegmentedControl from "../ui/atoms/SegmentedControl";
import PageShell from "../ui/templates/PageShell";

const TABS = [{ value: "components", label: "Компоненты" }] as const;

type TabValue = (typeof TABS)[number]["value"];

export default function SettingsPage() {
  const [tab, setTab] = useState<TabValue>("components");

  return (
    <PageShell
      icon={mdiTuneVariant}
      title="Настройки"
      subtitle="Конфигурация приложения"
      toolbar={
        <SegmentedControl
          label="Раздел настроек"
          value={tab}
          options={TABS}
          onChange={setTab}
          ghost={true}
        />
      }
    >
      {tab === "components" ? <ComponentsTab /> : null}
      <HostCheck />
    </PageShell>
  );
}
