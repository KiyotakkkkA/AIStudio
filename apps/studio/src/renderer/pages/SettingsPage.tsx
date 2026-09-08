import { mdiTuneVariant } from "@mdi/js";
import PagePlaceholder from "../ui/molecules/PagePlaceholder";
import PageShell from "../ui/templates/PageShell";

export default function SettingsPage() {
  return (
    <PageShell title="Настройки" subtitle="Конфигурация приложения, пути и обслуживание">
      <PagePlaceholder
        icon={mdiTuneVariant}
        title="Настройки ещё не подключены"
        description="Разделы конфигурации, пути хранения и обслуживание базы появятся здесь."
        task="TASK_052"
      />
    </PageShell>
  );
}
