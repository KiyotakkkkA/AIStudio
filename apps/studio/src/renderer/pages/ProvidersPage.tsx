import { mdiLayersOutline } from "@mdi/js";
import PagePlaceholder from "../ui/molecules/PagePlaceholder";
import PageShell from "../ui/templates/PageShell";

export default function ProvidersPage() {
  return (
    <PageShell
      icon={mdiLayersOutline}
      title="AI-провайдеры"
      subtitle="Подключения к моделям генерации, эмбеддингов и изображений"
    >
      <PagePlaceholder
        icon={mdiLayersOutline}
        title="Провайдеры не настроены"
        description="Список подключений, форма с тремя вкладками и карточки моделей появятся здесь."
        task="TASK_016"
      />
    </PageShell>
  );
}
