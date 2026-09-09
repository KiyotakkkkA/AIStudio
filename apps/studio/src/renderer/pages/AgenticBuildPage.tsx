import { mdiCreationOutline } from "@mdi/js";
import PagePlaceholder from "../ui/molecules/PagePlaceholder";
import PageShell from "../ui/templates/PageShell";

export default function AgenticBuildPage() {
  return (
    <PageShell
      icon={mdiCreationOutline}
      title="Агенты"
      subtitle="План агента, поток вызовов инструментов и контекст сессии"
    >
      <PagePlaceholder
        icon={mdiCreationOutline}
        title="Сессий пока нет"
        description="План агента, поток вызовов инструментов и панель подтверждений появятся здесь."
        task="TASK_041"
      />
    </PageShell>
  );
}
