import { observer } from "mobx-react-lite";
import { mdiFlashOutline } from "@mdi/js";
import { useStore } from "../../stores/useStore";
import Button from "../../ui/atoms/buttons/Button";
import Icon from "../../ui/atoms/Icon";
import ProbeResultLine from "./ProbeResultLine";

export const PROVIDER_CONTROL_ACTIONS_LABEL = "Управление подключением";
export const PROBE_PROVIDER = "Проверить связь";

function ProviderControlActions() {
  const { providers } = useStore();
  const form = providers.form;
  if (form === null) return null;
  return (
    <div
      role="group"
      aria-label={PROVIDER_CONTROL_ACTIONS_LABEL}
      className="mx-auto flex w-full max-w-360 flex-wrap items-center justify-between gap-3 rounded-xl bg-main-800 px-4 py-3 shadow-[0_6px_24px_rgb(0_0_0/0.12)]"
    >
      <div className="flex min-w-0 flex-1 flex-wrap items-center gap-3">
        <Button
          tone="secondary"
          disabled={providers.probing}
          onClick={() => void providers.probe()}
        >
          <Icon path={mdiFlashOutline} size={15} />
          {providers.probing
            ? form.isLocal
              ? "Поиск…"
              : "Проверка…"
            : form.isLocal
              ? "Найти модели"
              : PROBE_PROVIDER}
        </Button>
        <ProbeResultLine result={form.probeResult} probing={providers.probing} />
      </div>
      <div className="ml-auto flex flex-none items-center gap-2">
        <Button
          tone="ghost"
          disabled={!form.dirty || providers.saving}
          needConfirm
          modalSetup={{
            title: "Отменить?",
            content: "Несохранённые изменения будут потеряны.",
            confirmLabel: "Отменить",
          }}
          onClick={providers.resetForm}
        >
          Отменить
        </Button>
        <Button tone="primary" disabled={providers.saving} onClick={() => void providers.submit()}>
          {providers.saving ? "Сохранение…" : "Сохранить подключение"}
        </Button>
      </div>
    </div>
  );
}

export default observer(ProviderControlActions);
