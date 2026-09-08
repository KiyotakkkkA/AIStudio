import { mdiKeyOutline } from "@mdi/js";
import PagePlaceholder from "../ui/molecules/PagePlaceholder";
import PageShell from "../ui/templates/PageShell";

export default function SecretsPage() {
  return (
    <PageShell
      title="Секреты и профиль"
      subtitle="Ключи, учётные данные и профиль рабочего пространства"
    >
      <PagePlaceholder
        icon={mdiKeyOutline}
        title="Секретов пока нет"
        description="Хранилище секретов, форма по схеме провайдера и вкладки видимости появятся здесь."
        task="TASK_012"
      />
    </PageShell>
  );
}
