import { observer } from "mobx-react-lite";
import { Accordion, ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import { useStore } from "../../stores/useStore";
import Button from "../../ui/atoms/buttons/Button";
import Chip from "../../ui/atoms/Chip";
import TextInput from "../../ui/atoms/TextInput";
import Toggle from "../../ui/atoms/Toggle";
import Field from "../../ui/molecules/Field";
import SelectInput from "../../ui/atoms/SelectInput";
import VectorEvalSection from "./VectorEvalSection";

function VectorSearchPanel() {
  const { vectorStores: store } = useStore();
  const graph = store.graph;
  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3 p-4">
      <form
        className="flex items-end gap-2.5"
        onSubmit={(e) => {
          e.preventDefault();
          void store.search();
        }}
      >
        <div className="min-w-0 flex-1">
          <Field label="Запрос" htmlFor="vs-query">
            <TextInput
              id="vs-query"
              value={store.query}
              onChange={(e) => store.set("query", e.target.value)}
              disabled={store.searching}
            />
          </Field>
        </div>
        <div className="w-22">
          <Field label="Top K" htmlFor="vs-k">
            <TextInput
              id="vs-k"
              value={store.k}
              onChange={(e) => store.set("k", e.target.value)}
              disabled={store.searching}
              mono
            />
          </Field>
        </div>
        <div className="w-44">
          <Field label="Режим">
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
          </Field>
        </div>
        <div className="w-27.5">
          <Field label="Мин. оценка" htmlFor="vs-score">
            <TextInput
              id="vs-score"
              value={store.minScore}
              onChange={(e) => store.set("minScore", e.target.value)}
              disabled={store.searching}
              mono
            />
          </Field>
        </div>
        <Button
          type="submit"
          tone="primary"
          disabled={store.searching || store.busy || store.detail?.status === "broken"}
        >
          {store.searching ? "Поиск…" : "Найти"}
        </Button>
      </form>
      <Accordion
        defaultOpen={false}
        className="flex-none rounded-[9px] border border-line bg-main-900"
      >
        <Accordion.Summary className="min-h-12 px-4 text-left text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase hover:text-main-200">
          Расширенные настройки
        </Accordion.Summary>
        <Accordion.Content className="flex flex-col gap-3.25 border-t border-line p-4">
          <Toggle
            id="vs-search-expand-graph"
            checked={graph.expandEnabled}
            disabled={store.searching || store.busy}
            label="Расширение поиска по графу"
            onChange={graph.setExpandEnabled}
          />
          <p className="text-[11px] leading-relaxed text-main-500">
            Добавляет к результатам поиска связанные документы. Они не ранжируются и отображаются
            после основных результатов.
          </p>
          <Field
            label="Максимум документов по графу"
            htmlFor="vs-search-expand-max"
            help="Жёсткий лимит документов, добавляемых за один поиск."
          >
            <TextInput
              id="vs-search-expand-max"
              value={graph.expandMax}
              onChange={(event) => graph.set("expandMax", event.target.value)}
              disabled={store.searching || store.busy || !graph.expandEnabled}
              mono
              inputMode="numeric"
            />
          </Field>
        </Accordion.Content>
      </Accordion>
      <VectorEvalSection />
      {store.result ? (
        <div className="flex flex-wrap items-center gap-2" role="status">
          <Chip tone="accent">
            {store.hitCount} результатов ·{" "}
            {(
              store.result.embeddingMs +
              store.result.searchMs +
              (store.result.rerankMs ?? 0)
            ).toFixed(1)}{" "}
            мс
          </Chip>
          {store.result.mode === undefined ? null : (
            <Chip>{store.result.mode === "hybrid" ? "гибридный" : "векторный"}</Chip>
          )}
          {graph.expandEnabled ? <Chip>по графу {store.graphRows.length}</Chip> : null}
          <Chip>embedding {store.result.embeddingMs.toFixed(1)} мс</Chip>
          <Chip>поиск {store.result.searchMs.toFixed(1)} мс</Chip>
          {store.result.rerankMs === undefined ? null : (
            <Chip>rerank {store.result.rerankMs.toFixed(1)} мс</Chip>
          )}
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
      <p className="text-[12px] text-main-500">
        Результаты только для чтения. Чанки ниже порога показаны приглушёнными; найденные только по
        ключевым словам порогом не отсекаются. По умолчанию порог равен порогу релевантности чата (
        {store.detail?.chatMinScore ?? "—"}): приглушённые строки чат не увидит.
      </p>
      {store.result && graph.expandEnabled && store.graphRows.length === 0 ? (
        <p role="status" className="text-[12px] text-main-400">
          Граф не добавил ничего нового: у найденных документов нет связей за пределами выдачи.
          Свяжите документы на вкладке «Связи».
        </p>
      ) : null}
      {store.result && store.hitCount === 0 ? (
        <p role="status" className="text-[12px] text-main-400">
          Нет результатов выше порога. Попробуйте снизить минимальную оценку или изменить запрос.
        </p>
      ) : null}
      <ScrollArea className="flex min-h-0 flex-1 flex-col gap-2">
        {store.rows.map((row) => (
          <article
            key={row.id}
            className={`flex gap-3 rounded-card bg-main-900 shadow-card px-3.25 py-3 ${row.belowFloor ? "opacity-50" : ""}`}
          >
            <div className="w-13 flex-none">
              <div className="font-mono text-[13px] font-semibold text-accent-medium">
                {row.score}
              </div>
              <div className="mt-1.25 h-1 rounded bg-main-700">
                <div className="h-1 rounded bg-accent-dark" style={{ width: row.width }} />
              </div>
              {row.rerankScore === null && row.keywordScore === null ? null : (
                <div className="mt-1.5 flex flex-col gap-0.5 font-mono text-[10.5px] text-main-500">
                  {row.rerankScore === null ? null : (
                    <span title="Оценка кросс-энкодера">rr {row.rerankScore}</span>
                  )}
                  {row.rerankScore === null ? null : (
                    <span title="Косинусная близость векторов">vec {row.vectorScore}</span>
                  )}
                  {row.keywordScore === null ? null : (
                    <span title="Оценка BM25 по ключевым словам">bm25 {row.keywordScore}</span>
                  )}
                </div>
              )}
            </div>
            <div className="min-w-0 flex-1">
              <div className="mb-1 flex flex-wrap items-center gap-2">
                <span className="break-all font-mono text-[12px] text-main-300">{row.path}</span>
                <Chip>чанк {row.position}</Chip>
                {row.place === null ? null : <Chip>{row.place}</Chip>}
                {row.provenance === null ? null : (
                  <Chip tone="accent" title={row.provenanceTitle ?? undefined}>
                    {row.provenance}
                  </Chip>
                )}
                <button
                  type="button"
                  className="ml-auto text-[11px] text-main-500 hover:text-main-200 disabled:opacity-50"
                  disabled={store.neighboursLoading !== null && store.neighboursLoading !== row.id}
                  onClick={() => void store.toggleNeighbours(row.id)}
                >
                  {store.neighbours.has(row.id)
                    ? "Скрыть соседние"
                    : store.neighboursLoading === row.id
                      ? "Загрузка…"
                      : "Соседние фрагменты"}
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
          <article
            key={row.id}
            className="flex gap-3 rounded-card bg-main-800 bg-main-800 px-3.25 py-3"
          >
            <div className="w-13 flex-none">
              <Chip tone="raised" title={`Связь с результатом ${row.via}`}>
                {row.relation}
              </Chip>
              <div className="mt-1.25 text-[11px] text-main-500">по графу</div>
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
    </div>
  );
}
export default observer(VectorSearchPanel);
