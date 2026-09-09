import { mdiDatabaseOutline } from "@mdi/js";
import PagePlaceholder from "../ui/molecules/PagePlaceholder";
import PageShell from "../ui/templates/PageShell";

export default function VectorStoresPage() {
  return (
    <PageShell
      icon={mdiDatabaseOutline}
      title="Векторные хранилища"
      subtitle="Коллекции, документы и тестовый поиск"
    >
      <PagePlaceholder
        icon={mdiDatabaseOutline}
        title="Хранилищ пока нет"
        description="Список хранилищ, детальная карточка и тестовый поиск появятся здесь."
        task="TASK_021"
      />
    </PageShell>
  );
}
