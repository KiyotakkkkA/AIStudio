import { mdiPuzzleOutline } from "@mdi/js";
import HostCheck from "../features/diagnostics/HostCheck";
import PageShell from "../ui/templates/PageShell";

export default function SettingsPage() {
  return (
    <PageShell
      icon={mdiPuzzleOutline}
      title="Расширения"
      subtitle="Состояние подключения и диагностика приложения"
    >
      <HostCheck />
    </PageShell>
  );
}
