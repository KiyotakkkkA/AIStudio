import { mdiCubeOutline, mdiMagnify } from "@mdi/js";
import { observer } from "mobx-react-lite";
import Chip from "../../ui/atoms/Chip";
import Icon from "../../ui/atoms/Icon";
import TextInput from "../../ui/atoms/TextInput";
import EmptyState from "../../ui/molecules/EmptyState";
import useStore from "../../stores/useStore";
import ModelCard from "./ModelCard";

function ModelGrid() {
  const { providers } = useStore();
  const rows = providers.visibleRows;
  const saved = providers.detail;
  const providerName = saved?.name ?? providers.form?.name ?? "Черновик";
  const outcome = providers.probeResult?.outcome;
  const curated = outcome?.kind === "ok" && !outcome.live;

  return (
    <section className="flex min-h-0 flex-1 flex-col gap-3 rounded-card border border-main-700 bg-main-900 p-4">
      <div className="flex flex-none items-center gap-2.5">
        <h2 className="flex-1 text-[11px] font-semibold tracking-[0.08em] text-main-400 uppercase">
          Доступные модели
          <span className="ml-1.5 text-[11px] font-normal tracking-normal text-main-500 normal-case">
            {providers.rows.length === 0
              ? "— появятся после проверки связи"
              : curated
                ? "— курируемый список, а не ответ вендора"
                : "— возвращены проверкой связи"}
          </span>
        </h2>
        <span className="w-47.5 flex-none">
          <TextInput
            aria-label="Фильтр моделей"
            value={providers.modelQuery}
            placeholder="Фильтр моделей…"
            leading={<Icon path={mdiMagnify} size={14} />}
            onChange={(event) => {
              providers.setModelQuery(event.target.value);
            }}
          />
        </span>
        <Chip>{`${String(rows.length)} из ${String(providers.rows.length)}`}</Chip>
      </div>

      {providers.rows.length === 0 ? (
        <div className="flex flex-1 items-center justify-center rounded-card border border-dashed border-main-600 bg-main-800/40">
          <EmptyState
            icon={mdiCubeOutline}
            title="Моделей пока нет"
            description="Нажмите «Проверить связь» — список моделей приходит от провайдера и кэшируется локально."
          />
        </div>
      ) : rows.length === 0 ? (
        <p className="rounded-card border border-dashed border-main-600 px-3 py-4.5 text-center text-[12px] text-main-400">
          Ничего не найдено по фильтру.
        </p>
      ) : (
        <div className="grid grid-cols-2 gap-2.5 overflow-y-auto xl:grid-cols-3">
          {rows.map((row) => (
            <ModelCard
              key={row.key}
              row={row}
              providerName={providerName}
              isDefault={row.modelId !== null && saved?.defaultModelId === row.modelId}
              selectable={row.modelId !== null}
              onMakeDefault={() => {
                void providers.setDefaultModel(row.modelId);
              }}
            />
          ))}
        </div>
      )}
    </section>
  );
}

export default observer(ModelGrid);
