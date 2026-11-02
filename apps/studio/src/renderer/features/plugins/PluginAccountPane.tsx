import {
  mdiAlertCircleOutline,
  mdiCheck,
  mdiCheckCircleOutline,
  mdiDeleteOutline,
  mdiDotsHorizontal,
  mdiFlashOutline,
  mdiLogin,
  mdiToyBrickOutline,
} from "@mdi/js";
import { observer } from "mobx-react-lite";
import { Dropdown, ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import { useState } from "react";
import type { IntegrationTestResultDto } from "@zvs/shared";
import Button from "../../ui/atoms/buttons/Button";
import IconDropdownButton from "../../ui/atoms/buttons/IconDropdownButton";
import Chip from "../../ui/atoms/Chip";
import Icon from "../../ui/atoms/Icon";
import StatusDot from "../../ui/atoms/StatusDot";
import { STATUS_TONE_TEXT } from "../../ui/atoms/statusTone";
import TextInput from "../../ui/atoms/TextInput";
import Toggle from "../../ui/atoms/Toggle";
import { ActionMenu, ActionMenuItem, ActionMenuSeparator } from "../../ui/molecules/ActionMenu";
import Alert from "../../ui/molecules/Alert";
import ConfirmDialog from "../../ui/molecules/ConfirmDialog";
import EmptyState from "../../ui/molecules/EmptyState";
import Field from "../../ui/molecules/Field";
import { formatLatency } from "../connections/connectionPresentation";
import PluginField from "./PluginField";
import type { PluginFormVm } from "./PluginFormVm";
import PluginMark from "./PluginMark";
import { PLUGIN_LABELS, SIGNED_IN } from "./pluginPresentation";
import type PluginStore from "./PluginStore";
import SignInPanel from "./SignInPanel";
import SignInTypeChoice from "./SignInTypeChoice";
import { isSignedIn } from "./signInTypes";

export const ACCOUNT_HEADING = "Аккаунт";
export const ACCOUNT_NAME = "Название";
export const ENABLED_LABEL = "Включён";
export const SIGN_IN = "Войти";
export const SIGN_IN_AGAIN = "Войти снова";
export const NOT_SIGNED_IN = "Вход не выполнен";
export const TEST_CONNECTION = "Проверить подключение";
export const TESTING = "Проверка…";
export const SAVE = "Сохранить";
export const SAVING = "Сохранение…";
export const CONNECT = "Подключить";
export const CANCEL = "Отменить";
export const REMOVE_ACCOUNT = "Удалить аккаунт";
export const ACCOUNT_ACTIONS = "Действия с аккаунтом";
export const NEW_ACCOUNT = "Новый";
export const CHOOSE_TYPE_TITLE = "Выберите способ подключения";
export const CHOOSE_TYPE_HINT = "Поля формы зависят от выбранного способа.";

function TestResult({ result }: { readonly result: IntegrationTestResultDto }) {
  return (
    <div className="flex flex-col gap-1.5">
      {result.checks.map((check) => (
        <div key={check.label} className="flex min-w-0 items-start gap-2 text-[12px]">
          <Icon
            path={check.ok ? mdiCheck : mdiAlertCircleOutline}
            size={14}
            className={`mt-px flex-none ${check.ok ? "text-ok" : "text-err"}`}
          />
          <span className="flex-none font-medium text-main-100">{check.label}</span>
          {check.detail === null ? null : (
            <span className={`min-w-0 ${check.ok ? "text-main-400" : "text-err"}`}>
              {check.detail}
            </span>
          )}
        </div>
      ))}
      {result.error === null ? null : (
        <div className="flex min-w-0 items-start gap-2 text-[12px] text-err">
          <Icon path={mdiAlertCircleOutline} size={14} className="mt-px flex-none" />
          <span className="min-w-0">{result.error}</span>
        </div>
      )}
      <span className="font-mono text-[11px] text-main-500">{formatLatency(result.latencyMs)}</span>
    </div>
  );
}

const SignInBlock = observer(function SignInBlock({
  store,
  vm,
}: {
  readonly store: PluginStore;
  readonly vm: PluginFormVm;
}) {
  const account = store.selectedAccount;
  const signedIn = account !== null && isSignedIn(account.config);
  const expired = account !== null && store.stateOf(account) === "needsSignIn";
  return (
    <div className="flex flex-col gap-3 border-t border-line pt-3.5">
      <div className="flex items-center gap-2.5">
        {signedIn && !expired ? (
          <span className="flex flex-1 items-center gap-1.75 text-[12.5px] text-ok">
            <Icon path={mdiCheckCircleOutline} size={15} />
            {SIGNED_IN}
          </span>
        ) : (
          <span className="flex-1 text-[12.5px] text-main-400">
            {vm.isNew ? vm.type.label : NOT_SIGNED_IN}
          </span>
        )}
        <Button
          tone={signedIn && !expired ? "secondary" : "primary"}
          disabled={store.signingIn || store.saving}
          onClick={() => {
            void store.startSignIn();
          }}
        >
          <Icon path={mdiLogin} size={15} />
          {signedIn ? SIGN_IN_AGAIN : SIGN_IN}
        </Button>
      </div>
      <SignInPanel store={store} />
    </div>
  );
});

const AccountForm = observer(function AccountForm({
  store,
  vm,
}: {
  readonly store: PluginStore;
  readonly vm: PluginFormVm;
}) {
  const account = store.selectedAccount;
  const kind = vm.type.kind;
  const state = account === null ? null : store.stateOf(account);
  const tone = account === null ? null : store.stateTone(account);
  const reason =
    account !== null && (state === "failed" || state === "retrying") ? account.statusDetail : null;
  const test = store.selectedTest;
  const [confirming, setConfirming] = useState(false);
  const showCancel = vm.isNew ? store.accountsOf(kind).length > 0 : true;
  const showSave = !(vm.isNew && vm.type.browser);

  return (
    <>
      <section className="flex flex-none flex-col gap-3.5">
        <div className="flex items-center gap-2.5">
          <PluginMark kind={kind} mark={vm.type.mark} size="sm" />
          <Chip>{vm.type.chip}</Chip>
          {account === null || state === null || tone === null ? (
            <Chip tone="accent">{NEW_ACCOUNT}</Chip>
          ) : (
            <Chip>
              <StatusDot tone={tone} />
              <span className={STATUS_TONE_TEXT[tone]}>{store.stateLabel(account)}</span>
            </Chip>
          )}
          <span className="flex-1" />
          {account === null ? null : (
            <Toggle
              checked={account.enabled}
              label={ENABLED_LABEL}
              disabled={store.toggling || store.signingIn}
              onChange={(checked) => {
                void store.setEnabled(checked);
              }}
            />
          )}
        </div>

        <div className="flex items-center gap-3">
          <h2 className="flex-1 text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase">
            {ACCOUNT_HEADING}
          </h2>
          {showCancel ? (
            vm.isNew ? (
              <Button tone="secondary" onClick={store.cancelAdding}>
                {CANCEL}
              </Button>
            ) : (
              <Button
                tone="secondary"
                disabled={!vm.dirty || store.saving}
                onClick={store.resetForm}
              >
                {CANCEL}
              </Button>
            )
          ) : null}
          {showSave ? (
            <Button
              tone="primary"
              disabled={store.saving || (!vm.isNew && !vm.dirty)}
              onClick={() => {
                void store.save();
              }}
            >
              {store.saving ? SAVING : vm.isNew ? CONNECT : SAVE}
            </Button>
          ) : null}
          {account === null ? null : (
            <Dropdown menuWidth={220} menuPlacement="bottom-right">
              <IconDropdownButton
                label={ACCOUNT_ACTIONS}
                tooltip="bottom-right"
                path={mdiDotsHorizontal}
              />
              <ActionMenu label={ACCOUNT_ACTIONS}>
                <ActionMenuItem
                  icon={mdiFlashOutline}
                  disabled={store.testing || store.saving || store.signingIn || vm.dirty}
                  onClick={() => {
                    void store.test();
                  }}
                >
                  {store.testing ? TESTING : TEST_CONNECTION}
                </ActionMenuItem>
                <ActionMenuSeparator />
                <ActionMenuItem
                  icon={mdiDeleteOutline}
                  danger
                  disabled={store.removing}
                  onClick={() => setConfirming(true)}
                >
                  {REMOVE_ACCOUNT}
                </ActionMenuItem>
              </ActionMenu>
            </Dropdown>
          )}
        </div>

        {reason === null ? null : <Alert tone="error">{reason}</Alert>}

        <div className="grid grid-cols-1 gap-x-4 gap-y-3.5 sm:grid-cols-2">
          <div className="sm:col-span-2">
            <Field
              label={ACCOUNT_NAME}
              htmlFor="plugin-account-name"
              required
              error={vm.errorOf("name")}
            >
              <TextInput
                id="plugin-account-name"
                value={vm.name}
                placeholder={PLUGIN_LABELS[kind]}
                invalid={vm.errorOf("name") !== undefined}
                onChange={(event) => {
                  vm.setName(event.target.value);
                }}
              />
            </Field>
          </div>
          {vm.visibleFields.map((field) => (
            <PluginField key={field.key} field={field} vm={vm} secrets={store.secretChoices} />
          ))}
        </div>

        {vm.type.browser ? <SignInBlock store={store} vm={vm} /> : null}
      </section>

      {account === null || (test === null && !store.testing) ? null : (
        <section className="flex flex-none flex-col gap-2.5">
          <h2 className="text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase">
            {TEST_CONNECTION}
          </h2>
          {store.testing || test === null ? (
            <span className="text-[12px] text-main-400">{TESTING}</span>
          ) : (
            <TestResult result={test} />
          )}
        </section>
      )}

      <ConfirmDialog
        open={confirming && account !== null}
        title="Удалить аккаунт?"
        confirmLabel="Удалить"
        confirmTone="danger"
        onCancel={() => setConfirming(false)}
        onConfirm={() => {
          setConfirming(false);
          void store.remove();
        }}
      >
        Аккаунт «{account?.name}» будет отключён и удалён. Его инструменты пропадут из реестра, а
        сценарии, которые от него зависят, перестанут запускаться.
      </ConfirmDialog>
    </>
  );
});

function PluginAccountPane({ store }: { readonly store: PluginStore }) {
  const form = store.form;
  return (
    <ScrollArea className="flex min-w-0 flex-1 flex-col gap-3.5 px-4.5 py-4">
      {store.error === null ? null : <Alert onDismiss={store.dismissError}>{store.error}</Alert>}
      {form === null || form.isNew ? <SignInTypeChoice store={store} /> : null}
      {form === null ? (
        <div className="flex flex-1 items-center justify-center">
          <EmptyState
            icon={mdiToyBrickOutline}
            title={CHOOSE_TYPE_TITLE}
            description={CHOOSE_TYPE_HINT}
          />
        </div>
      ) : (
        <AccountForm key={form.accountId ?? form.type.id} store={store} vm={form} />
      )}
    </ScrollArea>
  );
}

export default observer(PluginAccountPane);
