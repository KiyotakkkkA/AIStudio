import { observer } from "mobx-react-lite";
import { InputCheckBox, ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import Chip from "../../ui/atoms/Chip";
import SelectInput from "../../ui/atoms/SelectInput";
import TextInput from "../../ui/atoms/TextInput";
import Field from "../../ui/molecules/Field";
import { KEY_HINT, type SkillFormVm } from "./SkillFormVm";
import { modelValue, type ToolRow } from "./SkillStore";
import type SkillStore from "./SkillStore";

const HEADING = "text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase";

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

function SkillRail({ store, vm }: { readonly store: SkillStore; readonly vm: SkillFormVm }) {
  return (
    <aside
      aria-label="Параметры навыка"
      className="flex w-75 flex-none flex-col border-l border-line bg-main-800"
    >
      <ScrollArea className="flex min-h-0 flex-1 flex-col gap-3 px-3.5 py-4">
        <span className={HEADING}>Параметры</span>
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
            Навык сужает права со страницы «Инструменты», но не расширяет их. Чтение файлов и запуск
            скриптов навыка добавляются сами.
          </p>
        </div>
      </ScrollArea>
    </aside>
  );
}

export default observer(SkillRail);
