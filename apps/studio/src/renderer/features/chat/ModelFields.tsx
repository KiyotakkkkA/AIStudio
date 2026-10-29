import type { PositionAnchor, SelectOption } from "@kiyotakkkka/zvs-uikit-lib";
import type { ProviderDto } from "@zvs/shared";
import SelectInput from "../../ui/atoms/SelectInput";

export function availableModelOptions(provider: ProviderDto | undefined): SelectOption[] {
  return (provider?.models ?? [])
    .filter((model) => model.available)
    .map((model) => ({ value: model.externalId, label: model.displayName || model.externalId }));
}

export interface ModelFieldsProps {
  readonly providerTitle: string;
  readonly providerLabel: string;
  readonly providerValue: string;
  readonly providerOptions: readonly SelectOption[];
  readonly providerDisabled?: boolean;
  readonly onProviderChange: (value: string) => void;
  readonly modelTitle: string;
  readonly modelLabel: string;
  readonly modelValue: string;
  readonly modelOptions: readonly SelectOption[];
  readonly modelDisabled?: boolean;
  readonly onModelChange: (value: string) => void;
  readonly placeholder?: string;
  readonly menuPlacement?: PositionAnchor;
}

export default function ModelFields({
  providerTitle,
  providerLabel,
  providerValue,
  providerOptions,
  providerDisabled = false,
  onProviderChange,
  modelTitle,
  modelLabel,
  modelValue,
  modelOptions,
  modelDisabled = false,
  onModelChange,
  placeholder,
  menuPlacement,
}: ModelFieldsProps) {
  const shared = {
    ...(placeholder === undefined ? {} : { placeholder }),
    ...(menuPlacement === undefined ? {} : { menuPlacement }),
  };
  return (
    <>
      <div className="min-w-0">
        <p className="mb-1.5 text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase">
          {providerTitle}
        </p>
        <SelectInput
          label={providerLabel}
          value={providerValue}
          options={providerOptions}
          disabled={providerDisabled}
          onChange={onProviderChange}
          {...shared}
        />
      </div>
      <div className="min-w-0">
        <p className="mb-1.5 text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase">
          {modelTitle}
        </p>
        <SelectInput
          label={modelLabel}
          value={modelValue}
          options={modelOptions}
          disabled={modelDisabled}
          onChange={onModelChange}
          {...shared}
        />
      </div>
    </>
  );
}
