import { observer } from "mobx-react-lite";
import { useEffect, useState } from "react";
import { mdiWeb } from "@mdi/js";
import Icon from "../../ui/atoms/Icon";
import SelectInput from "../../ui/atoms/SelectInput";
import Toggle from "../../ui/atoms/Toggle";
import Alert from "../../ui/molecules/Alert";
import PreferenceContainer from "../../ui/molecules/PreferenceContainer";
import { useStore } from "../../stores/useStore";
import SettingRow from "../preferences/SettingRow";
import SitePermissions from "./SitePermissions";
import DownloadHistory from "./DownloadHistory";
import BrowserExtensionsPage, { ExtensionsBlock } from "./BrowserExtensionsPage";
import { BookmarksPage, HistoryPage, ImportPage } from "./BrowserLibraryPages";
import {
  BOOKMARKS_HINT,
  BOOKMARKS_ID,
  BOOKMARKS_LABEL,
  HISTORY_HINT,
  HISTORY_ID,
  HISTORY_LABEL,
  IMPORT_OPEN,
} from "./browserLibraryCopy";
import { MANAGE } from "./browserDataCopy";
import Button from "../../ui/atoms/buttons/Button";
import { AutofillBlock, ClearDataRow, DownloadsBlock, type BrowserPage } from "./BrowserDataBlocks";
import { TOGGLE_OFF, TOGGLE_ON } from "../preferences/preferencesPresentation";
import {
  BROWSER_DISABLED_NOTE,
  BROWSER_ENABLED_HINT,
  BROWSER_ENABLED_ID,
  BROWSER_ENABLED_LABEL,
  BROWSER_GENERAL_TITLE,
  FULL_URL_HINT,
  FULL_URL_ID,
  FULL_URL_LABEL,
  LINK_TARGET_OPTIONS,
  LOCAL_LINKS_HINT,
  LOCAL_LINKS_ID,
  LOCAL_LINKS_LABEL,
  WEB_LINKS_HINT,
  WEB_LINKS_ID,
  WEB_LINKS_LABEL,
} from "./browserSettingsCopy";

const BrowserSwitch = observer(function BrowserSwitch() {
  const { browserSettings, ui } = useStore();
  const enabled = browserSettings.enabled;
  return (
    <div
      data-setting={BROWSER_ENABLED_ID}
      className={`flex items-center gap-3.5 rounded-card border border-line bg-main-850 px-4 py-3.5 transition-colors duration-500 ${
        ui.preferenceTarget === BROWSER_ENABLED_ID ? "bg-accent-soft" : ""
      }`}
    >
      <span className="flex size-10 flex-none items-center justify-center rounded-control bg-main-800 text-main-200">
        <Icon path={mdiWeb} size={20} />
      </span>
      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        <span className="text-[13px] font-medium text-main-100">{BROWSER_ENABLED_LABEL}</span>
        <span className="text-[12px] text-main-500">
          {enabled ? BROWSER_ENABLED_HINT : BROWSER_DISABLED_NOTE}
        </span>
      </div>
      <Toggle
        label={enabled ? TOGGLE_ON : TOGGLE_OFF}
        checked={enabled}
        disabled={!browserSettings.loaded}
        onChange={(value) => void browserSettings.update({ enabled: value })}
      />
    </div>
  );
});

const GeneralBlock = observer(function GeneralBlock({
  onManage,
}: {
  readonly onManage: (page: BrowserPage) => void;
}) {
  const { browserSettings } = useStore();
  const { settings } = browserSettings;
  const disabled = !settings.enabled;
  return (
    <PreferenceContainer
      title={BROWSER_GENERAL_TITLE}
      action={
        <Button tone="secondary" size="sm" onClick={() => onManage("import")}>
          {IMPORT_OPEN}
        </Button>
      }
    >
      <SettingRow id={WEB_LINKS_ID} label={WEB_LINKS_LABEL} hint={WEB_LINKS_HINT}>
        <div className="w-56">
          <SelectInput
            label={WEB_LINKS_LABEL}
            value={disabled ? "system" : settings.webLinks}
            options={LINK_TARGET_OPTIONS}
            disabled={disabled}
            onChange={(value) =>
              void browserSettings.update({ webLinks: value === "system" ? "system" : "studio" })
            }
            menuPlacement="bottom-right"
          />
        </div>
      </SettingRow>
      <SettingRow id={LOCAL_LINKS_ID} label={LOCAL_LINKS_LABEL} hint={LOCAL_LINKS_HINT}>
        <div className="w-56">
          <SelectInput
            label={LOCAL_LINKS_LABEL}
            value={disabled ? "system" : settings.localLinks}
            options={LINK_TARGET_OPTIONS}
            disabled={disabled}
            onChange={(value) =>
              void browserSettings.update({ localLinks: value === "system" ? "system" : "studio" })
            }
            menuPlacement="bottom-right"
          />
        </div>
      </SettingRow>
      <SettingRow id={FULL_URL_ID} label={FULL_URL_LABEL} hint={FULL_URL_HINT}>
        <Toggle
          label={settings.fullUrl ? TOGGLE_ON : TOGGLE_OFF}
          checked={settings.fullUrl}
          disabled={disabled}
          onChange={(value) => void browserSettings.update({ fullUrl: value })}
        />
      </SettingRow>
      <ClearDataRow />
      <SettingRow id={HISTORY_ID} label={HISTORY_LABEL} hint={HISTORY_HINT}>
        <Button tone="secondary" size="sm" onClick={() => onManage("history")}>
          {MANAGE}
        </Button>
      </SettingRow>
      <SettingRow id={BOOKMARKS_ID} label={BOOKMARKS_LABEL} hint={BOOKMARKS_HINT}>
        <Button tone="secondary" size="sm" onClick={() => onManage("bookmarks")}>
          {MANAGE}
        </Button>
      </SettingRow>
    </PreferenceContainer>
  );
});

export default observer(function BrowserPreferences() {
  const { browserSettings } = useStore();
  const [page, setPage] = useState<BrowserPage>("main");
  useEffect(() => {
    void browserSettings.load();
  }, [browserSettings]);
  const back = () => setPage("main");
  if (page === "downloads") return <DownloadHistory onBack={back} />;
  if (page === "history") return <HistoryPage onBack={back} />;
  if (page === "bookmarks") return <BookmarksPage onBack={back} />;
  if (page === "import") return <ImportPage onBack={back} />;
  if (page === "extensions") return <BrowserExtensionsPage onBack={back} />;
  return (
    <div className="flex flex-col gap-6 pt-2">
      {browserSettings.error === null ? null : (
        <Alert onDismiss={browserSettings.dismissError}>{browserSettings.error}</Alert>
      )}
      <BrowserSwitch />
      <GeneralBlock onManage={setPage} />
      <AutofillBlock />
      <ExtensionsBlock onManage={setPage} />
      <DownloadsBlock onManage={setPage} />
      <SitePermissions />
    </div>
  );
});
