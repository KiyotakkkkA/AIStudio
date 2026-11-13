import { observer } from "mobx-react-lite";
import { mdiDevices, mdiTune } from "@mdi/js";
import { useStore } from "../../stores/useStore";
import { useAppNavigation } from "../../hooks/useAppNavigation";
import Button from "../../ui/atoms/buttons/Button";
import Icon from "../../ui/atoms/Icon";
import Toggle from "../../ui/atoms/Toggle";
import Alert from "../../ui/molecules/Alert";
import PreferenceContainer from "../../ui/molecules/PreferenceContainer";
import SettingRow from "../preferences/SettingRow";
import { SETTINGS_MANAGE, TOGGLE_OFF, TOGGLE_ON } from "../preferences/preferencesPresentation";
import { DEVICE_APP_ACCESS_URL } from "../access/accessNavigation";
import {
  APP_PERMISSIONS_HINT,
  APP_PERMISSIONS_ID,
  APP_PERMISSIONS_TITLE,
  DEVICE_ENABLED_HINT,
  DEVICE_ENABLED_ID,
  DEVICE_ENABLED_LABEL,
  DEVICE_MAIN_TITLE,
} from "./deviceCopy";

function DevicePreferences() {
  const { appSettings, devicePermissions } = useStore();
  const navigation = useAppNavigation();
  const enabled = appSettings.values[DEVICE_ENABLED_ID];
  return (
    <div className="flex flex-col gap-4">
      <PreferenceContainer title={DEVICE_MAIN_TITLE}>
        <div
          data-setting={DEVICE_ENABLED_ID}
          className="flex items-center gap-3.5 border-b border-line py-3.5"
        >
          <span className="flex size-10 flex-none items-center justify-center rounded-control bg-main-800 text-main-200">
            <Icon path={mdiDevices} size={20} />
          </span>
          <div className="flex min-w-0 flex-1 flex-col gap-0.5">
            <span className="text-[13px] font-medium text-main-100">{DEVICE_ENABLED_LABEL}</span>
            <span className="text-[12px] text-main-500">{DEVICE_ENABLED_HINT}</span>
          </div>
          <Toggle
            label={enabled ? TOGGLE_ON : TOGGLE_OFF}
            checked={enabled}
            disabled={!appSettings.loaded || appSettings.pending[DEVICE_ENABLED_ID] !== undefined}
            onChange={(value) => void appSettings.set(DEVICE_ENABLED_ID, value)}
          />
        </div>
        {appSettings.errors[DEVICE_ENABLED_ID] === undefined ? null : (
          <Alert>{appSettings.errors[DEVICE_ENABLED_ID]}</Alert>
        )}
        <SettingRow
          id={APP_PERMISSIONS_ID}
          label={APP_PERMISSIONS_TITLE}
          hint={APP_PERMISSIONS_HINT}
        >
          <Button
            tone="primary"
            size="sm"
            onClick={() => {
              devicePermissions.select(null);
              devicePermissions.cancelAdding();
              navigation.go(DEVICE_APP_ACCESS_URL);
            }}
          >
            <Icon path={mdiTune} />
            {SETTINGS_MANAGE}
          </Button>
        </SettingRow>
      </PreferenceContainer>
    </div>
  );
}

export default observer(DevicePreferences);
