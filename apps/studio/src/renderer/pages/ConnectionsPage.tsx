import { mdiPowerPlugOutline } from "@mdi/js";
import PagePlaceholder from "../ui/molecules/PagePlaceholder";
import PageShell from "../ui/templates/PageShell";

export default function ConnectionsPage() {
  return (
    <PageShell
      icon={mdiPowerPlugOutline}
      title="Подключения"
      subtitle="MCP-серверы и обнаруженные инструменты"
    >
      <PagePlaceholder
        icon={mdiPowerPlugOutline}
        title="Подключений пока нет"
        description="Список MCP-серверов, обнаружение инструментов и проверка связи появятся здесь."
        task="TASK_033"
      />
    </PageShell>
  );
}
