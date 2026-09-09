import { mdiClockOutline } from "@mdi/js";
import PagePlaceholder from "../ui/molecules/PagePlaceholder";
import PageShell from "../ui/templates/PageShell";

export default function TasksPage() {
  return (
    <PageShell
      icon={mdiClockOutline}
      title="Задачи"
      subtitle="Выполняется сейчас: очередь, расписание и подтверждения"
    >
      <PagePlaceholder
        icon={mdiClockOutline}
        title="Активных задач нет"
        description="Живой список запусков, очередь и блокирующие подтверждения появятся здесь."
        task="TASK_026"
      />
    </PageShell>
  );
}
