import {
  mdiAlertCircleOutline,
  mdiCheck,
  mdiChevronDown,
  mdiChevronRight,
  mdiClose,
  mdiOpenInNew,
  mdiPlay,
} from "@mdi/js";
import { observer } from "mobx-react-lite";
import { InputCheckBox, ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import { useNavigate } from "react-router-dom";
import { ROUTES } from "../../app/routes";
import Chip from "../../ui/atoms/Chip";
import Icon from "../../ui/atoms/Icon";
import SelectInput from "../../ui/atoms/SelectInput";
import StatusDot from "../../ui/atoms/StatusDot";
import TextArea from "../../ui/atoms/TextArea";
import TextInput from "../../ui/atoms/TextInput";
import Button from "../../ui/atoms/buttons/Button";
import Field from "../../ui/molecules/Field";
import { formatDateTime, STATUS_LABELS, STATUS_TONES } from "../tasks/runPresentation";
import { KEY_HINT, type SkillFormVm } from "./SkillFormVm";
import { modelValue, type ToolRow } from "./SkillStore";
import type SkillStore from "./SkillStore";
import { formatScore, METHOD_LABELS } from "./skillPresentation";

const HEADING = "text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase";
const CARD = "flex flex-col gap-2.25 rounded-card border border-line bg-main-850 p-3";
const RECENT_RUNS = 5;

const ToolLine = observer(function ToolLine({
  row,
  vm,
}: {
  readonly row: ToolRow;
  readonly vm: SkillFormVm;
}) {
  const allowed = vm.allows(row.key);
  const blocked = row.ceiling.block !== null;
  const reason = vm.toolErrorOf(row.key) ?? row.ceiling.reason;
  return (
    <div
      className={`flex items-start gap-2.25 px-2.75 py-1.75 ${blocked && !allowed ? "opacity-60" : ""}`}
      title={row.tool?.description}
    >
      <span className="mt-0.5">
        <InputCheckBox
          aria-label={`Разрешить ${row.key}`}
          checked={allowed}
          disabled={blocked && !allowed}
          onChange={(checked) => {
            vm.setToolAllowed(row.key, checked);
          }}
        />
      </span>
      <span className="flex min-w-0 flex-1 flex-col">
        <span className="flex min-w-0 items-center gap-1.5">
          <span
            className={`min-w-0 flex-1 truncate font-mono text-[11.5px] ${allowed ? "text-main-50" : "text-main-300"}`}
          >
            {row.key}
          </span>
          {row.tool?.source === "mcp" ? <Chip tone="accent">mcp</Chip> : null}
        </span>
        {reason === null ? null : (
          <span className={`text-[11px] leading-snug ${allowed ? "text-err" : "text-main-500"}`}>
            {reason}
          </span>
        )}
      </span>
    </div>
  );
});

const TriggerCheck = observer(function TriggerCheck({
  store,
  vm,
}: {
  readonly store: SkillStore;
  readonly vm: SkillFormVm;
}) {
  const own = store.ownMatch;
  const rival = store.rival;
  const result = store.testResult;
  const described = vm.description.trim().length > 0;
  return (
    <div className={CARD}>
      <form
        className="flex gap-2"
        onSubmit={(event) => {
          event.preventDefault();
          void store.testTrigger();
        }}
      >
        <TextInput
          aria-label="Пример запроса"
          value={store.phrase}
          placeholder="Пример запроса пользователя"
          onChange={(event) => {
            store.setPhrase(event.target.value);
          }}
        />
        <Button
          type="submit"
          tone="secondary"
          disabled={store.testing || store.phrase.trim().length === 0 || !described}
        >
          {store.testing ? "Проверка…" : "Проверить"}
        </Button>
      </form>
      {described ? null : (
        <span className="text-[11.5px] text-main-500">
          Заполните описание: запрос сравнивается с ним.
        </span>
      )}
      {result !== null && own !== null ? (
        <div className="flex flex-col gap-1">
          <div className="flex items-center gap-2">
            <Icon path={own.triggered ? mdiCheck : mdiClose} size={15} />
            <span className={`flex-1 text-[11.5px] ${own.triggered ? "text-ok" : "text-main-300"}`}>
              {own.triggered ? "Навык сработает" : "Навык не сработает"}
            </span>
            <span
              className={`font-mono text-[11px] ${own.triggered ? "text-ok" : "text-main-400"}`}
            >
              {formatScore(own.score)}
            </span>
          </div>
          <span className="text-[11px] text-main-500">
            Порог {formatScore(result.threshold)} · сравнение {METHOD_LABELS[result.method]}
          </span>
          {rival === null ? null : (
            <span className="flex items-start gap-1.5 text-[11px] text-warn">
              <Icon path={mdiAlertCircleOutline} size={13} />
              Выше оценён навык «{rival.name || rival.key}» ({formatScore(rival.score)}).
            </span>
          )}
        </div>
      ) : null}
    </div>
  );
});

const RunPanel = observer(function RunPanel({ store }: { readonly store: SkillStore }) {
  const navigate = useNavigate();
  const override = store.selectedSkill?.modelOverride ?? null;
  const options = [
    {
      value: "",
      label: override === null ? "Выберите модель" : `Модель навыка · ${override.modelId}`,
    },
    ...store.modelOptions.filter((option) => option.value !== modelValue(override)),
  ];
  const blocker = store.runBlocker;
  const open = (id: string) => void navigate(`${ROUTES.runs}?run=${id}`);
  return (
    <div className={CARD}>
      <TextArea
        aria-label="Запрос для запуска"
        value={store.runMessage}
        placeholder="Что нужно сделать"
        onChange={(event) => {
          store.setRunMessage(event.target.value);
        }}
      />
      <SelectInput
        label="Модель запуска"
        value={store.runModel}
        options={options}
        onChange={store.setRunModel}
      />
      <Button
        type="button"
        tone="primary"
        disabled={blocker !== null || store.running}
        title={blocker ?? undefined}
        onClick={() => {
          void store.run().then((id) => {
            if (id !== null) open(id);
          });
        }}
      >
        <Icon path={mdiPlay} size={15} />
        {store.running ? "Запуск…" : "Запустить"}
      </Button>
      {blocker === null || store.runMessage.length === 0 ? null : (
        <span className="text-[11.5px] text-main-500">{blocker}</span>
      )}
      {store.runs.length === 0 ? null : (
        <div className="flex flex-col gap-0.5 border-t border-line pt-2">
          {store.runs.slice(0, RECENT_RUNS).map((run) => (
            <button
              key={run.id}
              type="button"
              className="flex items-center gap-2 rounded-control px-1 py-1 text-left hover:bg-main-800"
              onClick={() => {
                open(run.id);
              }}
            >
              <StatusDot tone={STATUS_TONES[run.status]} title={STATUS_LABELS[run.status]} />
              <span className="min-w-0 flex-1 truncate text-[11.5px] text-main-200">
                {STATUS_LABELS[run.status]}
              </span>
              <span className="text-[10.5px] text-main-500">{formatDateTime(run.createdAt)}</span>
              <Icon path={mdiOpenInNew} size={12} />
            </button>
          ))}
        </div>
      )}
    </div>
  );
});

function SkillRail({ store, vm }: { readonly store: SkillStore; readonly vm: SkillFormVm }) {
  return (
    <aside
      aria-label="Проверка и запуск навыка"
      className="flex w-75 flex-none flex-col border-l border-line bg-main-900"
    >
      <ScrollArea className="flex min-h-0 flex-1 flex-col gap-3 px-3.5 py-4">
        <span className={HEADING}>Проверка срабатывания</span>
        <TriggerCheck store={store} vm={vm} />

        {vm.isNew ? null : (
          <>
            <span className={`${HEADING} mt-1`}>Запуск</span>
            <RunPanel store={store} />
          </>
        )}

        <button
          type="button"
          aria-expanded={store.advancedOpen}
          className={`${HEADING} mt-1 flex items-center gap-1 text-left hover:text-main-300`}
          onClick={() => {
            store.setAdvancedOpen(!store.advancedOpen);
          }}
        >
          <Icon path={store.advancedOpen ? mdiChevronDown : mdiChevronRight} size={14} />
          Дополнительно
          {store.allowedCount > 0 ? (
            <span className="ml-1 font-normal tracking-normal normal-case">
              · инструментов: {store.allowedCount}
            </span>
          ) : null}
        </button>

        {store.advancedOpen ? (
          <div className="flex flex-col gap-3">
            <Field
              label="Идентификатор"
              htmlFor="skill-key"
              help={KEY_HINT}
              error={vm.errorOf("key")}
            >
              <TextInput
                id="skill-key"
                mono
                value={vm.key}
                invalid={vm.errorOf("key") !== undefined}
                onChange={(event) => {
                  vm.setKey(event.target.value);
                }}
              />
            </Field>
            <Field label="Модель по умолчанию">
              <SelectInput
                label="Модель по умолчанию"
                value={modelValue(vm.modelOverride)}
                options={[{ value: "", label: "Не задана" }, ...store.modelOptions]}
                onChange={store.setModelOverride}
              />
            </Field>
            <span className="text-[12px] font-medium text-main-300">Разрешённые инструменты</span>
            <ScrollArea className="flex max-h-80 flex-none flex-col rounded-card border border-line bg-main-850 py-1">
              {store.toolGroups.length === 0 ? (
                <span className="px-3 py-2 text-[12px] text-main-500">
                  Инструментов в реестре нет.
                </span>
              ) : null}
              {store.toolGroups.map((group) => (
                <div key={group.key} className="flex flex-col">
                  <span className="px-2.75 pt-2 pb-1 text-[10.5px] font-medium text-main-500">
                    {group.label}
                  </span>
                  {group.rows.map((row) => (
                    <ToolLine key={row.key} row={row} vm={vm} />
                  ))}
                </div>
              ))}
            </ScrollArea>
            <p className="text-[11px] leading-snug text-main-500">
              Навык сужает права со страницы «Инструменты», но не расширяет их. Чтение файлов и
              запуск скриптов навыка добавляются сами.
            </p>
          </div>
        ) : null}
      </ScrollArea>
    </aside>
  );
}

export default observer(SkillRail);
