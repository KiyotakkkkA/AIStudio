import { observer } from "mobx-react-lite";
import type { McpStartup, McpTransport } from "@zvs/shared";
import Chip from "../../ui/atoms/Chip";
import SegmentedControl from "../../ui/atoms/SegmentedControl";
import SelectInput from "../../ui/atoms/SelectInput";
import StatusDot from "../../ui/atoms/StatusDot";
import { STATUS_TONE_TEXT } from "../../ui/atoms/statusTone";
import TextInput from "../../ui/atoms/TextInput";
import Toggle from "../../ui/atoms/Toggle";
import Field from "../../ui/molecules/Field";
import type ConnectionStore from "./ConnectionStore";
import { STARTUP_LABELS, STATE_LABELS, STATE_TONES } from "./connectionPresentation";
import type { ServerFormVm } from "./ServerFormVm";
import Disclosure from "./Disclosure";
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

  return (
    <section className="mx-auto flex w-full max-w-360 flex-none flex-col gap-4 rounded-xl bg-main-700/55 p-5">
      <div className="flex flex-wrap items-center gap-2.25">
        <h2 className="flex-1 text-[15px] font-semibold tracking-[-0.01em] text-main-100">
          Настройка сервера
        </h2>
        {state === null ? (
          <Chip>Новый</Chip>
        ) : (
          <Chip>
            <StatusDot tone={STATE_TONES[state]} />
            <span className={STATUS_TONE_TEXT[STATE_TONES[state]]}>{STATE_LABELS[state]}</span>
          </Chip>
        )}
        <Toggle
          checked={vm.enabled}
          disabled={store.switching}
          label={vm.enabled ? "Включён" : "Выключен"}
          onChange={(checked) => {
            void store.setServerEnabled(checked);
          }}
        />
      </div>

      <div className="grid grid-cols-1 gap-3 lg:grid-cols-3">
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
          <div className="grid grid-cols-1 gap-3 lg:grid-cols-[180px_minmax(0,1fr)]">
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

      {vm.isNew || server?.instructions == null ? null : (
        <Disclosure label="Инструкции сервера" className="rounded-control px-3">
          <p className="pb-2 text-[12.5px]/relaxed whitespace-pre-wrap text-main-400">
            {server.instructions}
          </p>
        </Disclosure>
      )}

      {vm.banner === null ? null : <Alert>{vm.banner}</Alert>}
    </section>
  );
}

export default observer(ServerSetupCard);
