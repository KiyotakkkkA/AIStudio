import { mdiTuneVariant } from "@mdi/js";
import { useState } from "react";
import HostCheck from "../features/diagnostics/HostCheck";
import EnvironmentTab from "../features/toolchain/EnvironmentTab";
import SegmentedControl from "../ui/atoms/SegmentedControl";
import PageShell from "../ui/templates/PageShell";

const TABS = [{ value: "environment", label: "Окружение" }] as const;

type TabValue = (typeof TABS)[number]["value"];

export default function SettingsPage() {
  const [tab, setTab] = useState<TabValue>("environment");

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
      {tab === "environment" ? <EnvironmentTab /> : null}
      <HostCheck />
    </PageShell>
  );
}
