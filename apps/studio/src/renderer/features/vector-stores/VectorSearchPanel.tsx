import { observer } from "mobx-react-lite";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import { useStore } from "../../stores/useStore";
import Button from "../../ui/atoms/Button";
import Chip from "../../ui/atoms/Chip";
import TextInput from "../../ui/atoms/TextInput";
import Field from "../../ui/molecules/Field";

function VectorSearchPanel() {
  const { vectorStores: store } = useStore();
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
      {store.result ? (
        <div className="flex flex-wrap items-center gap-2" role="status">
          <Chip tone="accent">
            {store.hitCount} результатов ·{" "}
            {(store.result.embeddingMs + store.result.searchMs).toFixed(1)} мс
          </Chip>
          <Chip>embedding {store.result.embeddingMs.toFixed(1)} мс</Chip>
          <Chip>поиск {store.result.searchMs.toFixed(1)} мс</Chip>
        </div>
      ) : null}
      <p className="text-[11.5px] text-main-500">
        Результаты только для чтения. Чанки ниже порога показаны приглушёнными.
      </p>
      {store.result && store.hitCount === 0 ? (
        <p role="status" className="text-xs text-main-400">
          Нет результатов выше порога. Попробуйте снизить минимальную оценку или изменить запрос.
        </p>
      ) : null}
      <ScrollArea className="flex min-h-0 flex-1 flex-col gap-2">
        {store.rows.map((row) => (
          <article
            key={row.id}
            className={`flex gap-3 rounded-card border border-main-700 bg-main-800 px-3.25 py-3 ${row.belowFloor ? "opacity-50" : ""}`}
          >
            <div className="w-13 flex-none">
              <div className="font-mono text-sm font-semibold text-accent-medium">{row.score}</div>
              <div className="mt-1.25 h-0.75 rounded bg-main-700">
                <div className="h-0.75 rounded bg-accent-dark" style={{ width: row.width }} />
              </div>
            </div>
            <div className="min-w-0 flex-1">
              <div className="mb-1 flex flex-wrap items-center gap-2">
                <span className="break-all font-mono text-[11.5px] text-main-300">{row.path}</span>
                <Chip>чанк {row.position}</Chip>
              </div>
              <p className="whitespace-pre-wrap wrap-break-word text-xs text-main-300">
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
