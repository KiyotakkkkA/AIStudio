import { useEffect, useState } from "react";
import { observer } from "mobx-react-lite";
import { InputCheckBox } from "@kiyotakkkka/zvs-uikit-lib";
import {
  mdiApplicationOutline,
  mdiCalendarOutline,
  mdiClockOutline,
  mdiChatOutline,
  mdiDevices,
  mdiFolderOutline,
  mdiOpenInNew,
  mdiPlus,
  mdiTune,
  mdiShieldRemoveOutline,
} from "@mdi/js";
import type { AppGrantDto } from "@zvs/shared";
import { useStore } from "../../stores/useStore";
import { useAppNavigation } from "../../hooks/useAppNavigation";
import { ROUTES } from "../../app/routes";
import Button from "../../ui/atoms/buttons/Button";
import IconButton from "../../ui/atoms/buttons/IconButton";
import Icon from "../../ui/atoms/Icon";
import Toggle from "../../ui/atoms/Toggle";
import SelectInput from "../../ui/atoms/SelectInput";
import Alert from "../../ui/molecules/Alert";
import InfoTable from "../../ui/molecules/InfoTable";
import PreferenceContainer from "../../ui/molecules/PreferenceContainer";
import SettingsSubPage from "../preferences/SettingsSubPage";
import SettingRow from "../preferences/SettingRow";
import { SETTINGS_MANAGE, TOGGLE_OFF, TOGGLE_ON } from "../preferences/preferencesPresentation";
import { grantedLabel } from "../permissions/permissionsPresentation";
import { formatLastUsed } from "../tools/toolPresentation";
import AppIcon from "../computer/AppIcon";
import type { AppPermissionGroup } from "./DevicePermissionsStore";
import {
  ALWAYS_SECTION,
  APP_ADD,
  APP_CANCEL,
  APP_DETAILS,
  APP_DIALOGS,
  APP_DIALOGS_EMPTY,
  APP_OPEN_DIALOG,
  APP_PATH_UNKNOWN,
  APP_PERMISSIONS_EMPTY,
  APP_PERMISSIONS_HINT,
  APP_PERMISSIONS_ID,
  APP_PERMISSIONS_TITLE,
  APP_SAVE,
  APP_SCOPE_OPTIONS,
  DEVICE_ENABLED_HINT,
  DEVICE_ENABLED_ID,
  DEVICE_ENABLED_LABEL,
  DIALOG_SECTION,
  REVOKE,
  REVOKE_TITLE,
  appRevokeConsequence,
} from "./deviceCopy";

const RevokeButton = observer(function RevokeButton({ grant }: { readonly grant: AppGrantDto }) {
  const { devicePermissions: store } = useStore();
  return (
    <IconButton
      path={mdiShieldRemoveOutline}
      label={REVOKE}
      tone="danger"
      size="sm"
      disabled={store.busy}
      needConfirm
      modalSetup={{
        title: REVOKE_TITLE,
        content: appRevokeConsequence(grant),
        confirmLabel: REVOKE,
      }}
      onClick={() => void store.revoke(grant.id)}
    />
  );
});

const AddPermission = observer(function AddPermission() {
  const { devicePermissions: store } = useStore();
  if (store.draftPath === null) return null;
  return (
    <form
      className="flex flex-col gap-3 rounded-card border border-line bg-main-850 p-4"
      onSubmit={(event) => {
        event.preventDefault();
        void store.save();
      }}
    >
      <p className="m-0 font-mono text-[12px] break-all text-main-100">{store.draftPath}</p>
      <SelectInput
        label="Тип разрешения"
        value={store.draftScope}
        options={APP_SCOPE_OPTIONS}
        disabled={store.busy}
        onChange={(value) => {
          if (value === "always" || value === "conversation") store.setScope(value);
        }}
      />
      {store.draftScope === "conversation" ? (
        <fieldset className="m-0 min-w-0 rounded-control border border-line p-1.5">
          <legend className="mx-1.5 px-1 text-[12px] text-main-400">Диалоги</legend>
          <div className="flex max-h-72 flex-col gap-1 overflow-y-auto">
            {store.conversations.length === 0 ? (
              <p className="m-0 px-3 py-4 text-[12px] text-main-500">{APP_DIALOGS_EMPTY}</p>
            ) : (
              store.conversations.map((conversation) => {
                const selected = store.draftConversations.includes(conversation.id);
                return (
                  <label
                    key={conversation.id}
                    className={`flex min-w-0 items-center gap-3 rounded-control px-3 py-2.5 transition-colors ${
                      selected ? "bg-accent-soft" : "hover:bg-main-800"
                    } ${store.busy ? "cursor-default opacity-60" : "cursor-pointer"}`}
                  >
                    <span
                      className={`flex size-8 flex-none items-center justify-center rounded-control ${selected ? "text-accent-medium" : "bg-main-800 text-main-400"}`}
                    >
                      <Icon path={mdiChatOutline} size={16} />
                    </span>
                    <span
                      className="line-clamp-2 min-w-0 flex-1 text-[13px]/5 font-medium wrap-break-word text-main-100"
                      title={conversation.title}
                    >
                      {conversation.title}
                    </span>
                    <InputCheckBox
                      aria-label={conversation.title}
                      className="flex-none"
                      checked={selected}
                      disabled={store.busy}
                      onChange={() => store.toggleConversation(conversation.id)}
                    />
                  </label>
                );
              })
            )}
          </div>
        </fieldset>
      ) : null}
      <div className="flex justify-end gap-2">
        <Button tone="ghost" disabled={store.busy} onClick={store.cancelAdding}>
          {APP_CANCEL}
        </Button>
        <Button type="submit" tone="primary" disabled={!store.canSave}>
          {APP_SAVE}
        </Button>
      </div>
    </form>
  );
});

const AppDetail = observer(function AppDetail({ group }: { readonly group: AppPermissionGroup }) {
  const { devicePermissions: store, chat } = useStore();
  const navigation = useAppNavigation();
  const grant = group.grants[0];
  if (grant === undefined) return null;
  const latestGranted = Math.max(...group.grants.map((item) => item.grantedAt));
  const used = group.grants.flatMap((item) => (item.lastUsedAt === null ? [] : [item.lastUsedAt]));
  return (
    <SettingsSubPage
      title={group.app}
      hint={group.scope === "always" ? ALWAYS_SECTION : DIALOG_SECTION}
      onBack={() => store.select(null)}
    >
      <div className="flex items-center gap-3">
        <AppIcon icon={grant.icon} size={40} />
        <span className="min-w-0 flex-1 text-[14px] font-medium break-all text-main-100">
          {grant.app}
        </span>
        {group.scope === "always" ? <RevokeButton grant={grant} /> : null}
      </div>
      <InfoTable
        label="Сведения о приложении"
        rows={[
          {
            key: "app",
            icon: mdiApplicationOutline,
            label: "Приложение",
            value: grant.app,
            mono: true,
          },
          {
            key: "path",
            icon: mdiFolderOutline,
            label: "Путь",
            value: grant.path || APP_PATH_UNKNOWN,
            mono: grant.path.length > 0,
            muted: grant.path.length === 0,
          },
          {
            key: "granted",
            icon: mdiCalendarOutline,
            label: "Выдано",
            value: grantedLabel({ grantedAt: latestGranted }),
          },
          {
            key: "used",
            icon: mdiClockOutline,
            label: "Последнее использование",
            value:
              used.length === 0
                ? "Не использовалось"
                : `Использовано ${formatLastUsed(Math.max(...used), store.now)}`,
            muted: used.length === 0,
          },
        ]}
      />
      {group.scope === "conversation" ? (
        <PreferenceContainer title={APP_DIALOGS}>
          {group.grants.map((item) =>
            item.conversation === null ? null : (
              <div
                key={item.id}
                className="flex items-center gap-2 border-b border-line py-3 last:border-0"
              >
                <span className="min-w-0 flex-1 truncate text-[13px] text-main-100">
                  {item.conversation.title}
                </span>
                <IconButton
                  path={mdiOpenInNew}
                  label={APP_OPEN_DIALOG}
                  size="sm"
                  onClick={() => {
                    if (item.conversation === null) return;
                    navigation.go(ROUTES.chat);
                    void chat.open(item.conversation.id);
                  }}
                />
                <RevokeButton grant={item} />
              </div>
            ),
          )}
        </PreferenceContainer>
      ) : null}
    </SettingsSubPage>
  );
});

const AppPermissions = observer(function AppPermissions({
  onBack,
}: {
  readonly onBack: () => void;
}) {
  const { devicePermissions: store } = useStore();
  useEffect(() => {
    void store.load();
  }, [store]);
  return (
    <>
      {store.error === null ? null : <Alert onDismiss={store.dismissError}>{store.error}</Alert>}
      {store.selectedGroup !== null ? (
        <AppDetail group={store.selectedGroup} />
      ) : (
        <SettingsSubPage title={APP_PERMISSIONS_TITLE} hint={APP_PERMISSIONS_HINT} onBack={onBack}>
          <div className="flex justify-end">
            <Button
              tone="secondary"
              size="sm"
              disabled={store.busy || store.draftPath !== null}
              onClick={() => void store.pickApp()}
            >
              <Icon path={mdiPlus} size={14} />
              {APP_ADD}
            </Button>
          </div>
          <AddPermission />
          {store.loading ? <p className="text-[12px] text-main-500">Загрузка разрешений…</p> : null}
          {store.loaded && store.groups.length === 0 ? (
            <p className="text-[12px] text-main-500">{APP_PERMISSIONS_EMPTY}</p>
          ) : null}
          {(
            [
              ["always", ALWAYS_SECTION],
              ["conversation", DIALOG_SECTION],
            ] as const
          ).map(([scope, title]) => (
            <PreferenceContainer key={scope} title={title}>
              <div className="overflow-x-auto">
                <table className="w-full text-left text-[12px]">
                  <thead className="text-main-500">
                    <tr>
                      <th className="py-2 font-medium">Приложение</th>
                      <th className="px-3 py-2 font-medium">Путь</th>
                      <th className="py-2 text-right font-medium">Действия</th>
                    </tr>
                  </thead>
                  <tbody>
                    {store.groups
                      .filter((group) => group.scope === scope)
                      .map((group) => {
                        const grant = group.grants[0];
                        if (grant === undefined) return null;
                        return (
                          <tr key={group.app} className="border-t border-line">
                            <td className="py-3">
                              <span className="flex items-center gap-2">
                                <AppIcon icon={grant.icon} size={24} />
                                <span className="break-all text-main-100">{group.app}</span>
                              </span>
                            </td>
                            <td className="p-3 font-mono break-all text-main-400">
                              {grant.path || APP_PATH_UNKNOWN}
                            </td>
                            <td className="py-3">
                              <div className="flex items-center justify-end gap-2">
                                <Button
                                  tone="secondary"
                                  size="sm"
                                  onClick={() => store.select(group)}
                                >
                                  {APP_DETAILS}
                                </Button>
                                {scope === "always" ? <RevokeButton grant={grant} /> : null}
                              </div>
                            </td>
                          </tr>
                        );
                      })}
                  </tbody>
                </table>
              </div>
            </PreferenceContainer>
          ))}
        </SettingsSubPage>
      )}
    </>
  );
});

function DevicePreferences() {
  const { appSettings, devicePermissions } = useStore();
  const [permissions, setPermissions] = useState(false);
  const enabled = appSettings.values[DEVICE_ENABLED_ID];
  if (permissions)
    return (
      <AppPermissions
        onBack={() => {
          devicePermissions.cancelAdding();
          setPermissions(false);
        }}
      />
    );
  return (
    <div className="flex flex-col gap-4 pt-2">
      <div
        data-setting={DEVICE_ENABLED_ID}
        className="flex items-center gap-3.5 rounded-card border border-line bg-main-850 px-4 py-3.5"
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
      <PreferenceContainer title={APP_PERMISSIONS_TITLE}>
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
              setPermissions(true);
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
