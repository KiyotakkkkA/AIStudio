import { mdiLayersOutline, mdiPlus, mdiRefresh } from "@mdi/js";
import { observer } from "mobx-react-lite";
import { PROVIDER_CAPABILITIES } from "@zvs/shared";
import Button from "../ui/atoms/Button";
import Chip from "../ui/atoms/Chip";
import Icon from "../ui/atoms/Icon";
import ProvidersWorkspace from "../features/providers/ProvidersWorkspace";
import { CAPABILITY_LABELS, modelsWord } from "../features/providers/providerPresentation";
import useStore from "../stores/useStore";
import PageShell from "../ui/templates/PageShell";

function ProvidersPage() {
  const { providers } = useStore();

  return (
    <PageShell
      icon={mdiLayersOutline}
      title="AI-провайдеры"
      subtitle={`${String(providers.summaries.length)} подключений · ${modelsWord(providers.discoveredModelCount)} найдено`}
      actions={
        <>
          <Button
            type="button"
            tone="secondary"
            disabled={providers.refreshing}
            onClick={() => {
              void providers.refreshAll();
            }}
          >
            <Icon path={mdiRefresh} size={15} />
            {providers.refreshing ? "Обновляем…" : "Обновить все"}
          </Button>
          <Button type="button" tone="primary" onClick={providers.requestCreate}>
            <Icon path={mdiPlus} size={15} />
            Новое подключение
          </Button>
        </>
      }
      toolbar={
        <>
          {PROVIDER_CAPABILITIES.map((capability) => {
            const active = providers.capability === capability;
            return (
              <button
                key={capability}
                type="button"
                aria-current={active ? "page" : undefined}
                className={`inline-flex h-7.5 items-center gap-1.75 rounded-lg px-3 text-[12.5px] font-medium ${
                  active ? "bg-main-700 text-main-50" : "text-main-400 hover:text-main-100"
                }`}
                onClick={() => {
                  providers.requestCapability(capability);
                }}
              >
                {CAPABILITY_LABELS[capability]}
                <Chip tone={active ? "raised" : "neutral"}>
                  {String(providers.counts[capability])}
                </Chip>
              </button>
            );
          })}
          <span className="ml-auto text-[11.5px] text-main-500">
            На всех вкладках одна форма и один список — меняется только фильтр возможностей.
          </span>
        </>
      }
    >
      <ProvidersWorkspace />
    </PageShell>
  );
}

export default observer(ProvidersPage);
