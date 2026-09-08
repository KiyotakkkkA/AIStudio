import { mdiGraphOutline } from "@mdi/js";
import PagePlaceholder from "../ui/molecules/PagePlaceholder";
import PageShell from "../ui/templates/PageShell";

export default function ScenariosPage() {
  return (
    <PageShell title="Сценарии" subtitle="Узловые сценарии, версии и запуски">
      <PagePlaceholder
        icon={mdiGraphOutline}
        title="Сценариев пока нет"
        description="Холст сценария, инспектор узла и журнал последнего запуска появятся здесь."
        task="TASK_038"
      />
    </PageShell>
  );
}
