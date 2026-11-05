import { mdiClose, mdiFlashOutline, mdiLogin, mdiLogout, mdiPlay, mdiStop } from "@mdi/js";
import { observer } from "mobx-react-lite";
import Button from "../../ui/atoms/buttons/Button";
import Icon from "../../ui/atoms/Icon";
import type ConnectionStore from "./ConnectionStore";
import TestResultLine from "./TestResultLine";

export const CONNECTOR_CONTROL_ACTIONS = "Управление коннектором";

function ConnectionControlActions({ store }: { readonly store: ConnectionStore }) {
  const vm = store.form;
  if (vm === null) return null;
  const state = vm.isNew ? null : store.selectedState;
  const server = store.selectedServer;
  const showsError =
    store.testResult === null &&
    server !== null &&
    server.lastError !== null &&
    (state === "failed" || state === "reauth" || state === "restarting");
  const running = state === "connected" || state === "starting" || state === "restarting";
  const oauthable = !vm.isNew && server !== null && server.transport === "http";
  const signedIn = oauthable && server.oauth === "signed-in";
  const needsSignIn = oauthable && (server.oauth === "required" || state === "reauth");
  return (
    <div
      role="group"
      aria-label={CONNECTOR_CONTROL_ACTIONS}
      className="mx-auto flex w-full max-w-360 flex-wrap items-center justify-between gap-3 rounded-xl bg-main-800 px-4 py-3 shadow-[0_6px_24px_rgb(0_0_0/0.12)]"
    >
      <div className="flex min-w-0 flex-1 flex-wrap items-center gap-3">
        <Button
          tone="secondary"
          disabled={store.testing || store.saving || !vm.enabled}
          onClick={() => {
            void store.test();
          }}
        >
          <Icon path={mdiFlashOutline} size={15} />
          {store.testing ? "Проверка…" : "Проверить связь"}
        </Button>
        {vm.isNew ? null : running ? (
          <Button
            tone="secondary"
            disabled={store.switching}
            onClick={() => {
              void store.stopSelected();
            }}
          >
            <Icon path={mdiStop} size={15} />
            Остановить
          </Button>
        ) : (
          <Button
            tone="secondary"
            disabled={store.switching || !vm.enabled || vm.dirty}
            onClick={() => {
              void store.startSelected();
            }}
          >
            <Icon path={mdiPlay} size={15} />
            Подключить
          </Button>
        )}
        {store.signingIn ? (
          <Button
            tone="secondary"
            onClick={() => {
              void store.cancelSignIn();
            }}
          >
            <Icon path={mdiClose} size={15} />
            Отменить вход
          </Button>
        ) : needsSignIn ? (
          <Button
            tone="primary"
            disabled={store.switching || !vm.enabled}
            onClick={() => {
              void store.signIn();
            }}
          >
            <Icon path={mdiLogin} size={15} />
            Войти
          </Button>
        ) : signedIn ? (
          <Button
            tone="ghost"
            disabled={store.switching}
            onClick={() => {
              void store.signOut();
            }}
          >
            <Icon path={mdiLogout} size={15} />
            Выйти
          </Button>
        ) : null}
        <span className="min-w-0 flex-1">
          <TestResultLine
            result={store.testResult}
            testing={store.testing}
            signingIn={store.signingIn}
            unsaved={vm.dirty}
            lastError={showsError ? server.lastError : null}
          />
        </span>
      </div>
      <div className="ml-auto flex flex-none items-center gap-2">
        <Button
          tone="ghost"
          disabled={(!vm.isNew && !vm.dirty) || store.saving}
          onClick={store.resetForm}
        >
          Отменить
        </Button>
        <Button
          tone="primary"
          disabled={store.saving || (!vm.isNew && !vm.dirty)}
          onClick={() => {
            void store.submit();
          }}
        >
          {store.saving ? "Сохранение…" : "Сохранить"}
        </Button>
      </div>
    </div>
  );
}
export default observer(ConnectionControlActions);
