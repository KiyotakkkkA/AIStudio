import { mdiTuneVariant } from "@mdi/js";
import HostCheck from "../features/diagnostics/HostCheck";
import PagePlaceholder from "../ui/molecules/PagePlaceholder";
import PageShell from "../ui/templates/PageShell";

export default function SettingsPage() {
  return (
    <PageShell
      icon={mdiTuneVariant}
      title="Настройки"
      subtitle="Конфигурация приложения, пути и обслуживание"
    >
      <HostCheck />
      <PagePlaceholder
        icon={mdiTuneVariant}
        title="Настройки ещё не подключены"
        description="Разделы конфигурации, пути хранения и обслуживание базы появятся здесь."
        task="TASK_052"
      />
    </PageShell>
  );
}
