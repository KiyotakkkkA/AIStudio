import {
  mdiAlertCircleOutline,
  mdiCheck,
  mdiCheckCircleOutline,
  mdiDeleteOutline,
  mdiFlashOutline,
  mdiLogin,
  mdiToyBrickOutline,
} from "@mdi/js";
import { observer } from "mobx-react-lite";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import type { IntegrationTestResultDto } from "@zvs/shared";
import Button from "../../ui/atoms/buttons/Button";
import IconButton from "../../ui/atoms/buttons/IconButton";
import Chip from "../../ui/atoms/Chip";
import Icon from "../../ui/atoms/Icon";
import StatusDot from "../../ui/atoms/StatusDot";
import { STATUS_TONE_TEXT } from "../../ui/atoms/statusTone";
import TextInput from "../../ui/atoms/TextInput";
import Toggle from "../../ui/atoms/Toggle";
import Alert from "../../ui/molecules/Alert";
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

  return (
    <>
      <section className="flex flex-none flex-col gap-3.5 rounded-card border border-line bg-main-900 p-4">
        <div className="flex items-center gap-2.5">
          <PluginMark kind={kind} mark={vm.type.mark} size="sm" />
          <h2 className="flex-1 text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase">
            {ACCOUNT_HEADING}
          </h2>
          <Chip>{vm.type.chip}</Chip>
          {account === null || state === null || tone === null ? (
            <Chip tone="accent">{NEW_ACCOUNT}</Chip>
          ) : (
            <Chip>
              <StatusDot tone={tone} />
              <span className={STATUS_TONE_TEXT[tone]}>{store.stateLabel(account)}</span>
            </Chip>
          )}
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
        <section className="flex flex-none flex-col gap-2.5 rounded-card border border-line bg-main-900 p-4">
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

      <div className="flex flex-none flex-wrap items-center gap-3 rounded-card border border-line bg-main-900 px-4 py-3">
        {account === null ? null : (
          <IconButton
            key={account.id}
            tone="danger"
            size="md"
            path={mdiDeleteOutline}
            label={REMOVE_ACCOUNT}
            disabled={store.removing}
            needConfirm
            modalSetup={{
              title: "Удалить аккаунт?",
              content: (
                <>
                  Аккаунт «{account.name}» будет отключён и удалён. Его инструменты пропадут из
                  реестра, а сценарии, которые от него зависят, перестанут запускаться.
                </>
              ),
              tone: "danger",
              confirmLabel: "Удалить",
            }}
            onClick={() => {
              void store.remove();
            }}
          />
        )}
        {account === null ? null : (
          <Button
            tone="secondary"
            disabled={store.testing || store.saving || store.signingIn || vm.dirty}
            onClick={() => {
              void store.test();
            }}
          >
            <Icon path={mdiFlashOutline} size={15} />
            {store.testing ? TESTING : TEST_CONNECTION}
          </Button>
        )}
        <span className="flex-1" />
        {vm.isNew ? (
          store.accountsOf(kind).length === 0 ? null : (
            <Button tone="secondary" onClick={store.cancelAdding}>
              {CANCEL}
            </Button>
          )
        ) : (
          <Button tone="secondary" disabled={!vm.dirty || store.saving} onClick={store.resetForm}>
            {CANCEL}
          </Button>
        )}
        {vm.isNew && vm.type.browser ? null : (
          <Button
            tone="primary"
            disabled={store.saving || (!vm.isNew && !vm.dirty)}
            onClick={() => {
              void store.save();
            }}
          >
            {store.saving ? SAVING : vm.isNew ? CONNECT : SAVE}
          </Button>
        )}
      </div>
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
