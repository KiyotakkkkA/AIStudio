import { mdiChatOutline } from "@mdi/js";
import PagePlaceholder from "../ui/molecules/PagePlaceholder";
import PageShell from "../ui/templates/PageShell";

export default function ChatPage() {
  return (
    <PageShell
      icon={mdiChatOutline}
      title="Чат"
      subtitle="Диалог с моделью, инструменты и цитаты из документов"
    >
      <PagePlaceholder
        icon={mdiChatOutline}
        title="Диалогов пока нет"
        description="Лента сообщений, композер и цитаты из векторного поиска появятся здесь."
        task="TASK_025"
      />
    </PageShell>
  );
}
