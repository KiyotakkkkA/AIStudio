import { mdiWrenchOutline } from "@mdi/js";
import PagePlaceholder from "../ui/molecules/PagePlaceholder";
import PageShell from "../ui/templates/PageShell";

export default function ToolsPage() {
  return (
    <PageShell title="Инструменты" subtitle="Реестр инструментов и потолок разрешений">
      <PagePlaceholder
        icon={mdiWrenchOutline}
        title="Инструментов пока нет"
        description="Реестр инструментов из всех источников и режимы auto / ask / off появятся здесь."
        task="TASK_034"
      />
    </PageShell>
  );
}
