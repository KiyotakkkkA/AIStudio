import {
  mdiClose,
  mdiDeleteOutline,
  mdiDotsHorizontal,
  mdiFlashOutline,
  mdiLogin,
  mdiLogout,
  mdiPlay,
  mdiStop,
} from "@mdi/js";
import { observer } from "mobx-react-lite";
import { Dropdown } from "@kiyotakkkka/zvs-uikit-lib";
import { useState } from "react";
import type { McpStartup, McpTransport } from "@zvs/shared";
import Button from "../../ui/atoms/buttons/Button";
import IconDropdownButton from "../../ui/atoms/buttons/IconDropdownButton";
import Chip from "../../ui/atoms/Chip";
import Icon from "../../ui/atoms/Icon";
import SegmentedControl from "../../ui/atoms/SegmentedControl";
import SelectInput from "../../ui/atoms/SelectInput";
import StatusDot from "../../ui/atoms/StatusDot";
import { STATUS_TONE_TEXT } from "../../ui/atoms/statusTone";
import TextInput from "../../ui/atoms/TextInput";
import Toggle from "../../ui/atoms/Toggle";
import { ActionMenu, ActionMenuItem } from "../../ui/molecules/ActionMenu";
import ConfirmDialog from "../../ui/molecules/ConfirmDialog";
import Field from "../../ui/molecules/Field";
import type ConnectionStore from "./ConnectionStore";
import { STARTUP_LABELS, STATE_LABELS, STATE_TONES } from "./connectionPresentation";
import type { ServerFormVm } from "./ServerFormVm";
import Disclosure from "./Disclosure";
import TestResultLine from "./TestResultLine";
import ValueTable from "./ValueTable";
import Alert from "../../ui/molecules/Alert";

const TRANSPORTS: readonly { value: McpTransport; label: string }[] = [
  { value: "stdio", label: "stdio" },
  { value: "http", label: "http" },
];

const STARTUPS: readonly McpStartup[] = ["manual", "launch"];

interface ServerSetupCardProps {
  readonly store: ConnectionStore;
  readonly vm: ServerFormVm;
}

function ServerSetupCard({ store, vm }: ServerSetupCardProps) {
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
  const [confirmingRemove, setConfirmingRemove] = useState(false);

  return (
    <section className="flex flex-none flex-col gap-3.25">
      <div className="flex items-center gap-2.25">
        {state === null ? (
          <Chip>Новый</Chip>
        ) : (
          <Chip>
            <StatusDot tone={STATE_TONES[state]} />
            <span className={STATUS_TONE_TEXT[STATE_TONES[state]]}>{STATE_LABELS[state]}</span>
          </Chip>
        )}
        <span className="flex-1" />
        <Toggle
          checked={vm.enabled}
          disabled={store.switching}
          label={vm.enabled ? "Включён" : "Выключен"}
          onChange={(checked) => {
            void store.setServerEnabled(checked);
          }}
        />
      </div>

      <div className="flex items-center gap-3">
        <h2 className="flex-1 text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase">
          Настройка сервера
        </h2>
        <Button
          tone="secondary"
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
        {vm.isNew ? null : (
          <Dropdown menuWidth={220} menuPlacement="bottom-right">
            <IconDropdownButton
              label="Действия с сервером"
              tooltip="bottom-right"
              path={mdiDotsHorizontal}
            />
            <ActionMenu label="Действия с сервером">
              <ActionMenuItem
                icon={mdiDeleteOutline}
                danger
                disabled={store.removing}
                onClick={() => setConfirmingRemove(true)}
              >
                Удалить сервер
              </ActionMenuItem>
            </ActionMenu>
          </Dropdown>
        )}
      </div>

      <div className="grid grid-cols-3 gap-3.25">
        <Field label="Имя" htmlFor="mcp-name" required error={vm.errorOf("name")}>
          <TextInput
            id="mcp-name"
            mono
            value={vm.name}
            maxLength={120}
            placeholder="qdrant-local"
            invalid={vm.errorOf("name") !== undefined}
            onChange={(event) => {
              vm.setName(event.target.value);
            }}
          />
        </Field>
        <Field label="Транспорт">
          <SegmentedControl
            label="Транспорт"
            value={vm.transport}
            options={TRANSPORTS}
            onChange={vm.setTransport}
          />
        </Field>
        <Field label="Запуск">
          <SelectInput
            label="Запуск"
            value={vm.startup}
            options={STARTUPS.map((startup) => ({
              value: startup,
              label: STARTUP_LABELS[startup],
            }))}
            onChange={(value) => {
              vm.setStartup(value as McpStartup);
            }}
          />
        </Field>
      </div>

      {vm.transport === "stdio" ? (
        <>
          <div className="grid grid-cols-[180px_minmax(0,1fr)] gap-3.25">
            <Field label="Команда" htmlFor="mcp-command" required error={vm.errorOf("command")}>
              <TextInput
                id="mcp-command"
                mono
                value={vm.command}
                placeholder="uvx"
                invalid={vm.errorOf("command") !== undefined}
                onChange={(event) => {
                  vm.setCommand(event.target.value);
                }}
              />
            </Field>
            <Field
              label="Аргументы"
              htmlFor="mcp-args"
              help="Через пробел; значения с пробелами — в кавычках."
              error={vm.errorOf("args")}
            >
              <TextInput
                id="mcp-args"
                mono
                value={vm.argsText}
                placeholder="mcp-server-qdrant --url http://127.0.0.1:6333"
                invalid={vm.errorOf("args") !== undefined}
                onChange={(event) => {
                  vm.setArgsText(event.target.value);
                }}
              />
            </Field>
          </div>

          <div>
            <span className="mb-1.5 flex items-center text-[12px] font-medium text-main-300">
              Окружение
              <span className="ml-1.5 font-normal text-main-500">
                — секреты подставляются при запуске
              </span>
            </span>
            <ValueTable
              vm={vm}
              table="env"
              secrets={store.secrets}
              namePlaceholder="API_KEY"
              empty="Переменных окружения нет."
            />
          </div>
        </>
      ) : (
        <>
          <Field label="Адрес" htmlFor="mcp-url" required error={vm.errorOf("url")}>
            <TextInput
              id="mcp-url"
              mono
              value={vm.url}
              placeholder="https://example.com/mcp"
              invalid={vm.errorOf("url") !== undefined}
              onChange={(event) => {
                vm.setUrl(event.target.value);
              }}
            />
          </Field>

          <div>
            <span className="mb-1.5 flex items-center text-[12px] font-medium text-main-300">
              Заголовки
            </span>
            <ValueTable
              vm={vm}
              table="headers"
              secrets={store.secrets}
              namePlaceholder="Authorization"
              empty="Заголовков нет."
            />
          </div>
        </>
      )}

      {vm.preset?.note === undefined ? null : (
        <p className="rounded-control bg-main-800 px-3 py-2 text-[12px] text-main-400">
          {vm.preset.note}
        </p>
      )}

      <div className="flex items-center gap-3 border-b border-line pb-3.25">
        <Button
          tone="primary"
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

      {vm.isNew || server?.instructions == null ? null : (
        <Disclosure label="Инструкции сервера" className="border-b border-line">
          <p className="pb-2 text-[12.5px]/relaxed whitespace-pre-wrap text-main-400">
            {server.instructions}
          </p>
        </Disclosure>
      )}

      {vm.banner === null ? null : <Alert>{vm.banner}</Alert>}

      <ConfirmDialog
        open={confirmingRemove}
        title="Удалить сервер?"
        confirmLabel="Удалить"
        confirmTone="danger"
        onCancel={() => setConfirmingRemove(false)}
        onConfirm={() => {
          setConfirmingRemove(false);
          void store.removeSelected();
        }}
      >
        «{vm.name}» будет остановлен и удалён. Его инструменты пропадут из реестра, а сценарии и
        навыки, которые на них ссылаются, перестанут их вызывать.
      </ConfirmDialog>
    </section>
  );
}

export default observer(ServerSetupCard);
