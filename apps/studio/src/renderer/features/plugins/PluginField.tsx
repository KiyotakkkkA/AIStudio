import { observer } from "mobx-react-lite";
import { useNavigate } from "react-router-dom";
import type { FieldDescriptor } from "@zvs/shared";
import SelectInput from "../../ui/atoms/SelectInput";
import TextInput from "../../ui/atoms/TextInput";
import Toggle from "../../ui/atoms/Toggle";
import Field from "../../ui/molecules/Field";
import { ROUTES } from "../../app/routes";
import { configKey, type PluginFormVm, type SecretOption } from "./PluginFormVm";

export const SECRET_PLACEHOLDER = "Выберите секрет";
export const OPEN_SECRETS = "Секреты";

export interface PluginFieldProps {
  readonly field: FieldDescriptor;
  readonly vm: PluginFormVm;
  readonly secrets: readonly SecretOption[];
}

function PluginField({ field, vm, secrets }: PluginFieldProps) {
  const navigate = useNavigate();
  const id = `plugin-field-${field.key}`;
  const error = vm.errorOf(configKey(field.key));
  const raw = vm.values[field.key];
  const text = typeof raw === "string" ? raw : "";
  const required = vm.isRequired(field);

  if (field.kind === "boolean") {
    return (
      <div className="min-w-0 sm:col-span-2">
        <Toggle
          id={id}
          checked={raw === true}
          label={field.label}
          onChange={(checked) => {
            vm.setValue(field.key, checked);
          }}
        />
        {field.help === undefined ? null : (
          <p className="mt-1.5 text-[11.5px] leading-snug text-main-500">{field.help}</p>
        )}
      </div>
    );
  }

  return (
    <Field
      label={field.label}
      htmlFor={id}
      required={required}
      optionalHint={!required}
      help={field.help}
      error={error}
      labelAction={
        field.kind === "secret" ? (
          <button
            type="button"
            className="ml-auto text-[11.5px] font-medium text-accent-dark hover:text-accent-medium"
            onClick={() => {
              void navigate(ROUTES.secrets);
            }}
          >
            {OPEN_SECRETS}
          </button>
        ) : undefined
      }
    >
      {field.kind === "select" ? (
        <SelectInput
          label={field.label}
          value={text}
          invalid={error !== undefined}
          options={(field.options ?? []).map((option) => ({
            value: option.value,
            label: option.label,
          }))}
          onChange={(next) => {
            vm.setValue(field.key, next);
          }}
        />
      ) : field.kind === "secret" ? (
        <SelectInput
          label={field.label}
          value={text}
          searchable
          placeholder={SECRET_PLACEHOLDER}
          invalid={error !== undefined}
          options={secrets}
          onChange={(next) => {
            vm.setValue(field.key, next);
          }}
        />
      ) : (
        <TextInput
          id={id}
          value={text}
          mono={field.kind === "url"}
          type={field.kind === "number" ? "number" : "text"}
          inputMode={field.kind === "number" ? "numeric" : undefined}
          placeholder={field.placeholder}
          invalid={error !== undefined}
          onChange={(event) => {
            vm.setValue(field.key, event.target.value);
          }}
        />
      )}
    </Field>
  );
}

export default observer(PluginField);
