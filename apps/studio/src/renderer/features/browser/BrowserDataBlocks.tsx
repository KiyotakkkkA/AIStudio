import { observer } from "mobx-react-lite";
import Button from "../../ui/atoms/buttons/Button";
import Toggle from "../../ui/atoms/Toggle";
import PreferenceContainer from "../../ui/molecules/PreferenceContainer";
import { useStore } from "../../stores/useStore";
import SettingRow from "../preferences/SettingRow";
import { TOGGLE_OFF, TOGGLE_ON } from "../preferences/preferencesPresentation";
import {
  ASK_DOWNLOAD_HINT,
  ASK_DOWNLOAD_ID,
  ASK_DOWNLOAD_LABEL,
  AUTOFILL_TITLE,
  CLEAR_DATA,
  CLEAR_DATA_CONSEQUENCE,
  CLEAR_DATA_HINT,
  CLEAR_DATA_ID,
  CLEAR_DATA_LABEL,
  CLEAR_DATA_TITLE,
  DOWNLOAD_DIR_CHANGE,
  DOWNLOAD_DIR_ID,
  DOWNLOAD_DIR_LABEL,
  DOWNLOAD_DIR_RESET,
  DOWNLOAD_DIR_SYSTEM,
  DOWNLOAD_HISTORY_HINT,
  DOWNLOAD_HISTORY_ID,
  DOWNLOAD_HISTORY_LABEL,
  DOWNLOADS_TITLE,
  FORMS_HINT,
  FORMS_ID,
  FORMS_LABEL,
  MANAGE,
  PASSWORDS_HINT,
  PASSWORDS_ID,
  PASSWORDS_LABEL,
} from "./browserDataCopy";

export type BrowserPage = "main" | "downloads" | "history" | "bookmarks" | "import";

export const ClearDataRow = observer(function ClearDataRow() {
  const { browserSettings } = useStore();
  return (
    <SettingRow
      id={CLEAR_DATA_ID}
      label={CLEAR_DATA_LABEL}
      hint={CLEAR_DATA_HINT}
      footer={
        browserSettings.notice === null ? undefined : (
          <p role="status" className="m-0 text-[12px] text-main-500">
            {browserSettings.notice}
          </p>
        )
      }
    >
      <Button
        tone="secondary"
        size="sm"
        disabled={browserSettings.clearing || !browserSettings.enabled}
        needConfirm
        modalSetup={{
          title: CLEAR_DATA_TITLE,
          content: CLEAR_DATA_CONSEQUENCE,
          confirmLabel: CLEAR_DATA,
        }}
        onClick={() => void browserSettings.clearData()}
      >
        {CLEAR_DATA}
      </Button>
    </SettingRow>
  );
});

export const AutofillBlock = observer(function AutofillBlock() {
  const { browserSettings } = useStore();
  const { settings } = browserSettings;
  return (
    <PreferenceContainer title={AUTOFILL_TITLE}>
      <SettingRow id={PASSWORDS_ID} label={PASSWORDS_LABEL} hint={PASSWORDS_HINT}>
        <Toggle
          label={settings.passwordAutosave ? TOGGLE_ON : TOGGLE_OFF}
          checked={settings.passwordAutosave}
          onChange={(value) => void browserSettings.update({ passwordAutosave: value })}
        />
      </SettingRow>
      <SettingRow id={FORMS_ID} label={FORMS_LABEL} hint={FORMS_HINT}>
        <Toggle
          label={settings.formAutofill ? TOGGLE_ON : TOGGLE_OFF}
          checked={settings.formAutofill}
          onChange={(value) => void browserSettings.update({ formAutofill: value })}
        />
      </SettingRow>
    </PreferenceContainer>
  );
});

export const DownloadsBlock = observer(function DownloadsBlock({
  onManage,
}: {
  readonly onManage: (page: BrowserPage) => void;
}) {
  const { browserSettings } = useStore();
  const { settings } = browserSettings;
  return (
    <PreferenceContainer title={DOWNLOADS_TITLE}>
      <SettingRow
        id={DOWNLOAD_DIR_ID}
        label={DOWNLOAD_DIR_LABEL}
        hint={settings.downloadDir ?? DOWNLOAD_DIR_SYSTEM}
      >
        <div className="flex items-center gap-2">
          {settings.downloadDir === null ? null : (
            <Button
              tone="ghost"
              size="sm"
              onClick={() => void browserSettings.update({ downloadDir: null })}
            >
              {DOWNLOAD_DIR_RESET}
            </Button>
          )}
          <Button tone="secondary" size="sm" onClick={() => void browserSettings.pickDownloadDir()}>
            {DOWNLOAD_DIR_CHANGE}
          </Button>
        </div>
      </SettingRow>
      <SettingRow id={ASK_DOWNLOAD_ID} label={ASK_DOWNLOAD_LABEL} hint={ASK_DOWNLOAD_HINT}>
        <Toggle
          label={settings.askDownload ? TOGGLE_ON : TOGGLE_OFF}
          checked={settings.askDownload}
          onChange={(value) => void browserSettings.update({ askDownload: value })}
        />
      </SettingRow>
      <SettingRow
        id={DOWNLOAD_HISTORY_ID}
        label={DOWNLOAD_HISTORY_LABEL}
        hint={DOWNLOAD_HISTORY_HINT}
      >
        <Button tone="secondary" size="sm" onClick={() => onManage("downloads")}>
          {MANAGE}
        </Button>
      </SettingRow>
    </PreferenceContainer>
  );
});
