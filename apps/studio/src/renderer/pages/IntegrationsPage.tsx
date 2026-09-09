import { mdiEmailOutline } from "@mdi/js";
import PagePlaceholder from "../ui/molecules/PagePlaceholder";
import PageShell from "../ui/templates/PageShell";

export default function IntegrationsPage() {
  return (
    <PageShell
      icon={mdiEmailOutline}
      title="Интеграции"
      subtitle="Почта, Telegram, git-хостинг и вебхуки"
    >
      <PagePlaceholder
        icon={mdiEmailOutline}
        title="Аккаунтов пока нет"
        description="Учётные записи, форма подключения и лента автоматизаций появятся здесь."
        task="TASK_051"
      />
    </PageShell>
  );
}
