import { mdiContentCopy, mdiDeleteOutline, mdiLockOutline } from "@mdi/js";
import { observer } from "mobx-react-lite";
import type { SecretTypeKey } from "@zvs/shared";
import Button from "../../ui/atoms/Button";
import Chip from "../../ui/atoms/Chip";
import Icon from "../../ui/atoms/Icon";
import SegmentedControl from "../../ui/atoms/SegmentedControl";
import SelectInput from "../../ui/atoms/SelectInput";
import TextArea from "../../ui/atoms/TextArea";
import TextInput from "../../ui/atoms/TextInput";
import Toggle from "../../ui/atoms/Toggle";
import Field from "../../ui/molecules/Field";
import TagsInput from "../../ui/molecules/TagsInput";
import SecretCredentialField from "./SecretCredentialField";
import { ROTATION_OPTIONS, type SecretFormVm } from "./SecretFormVm";
import { maskedHint, SCOPE_OPTIONS } from "./secretPresentation";

export interface SecretFormProps {
  readonly vm: SecretFormVm;
  readonly hint: string | null;
  readonly saving: boolean;
  readonly onSave: () => void;
  readonly onCancel: () => void;
  readonly onDelete: () => void;
}

function SecretForm({ vm, hint, saving, onSave, onCancel, onDelete }: SecretFormProps) {
  const secretField = vm.secretField;
  const inputs = vm.credentialFields.filter((field) => field.kind !== "boolean");
  const switches = vm.credentialFields.filter((field) => field.kind === "boolean");

  return (
    <form
      className="flex min-h-0 min-w-0 flex-1 flex-col rounded-card border border-main-700 bg-main-900"
      onSubmit={(event) => {
        event.preventDefault();
        onSave();
      }}
    >
      <div className="flex flex-none items-center gap-2.5 border-b border-main-700 px-4.5 py-4">
        <div className="min-w-0 flex-1">
          <h2 className="truncate text-[14px] font-semibold text-main-50">
            {vm.name.trim().length > 0 ? vm.name : "Новый секрет"}
          </h2>
          <p className="mt-0.5 text-[11.5px] text-main-400">
            Поля ниже сгенерированы из схемы{" "}
            <span className="font-mono text-accent-dark">{vm.schemaChip}</span>.
          </p>
        </div>
        {vm.isNew ? null : (
          <Button
            key={vm.secretId}
            type="button"
            tone="danger"
            disabled={saving}
            needConfirm
            modalSetup={{
              title: "Удалить секрет?",
              content: <>Секрет «{vm.name}» будет удалён. Это действие нельзя отменить.</>,
              tone: "danger",
              confirmLabel: "Удалить",
            }}
            onClick={onDelete}
          >
            <Icon path={mdiDeleteOutline} size={15} />
            Удалить
          </Button>
        )}
      </div>

      <div className="flex min-h-0 flex-1 flex-col gap-5 overflow-y-auto p-4.5">
        {vm.banner === null ? null : (
          <p
            role="alert"
            className="rounded-[6px] border border-err-border bg-main-800 px-3 py-2.25 text-[12px] text-err"
          >
            {vm.banner}
          </p>
        )}

        <section className="flex flex-col gap-3">
          <h3 className="text-[11px] font-semibold tracking-[0.08em] text-main-400 uppercase">
            Общее
          </h3>
          <div className="grid grid-cols-2 gap-3.5">
            <Field label="Тип секрета">
              <SelectInput
                label="Тип секрета"
                value={vm.type}
                onChange={(value) => {
                  vm.setType(value as SecretTypeKey);
                }}
                options={vm.types.map((schema) => ({ value: schema.key, label: schema.label }))}
              />
            </Field>
            <Field
              label="Отображаемое имя"
              htmlFor="secret-name"
              required
              error={vm.errorOf("name")}
            >
              <TextInput
                id="secret-name"
                value={vm.name}
                maxLength={128}
                placeholder="Ollama — личный"
                invalid={vm.errorOf("name") !== undefined}
                onChange={(event) => {
                  vm.setName(event.target.value);
                }}
              />
            </Field>
          </div>
          <Field label="Область видимости">
            <SegmentedControl
              label="Область видимости"
              value={vm.scope}
              options={SCOPE_OPTIONS}
              onChange={vm.setScope}
            />
          </Field>
        </section>

        <section className="flex flex-col gap-3 rounded-[9px] border border-main-700 bg-main-800 p-3.5">
          <div className="flex items-center gap-2">
            <h3 className="flex-1 text-[11px] font-semibold tracking-[0.08em] text-main-400 uppercase">
              {vm.schema.label}
            </h3>
            <Chip tone="accent">{`schema ${vm.schemaChip}`}</Chip>
          </div>

          {secretField === undefined ? null : (
            <Field
              label={secretField.label}
              htmlFor="secret-value"
              required={vm.valueRequired}
              optionalHint={secretField.required !== true}
              help={
                vm.isNew
                  ? secretField.help
                  : "Оставьте поле пустым, чтобы сохранённое значение осталось прежним."
              }
              error={vm.errorOf("value")}
            >
              <TextInput
                id="secret-value"
                type="password"
                autoComplete="off"
                spellCheck={false}
                mono
                value={vm.value}
                invalid={vm.errorOf("value") !== undefined}
                placeholder={vm.isNew ? (secretField.placeholder ?? "") : maskedHint(hint)}
                onChange={(event) => {
                  vm.setValue(event.target.value);
                }}
                trailing={
                  hint === null ? null : (
                    <button
                      type="button"
                      title="Скопировать маску значения"
                      className="flex-none text-main-400 hover:text-main-100"
                      onClick={() => {
                        void navigator.clipboard?.writeText(hint);
                      }}
                    >
                      <Icon path={mdiContentCopy} size={15} />
                    </button>
                  )
                }
              />
            </Field>
          )}

          {inputs.length === 0 ? null : (
            <div className="grid grid-cols-2 gap-3.5">
              {inputs.map((field) => (
                <SecretCredentialField key={field.key} field={field} vm={vm} />
              ))}
            </div>
          )}

          {switches.map((field) => (
            <Toggle
              key={field.key}
              id={`secret-field-${field.key}`}
              label={field.label}
              checked={vm.fieldValues[field.key] === true}
              onChange={(checked) => {
                vm.setField(field.key, checked);
              }}
            />
          ))}
        </section>

        <section className="flex flex-col gap-3">
          <h3 className="text-[11px] font-semibold tracking-[0.08em] text-main-400 uppercase">
            Метаданные
          </h3>
          <div className="grid grid-cols-2 gap-3.5">
            <Field label="Теги" htmlFor="secret-tags">
              <TagsInput id="secret-tags" tags={vm.tags} onChange={vm.setTags} />
            </Field>
            <Field label="Напоминание о ротации">
              <SelectInput
                label="Напоминание о ротации"
                value={vm.rotationDays === null ? "" : String(vm.rotationDays)}
                options={[...ROTATION_OPTIONS]}
                onChange={(value) => {
                  vm.setRotationDays(value === "" ? null : Number(value));
                }}
              />
            </Field>
          </div>
          <Field label="Заметка" htmlFor="secret-note">
            <TextArea
              id="secret-note"
              value={vm.note}
              maxLength={2000}
              placeholder="Для чего этот ключ и где его не стоит использовать."
              onChange={(event) => {
                vm.setNote(event.target.value);
              }}
            />
          </Field>
        </section>
      </div>

      <div className="flex flex-none items-center gap-3 border-t border-main-700 px-4.5 py-3.5">
        <Icon path={mdiLockOutline} size={15} className="flex-none text-ok" />
        <span
          className="line-clamp-2 min-w-0 flex-1 text-[11.5px] text-main-400"
          title="Значение шифруется хранилищем учётных данных операционной системы и не покидает эту машину, пока его не запросит провайдер."
        >
          Значение шифруется хранилищем учётных данных операционной системы и не покидает эту
          машину, пока его не запросит провайдер.
        </span>
        <Button type="button" tone="secondary" onClick={onCancel} disabled={saving}>
          Отмена
        </Button>
        <Button type="submit" tone="primary" disabled={saving}>
          {saving ? "Сохранение…" : "Сохранить секрет"}
        </Button>
      </div>
    </form>
  );
}

export default observer(SecretForm);
