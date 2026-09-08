import { mdiHistory } from "@mdi/js";
import PagePlaceholder from "../ui/molecules/PagePlaceholder";
import PageShell from "../ui/templates/PageShell";

export default function RunsPage() {
  return (
    <PageShell title="Запуски и логи" subtitle="История выполнений, шаги и журналы">
      <PagePlaceholder
        icon={mdiHistory}
        title="Запусков пока нет"
        description="История завершённых запусков, дерево шагов и журналы появятся здесь."
        task="TASK_026"
      />
    </PageShell>
  );
}
