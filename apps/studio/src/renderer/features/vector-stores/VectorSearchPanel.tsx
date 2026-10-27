import { observer } from "mobx-react-lite";
import { mdiTextSearch } from "@mdi/js";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import { useStore } from "../../stores/useStore";
import Button from "../../ui/atoms/buttons/Button";
import Chip from "../../ui/atoms/Chip";
import TextInput from "../../ui/atoms/TextInput";
import Toggle from "../../ui/atoms/Toggle";
import SelectInput from "../../ui/atoms/SelectInput";
import { IconFloatingButton } from "../../ui/atoms/buttons/IconFloatingButton";
import EmptyState from "../../ui/molecules/EmptyState";
import { InlineSkeleton } from "../../ui/molecules/LoadingSkeleton";

function VectorSearchPanel() {
  const { vectorStores: store } = useStore();
  const graph = store.graph;
  const total = store.result
    ? store.result.embeddingMs + store.result.searchMs + (store.result.rerankMs ?? 0)
    : 0;
  const locked = store.searching || store.busy;
  return (
    <div role="tabpanel" className="flex min-h-0 flex-1 flex-col gap-3 px-6 py-5">
      <form
        className="flex flex-none flex-col gap-3 border-b border-line pb-3.5"
        onSubmit={(e) => {
          e.preventDefault();
          void store.search();
        }}
      >
        <div className="flex items-center gap-2.5">
          <div className="min-w-0 flex-1">
            <TextInput
              id="vs-query"
              aria-label="Запрос"
              value={store.query}
              placeholder="Задайте вопрос так, как его задал бы пользователь"
              onChange={(e) => store.set("query", e.target.value)}
              disabled={store.searching}
            />
          </div>
          <Button
            type="submit"
            tone="primary"
            disabled={locked || store.detail?.status === "broken"}
          >
            {store.searching ? "Поиск…" : "Найти"}
          </Button>
        </div>
        <div className="flex flex-wrap items-center gap-x-5 gap-y-2 text-[12px] text-main-400">
          <span className="flex items-center gap-2">
            <label htmlFor="vs-k">Результатов</label>
            <span className="w-14">
              <TextInput
                id="vs-k"
                value={store.k}
                onChange={(e) => store.set("k", e.target.value)}
                disabled={store.searching}
                mono
                inputMode="numeric"
              />
            </span>
          </span>
          <span className="flex items-center gap-2">
            <span>Режим</span>
            <span className="w-40">
              <SelectInput
                label="Режим поиска"
                value={store.hybridAvailable ? store.searchMode : "vector"}
                options={[
                  { value: "hybrid", label: "Гибридный" },
                  { value: "vector", label: "Только векторы" },
                ]}
                disabled={store.searching || !store.hybridAvailable}
                onChange={store.setSearchMode}
              />
            </span>
          </span>
          <span className="flex items-center gap-1.5">
            <label htmlFor="vs-score" className="mr-0.5">
              Мин. оценка
            </label>
            <span className="w-16">
              <TextInput
                id="vs-score"
                value={store.minScore}
                onChange={(e) => store.set("minScore", e.target.value)}
                disabled={store.searching}
                mono
              />
            </span>
            <IconFloatingButton label="Что означает минимальная оценка">
              <p className="mb-2 font-medium text-main-50">Минимальная оценка</p>
              <p>
                Фрагменты ниже порога показаны приглушёнными; найденные только по ключевым словам
                порогом не отсекаются. По умолчанию порог равен порогу релевантности чата (
                {store.detail?.chatMinScore ?? "—"}): приглушённые строки чат не увидит.
              </p>
            </IconFloatingButton>
          </span>
          <span className="h-5 w-px bg-line" aria-hidden />
          <span className="flex items-center gap-2.5">
            <Toggle
              id="vs-search-expand-graph"
              checked={graph.expandEnabled}
              disabled={locked}
              label="Связанные документы"
              onChange={graph.setExpandEnabled}
            />
            {graph.expandEnabled ? (
              <span className="flex items-center gap-2">
                <label htmlFor="vs-search-expand-max">не больше</label>
                <span className="w-14">
                  <TextInput
                    id="vs-search-expand-max"
                    value={graph.expandMax}
                    onChange={(event) => graph.set("expandMax", event.target.value)}
                    disabled={locked}
                    mono
                    inputMode="numeric"
                  />
                </span>
              </span>
            ) : null}
          </span>
        </div>
      </form>
      {store.result ? (
        <div role="status" className="flex flex-wrap items-center gap-2 text-[12px] text-main-300">
          <span
            title={`эмбеддинг ${store.result.embeddingMs.toFixed(1)} мс · поиск ${store.result.searchMs.toFixed(1)} мс${store.result.rerankMs === undefined ? "" : ` · переранжирование ${store.result.rerankMs.toFixed(1)} мс`}`}
          >
            {store.hitCount} результатов
            <span className="ml-1.5 text-main-500">
              за {total.toFixed(0)} мс
              {store.result.mode === undefined
                ? ""
                : store.result.mode === "hybrid"
                  ? " · гибридный"
                  : " · по векторам"}
            </span>
          </span>
          {graph.expandEnabled ? <Chip>по связям {store.graphRows.length}</Chip> : null}
          {store.result.rerank === "fallback" ? (
            <Chip
              tone="raised"
              title="Модель переранжирования не ответила — показан порядок векторного поиска"
            >
              без переранжирования
            </Chip>
          ) : null}
        </div>
      ) : null}
      {store.result && graph.expandEnabled && store.graphRows.length === 0 ? (
        <p role="status" className="text-[12px] text-main-400">
          Связи не добавили новых документов. Свяжите документы на вкладке «Связи».
        </p>
      ) : null}
      {store.result && store.hitCount === 0 ? (
        <p role="status" className="text-[12px] text-main-400">
          Нет результатов выше порога. Измените запрос или снизьте минимальную оценку в параметрах
          поиска.
        </p>
      ) : null}
      {store.result === null && !store.searching ? (
        <div className="flex min-h-0 flex-1 items-center justify-center">
          <EmptyState
            icon={mdiTextSearch}
            title="Проверьте, что найдёт хранилище"
            description="Введите вопрос: появятся фрагменты с оценкой совпадения."
          />
        </div>
      ) : null}
      {store.result === null ? null : (
        <ScrollArea className="flex min-h-0 flex-1 flex-col">
          {store.rows.map((row) => (
            <article
              key={row.id}
              className={`flex gap-4 border-b border-line py-3.5 ${row.belowFloor ? "opacity-50" : ""}`}
            >
              <div
                className="w-13 flex-none"
                title={
                  [
                    row.rerankScore === null ? null : `переранжирование ${row.rerankScore}`,
                    row.rerankScore === null ? null : `векторы ${row.vectorScore}`,
                    row.keywordScore === null ? null : `ключевые слова ${row.keywordScore}`,
                  ]
                    .filter((part) => part !== null)
                    .join(" · ") || undefined
                }
              >
                <div className="font-mono text-[13px] font-semibold text-accent-medium">
                  {row.score}
                </div>
                <div className="mt-1.25 h-1 rounded bg-main-700">
                  <div className="h-1 rounded bg-accent-dark" style={{ width: row.width }} />
                </div>
              </div>
              <div className="min-w-0 flex-1">
                <div className="mb-1 flex flex-wrap items-center gap-2">
                  <span className="break-all font-mono text-[12px] text-main-300">{row.path}</span>
                  <Chip>фрагмент {row.position}</Chip>
                  {row.place === null ? null : <Chip>{row.place}</Chip>}
                  {row.provenance === null ? null : (
                    <Chip tone="accent" title={row.provenanceTitle ?? undefined}>
                      {row.provenance}
                    </Chip>
                  )}
                  <button
                    type="button"
                    className="ml-auto text-[11px] text-main-500 hover:text-main-200 disabled:opacity-50"
                    disabled={
                      store.neighboursLoading !== null && store.neighboursLoading !== row.id
                    }
                    onClick={() => void store.toggleNeighbours(row.id)}
                  >
                    {store.neighbours.has(row.id) ? (
                      "Скрыть соседние"
                    ) : store.neighboursLoading === row.id ? (
                      <InlineSkeleton label="Загрузка…" className="w-24" />
                    ) : (
                      "Соседние фрагменты"
                    )}
                  </button>
                </div>
                {store
                  .neighbourRowsFor(row.id)
                  ?.filter((neighbour) => neighbour.before)
                  .map((neighbour) => (
                    <p
                      key={neighbour.id}
                      title={neighbour.label}
                      className="mb-1.5 border-l-2 border-line-strong pl-2 whitespace-pre-wrap wrap-break-word text-[11.5px] text-main-500"
                    >
                      {neighbour.text}
                    </p>
                  ))}
                <p className="whitespace-pre-wrap wrap-break-word text-[12px] text-main-300">
                  {row.text}
                </p>
                {store
                  .neighbourRowsFor(row.id)
                  ?.filter((neighbour) => !neighbour.before)
                  .map((neighbour) => (
                    <p
                      key={neighbour.id}
                      title={neighbour.label}
                      className="mt-1.5 border-l-2 border-line-strong pl-2 whitespace-pre-wrap wrap-break-word text-[11.5px] text-main-500"
                    >
                      {neighbour.text}
                    </p>
                  ))}
                {store.neighbourRowsFor(row.id)?.length === 0 ? (
                  <p className="mt-1.5 text-[11px] text-main-500">Соседних фрагментов нет.</p>
                ) : null}
              </div>
            </article>
          ))}
          {store.graphRows.map((row) => (
            <article key={row.id} className="flex gap-4 border-b border-line py-3.5">
              <div className="w-13 flex-none">
                <Chip tone="raised" title={`Связь с результатом ${row.via}`}>
                  {row.relation}
                </Chip>
                <div className="mt-1.25 text-[11px] text-main-500">по связям</div>
              </div>
              <div className="min-w-0 flex-1">
                <div className="mb-1 flex flex-wrap items-center gap-2">
                  <span className="break-all font-mono text-[12px] text-main-300">{row.path}</span>
                  <Chip>{row.position}</Chip>
                </div>
                <p className="whitespace-pre-wrap wrap-break-word text-[12px] text-main-300">
                  {row.text}
                </p>
              </div>
            </article>
          ))}
        </ScrollArea>
      )}
    </div>
  );
}
export default observer(VectorSearchPanel);
