import { observer } from "mobx-react-lite";
import type { FieldDescriptor } from "@zvs/shared";
import SelectInput from "../../ui/atoms/SelectInput";
import TextInput from "../../ui/atoms/TextInput";
import Field from "../../ui/molecules/Field";
import type { SecretFormVm } from "./SecretFormVm";

export interface SecretCredentialFieldProps {
  readonly field: FieldDescriptor;
  readonly vm: SecretFormVm;
}

function SecretCredentialField({ field, vm }: SecretCredentialFieldProps) {
  const id = `secret-field-${field.key}`;
  const error = vm.errorOf(`fields.${field.key}`);
  const raw = vm.fieldValues[field.key];
  const value = typeof raw === "string" ? raw : "";

  const isSelect = field.kind === "select";

  return (
    <Field
      label={field.label}
      htmlFor={isSelect ? undefined : id}
      required={field.required}
      optionalHint={!field.required}
      help={field.help}
      error={error}
    >
      {isSelect ? (
        <SelectInput
          label={field.label}
          value={value}
          invalid={error !== undefined}
          options={(field.options ?? []).map((option) => ({
            value: option.value,
            label: option.label,
          }))}
          onChange={(next) => {
            vm.setField(field.key, next);
          }}
        />
      ) : (
        <TextInput
          id={id}
          value={value}
          mono={field.kind === "url"}
          type={field.kind === "number" ? "number" : "text"}
          placeholder={field.placeholder}
          invalid={error !== undefined}
          onChange={(event) => {
            vm.setField(field.key, event.target.value);
          }}
        />
      )}
    </Field>
  );
}

export default observer(SecretCredentialField);
