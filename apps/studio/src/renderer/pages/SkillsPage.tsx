import { mdiBookOpenOutline } from "@mdi/js";
import PagePlaceholder from "../ui/molecules/PagePlaceholder";
import PageShell from "../ui/templates/PageShell";

export default function SkillsPage() {
  return (
    <PageShell title="Навыки" subtitle="Определения навыков, триггеры и разрешения">
      <PagePlaceholder
        icon={mdiBookOpenOutline}
        title="Навыков пока нет"
        description="Список навыков, редактор определения и правила срабатывания появятся здесь."
        task="TASK_036"
      />
    </PageShell>
  );
}
