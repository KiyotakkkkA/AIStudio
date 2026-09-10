import { observer } from "mobx-react-lite";
import Slider from "../../ui/atoms/Slider";
import TextInput from "../../ui/atoms/TextInput";
import Field from "../../ui/molecules/Field";
import { SETTING_BOUNDS, type ProviderFormVm } from "./ProviderFormVm";
import { PARAMETER_LABELS } from "./providerPresentation";

export interface ModelSettingsCardProps {
  readonly vm: ProviderFormVm;
}

function ignoredNote(
  vm: ProviderFormVm,
  parameter: "temperature" | "topK" | "topP",
): string | undefined {
  return vm.honours(parameter) ? undefined : `Адаптер ${vm.adapter} не учитывает этот параметр.`;
}

function ModelSettingsCard({ vm }: ModelSettingsCardProps) {
  return (
    <section className="flex min-w-0 flex-1 flex-col gap-3.5 rounded-card border border-main-700 bg-main-900 p-4">
      <h2 className="text-[11px] font-semibold tracking-[0.08em] text-main-400 uppercase">
        Параметры модели
      </h2>

      <Slider
        id="provider-temperature"
        label={PARAMETER_LABELS.temperature}
        value={vm.settingOf("temperature")}
        min={SETTING_BOUNDS.temperature.min}
        max={SETTING_BOUNDS.temperature.max}
        step={SETTING_BOUNDS.temperature.step}
        disabled={!vm.honours("temperature")}
        note={ignoredNote(vm, "temperature")}
        readout={vm.settingOf("temperature").toFixed(2)}
        onChange={(value) => {
          vm.setSetting("temperature", value);
        }}
      />

      <Slider
        id="provider-top-k"
        label={PARAMETER_LABELS.topK}
        value={vm.settingOf("topK")}
        min={SETTING_BOUNDS.topK.min}
        max={200}
        step={SETTING_BOUNDS.topK.step}
        disabled={!vm.honours("topK")}
        note={ignoredNote(vm, "topK")}
        readout={String(vm.settingOf("topK"))}
        onChange={(value) => {
          vm.setSetting("topK", Math.round(value));
        }}
      />

      <Slider
        id="provider-top-p"
        label={PARAMETER_LABELS.topP}
        value={vm.settingOf("topP")}
        min={SETTING_BOUNDS.topP.min}
        max={SETTING_BOUNDS.topP.max}
        step={SETTING_BOUNDS.topP.step}
        disabled={!vm.honours("topP")}
        note={ignoredNote(vm, "topP")}
        readout={vm.settingOf("topP").toFixed(2)}
        onChange={(value) => {
          vm.setSetting("topP", value);
        }}
      />

      <div className="grid grid-cols-2 gap-3">
        <Field
          label={PARAMETER_LABELS.maxOutputTokens}
          htmlFor="provider-max-output"
          help={
            vm.honours("maxOutputTokens") ? undefined : `Адаптер ${vm.adapter} не учитывает бюджет.`
          }
        >
          <TextInput
            id="provider-max-output"
            mono
            type="number"
            min={SETTING_BOUNDS.maxOutputTokens.min}
            max={SETTING_BOUNDS.maxOutputTokens.max}
            disabled={!vm.honours("maxOutputTokens")}
            value={String(vm.settingOf("maxOutputTokens"))}
            trailing={<span className="text-[11px] text-main-500">ток</span>}
            onChange={(event) => {
              vm.setSetting("maxOutputTokens", numberOrNull(event.target.value));
            }}
          />
        </Field>
        <Field label="Таймаут запроса" htmlFor="provider-timeout">
          <TextInput
            id="provider-timeout"
            mono
            type="number"
            min={SETTING_BOUNDS.timeoutSeconds.min}
            max={SETTING_BOUNDS.timeoutSeconds.max}
            value={String(vm.settingOf("timeoutSeconds"))}
            trailing={<span className="text-[11px] text-main-500">с</span>}
            onChange={(event) => {
              vm.setSetting("timeoutSeconds", numberOrNull(event.target.value));
            }}
          />
        </Field>
      </div>
    </section>
  );
}

function numberOrNull(raw: string): number | null {
  const trimmed = raw.trim();
  if (trimmed.length === 0) return null;
  const value = Number(trimmed);
  return Number.isFinite(value) ? Math.round(value) : null;
}

export default observer(ModelSettingsCard);
