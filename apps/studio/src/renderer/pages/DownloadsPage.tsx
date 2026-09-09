import { mdiTrayArrowDown } from "@mdi/js";
import PagePlaceholder from "../ui/molecules/PagePlaceholder";
import PageShell from "../ui/templates/PageShell";

export default function DownloadsPage() {
  return (
    <PageShell
      icon={mdiTrayArrowDown}
      title="Загрузки"
      subtitle="Очередь загрузок и локальный каталог моделей"
    >
      <PagePlaceholder
        icon={mdiTrayArrowDown}
        title="Загрузок пока нет"
        description="Очередь загрузок, докачка и учёт занятого места появятся здесь."
        task="TASK_030"
      />
    </PageShell>
  );
}
