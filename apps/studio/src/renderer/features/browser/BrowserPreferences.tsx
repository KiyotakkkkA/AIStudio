import { observer } from "mobx-react-lite";
import { useEffect } from "react";
import { mdiWeb } from "@mdi/js";
import Icon from "../../ui/atoms/Icon";
import SelectInput from "../../ui/atoms/SelectInput";
import Toggle from "../../ui/atoms/Toggle";
import Alert from "../../ui/molecules/Alert";
import PreferenceContainer from "../../ui/molecules/PreferenceContainer";
import { useStore } from "../../stores/useStore";
import SettingRow from "../preferences/SettingRow";
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

const GeneralBlock = observer(function GeneralBlock() {
  const { browserSettings } = useStore();
  const { settings } = browserSettings;
  const disabled = !settings.enabled;
  return (
    <PreferenceContainer title={BROWSER_GENERAL_TITLE}>
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
    </PreferenceContainer>
  );
});

export default observer(function BrowserPreferences() {
  const { browserSettings } = useStore();
  useEffect(() => {
    void browserSettings.load();
  }, [browserSettings]);
  return (
    <div className="flex flex-col gap-6 pt-2">
      {browserSettings.error === null ? null : (
        <Alert onDismiss={browserSettings.dismissError}>{browserSettings.error}</Alert>
      )}
      <BrowserSwitch />
      <GeneralBlock />
    </div>
  );
});
