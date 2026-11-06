import { mdiPuzzleOutline } from "@mdi/js";
import PreferencesWorkspace from "../features/preferences/PreferencesWorkspace";
import {
  SETTINGS_PAGE_SUBTITLE,
  SETTINGS_PAGE_TITLE,
} from "../features/preferences/preferencesPresentation";
import PageShell from "../ui/templates/PageShell";

export default function SettingsPage() {
  return (
    <PageShell
      bleed={true}
      icon={mdiPuzzleOutline}
      title={SETTINGS_PAGE_TITLE}
      subtitle={SETTINGS_PAGE_SUBTITLE}
    >
      <PreferencesWorkspace />
    </PageShell>
  );
}
