import { mdiClose, mdiKeyOutline, mdiPlus } from "@mdi/js";
import { observer } from "mobx-react-lite";
import type { SecretId, SecretSummaryDto } from "@zvs/shared";
import Button from "../../ui/atoms/buttons/Button";
import IconButton from "../../ui/atoms/buttons/IconButton";
import Chip from "../../ui/atoms/Chip";
import Icon from "../../ui/atoms/Icon";
import SelectInput from "../../ui/atoms/SelectInput";
import TextInput from "../../ui/atoms/TextInput";
import type { ServerFormVm, ValueKind, ValueTable as TableName } from "./ServerFormVm";

interface ValueTableProps {
  readonly vm: ServerFormVm;
  readonly table: TableName;
  readonly secrets: readonly SecretSummaryDto[];
  readonly namePlaceholder: string;
  readonly empty: string;
}

const KIND_LABELS: Record<ValueKind, string> = { literal: "literal", secret: "secret" };

function ValueTable({ vm, table, secrets, namePlaceholder, empty }: ValueTableProps) {
  const rows = vm.rows(table);
  const secretOptions = secrets.map((secret) => ({
    value: secret.id,
    label: secret.hint === null ? secret.name : `${secret.name} — ${secret.hint}`,
  }));

  return (
    <div className="flex flex-col gap-2">
      <div className="overflow-hidden rounded-card border border-line">
        {rows.length === 0 ? (
          <div className="px-3 py-2.5 text-[12px] text-main-500">{empty}</div>
        ) : (
          rows.map((row, index) => {
            const error = vm.rowErrorOf(table, row.id);
            return (
              <div
                key={row.id}
                className={`flex flex-col gap-1 px-2.75 py-2 ${index > 0 ? "border-t border-line" : ""}`}
              >
                <div className="flex items-center gap-2.5">
                  <span className="w-42.5 flex-none">
                    <TextInput
                      mono
                      aria-label="Имя"
                      value={row.name}
                      placeholder={namePlaceholder}
                      invalid={vm.errorOf(`${table}.${row.id}.name`) !== undefined}
                      onChange={(event) => {
                        vm.setRowName(table, row.id, event.target.value);
                      }}
                    />
                  </span>
                  {row.kind === "secret" ? (
                    <span className="flex min-w-0 flex-1 items-center gap-2">
                      <Icon path={mdiKeyOutline} size={14} className="flex-none text-accent-dark" />
                      <span className="min-w-0 flex-1">
                        <SelectInput
                          label="Секрет"
                          value={row.secretId ?? ""}
                          placeholder={
                            secretOptions.length === 0 ? "Секретов пока нет" : "Выберите секрет"
                          }
                          invalid={vm.errorOf(`${table}.${row.id}.value`) !== undefined}
                          options={secretOptions}
                          onChange={(value) => {
                            vm.setRowSecret(
                              table,
                              row.id,
                              value === "" ? null : (value as SecretId),
                            );
                          }}
                        />
                      </span>
                    </span>
                  ) : (
                    <span className="min-w-0 flex-1">
                      <TextInput
                        mono
                        aria-label="Значение"
                        value={row.literal}
                        placeholder="значение"
                        onChange={(event) => {
                          vm.setRowLiteral(table, row.id, event.target.value);
                        }}
                      />
                    </span>
                  )}
                  <Chip
                    tone={row.kind === "secret" ? "accent" : "neutral"}
                    title={
                      row.kind === "secret"
                        ? "Сделать обычным значением"
                        : "Брать значение из секрета"
                    }
                    onClick={() => {
                      vm.setRowKind(table, row.id, row.kind === "secret" ? "literal" : "secret");
                    }}
                  >
                    {KIND_LABELS[row.kind]}
                  </Chip>
                  <IconButton
                    path={mdiClose}
                    label="Удалить строку"
                    tooltip="left-center"
                    onClick={() => {
                      vm.removeRow(table, row.id);
                    }}
                  />
                </div>
                {error === undefined ? null : (
                  <span className="text-[11.5px] text-err">{error}</span>
                )}
              </div>
            );
          })
        )}
      </div>
      <div className="flex gap-1.5">
        <Button
          tone="ghost"
          onClick={() => {
            vm.addRow(table, "literal");
          }}
        >
          <Icon path={mdiPlus} size={14} />
          Значение
        </Button>
        <Button
          tone="ghost"
          onClick={() => {
            vm.addRow(table, "secret");
          }}
        >
          <Icon path={mdiPlus} size={14} />
          Секрет
        </Button>
      </div>
    </div>
  );
}

export default observer(ValueTable);
