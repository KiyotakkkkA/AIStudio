import { mdiFileDocumentOutline, mdiFolderOutline } from "@mdi/js";
import { observer } from "mobx-react-lite";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import { useStore } from "../../stores/useStore";
import Button from "../../ui/atoms/buttons/Button";
import Chip from "../../ui/atoms/Chip";
import Icon from "../../ui/atoms/Icon";
import SegmentedControl from "../../ui/atoms/SegmentedControl";
import TextInput from "../../ui/atoms/TextInput";
import EmptyState from "../../ui/molecules/EmptyState";
import Field from "../../ui/molecules/Field";
import ResourceMeters from "./ResourceMeters";
import {
  formatBytes,
  formatDuration,
  formatIndexedAt,
  formatThroughput,
} from "./vectorPresentation";

function VectorDocumentsPanel() {
  const { vectorStores: store } = useStore();
  const busy = store.busy || store.indexing;
  const run = store.indexRun;
  const percent =
    run && run.total > 0 ? Math.min(100, Math.round((run.done / run.total) * 100)) : 0;
  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3 p-4">
      <form
        className="flex flex-wrap items-end gap-2.5"
        onSubmit={(e) => {
          e.preventDefault();
          void store.addSource();
        }}
      >
        <SegmentedControl
          label="Тип источника"
          value={store.sourceKind}
          options={[
            { value: "folder", label: "Папка" },
            { value: "file", label: "Файл" },
          ]}
          onChange={store.setSourceKind}
        />
        <div className="min-w-50 flex-1">
          <Field label="Путь" htmlFor="vs-source-path">
            <TextInput
              id="vs-source-path"
              value={store.sourcePath}
              placeholder="C:\\docs\\handbook"
              onChange={(e) => store.set("sourcePath", e.target.value)}
              disabled={busy}
              mono
            />
          </Field>
        </div>
        <div className="w-40">
          <Field label="Включить" htmlFor="vs-source-include">
            <TextInput
              id="vs-source-include"
              value={store.sourceInclude}
              placeholder="**/*.md"
              onChange={(e) => store.set("sourceInclude", e.target.value)}
              disabled={busy}
              mono
            />
          </Field>
        </div>
        <div className="w-40">
          <Field label="Исключить" htmlFor="vs-source-exclude">
            <TextInput
              id="vs-source-exclude"
              value={store.sourceExclude}
              onChange={(e) => store.set("sourceExclude", e.target.value)}
              disabled={busy}
              mono
            />
          </Field>
        </div>
        <Button type="submit" disabled={busy || store.sourcePath.trim() === ""}>
          Добавить
        </Button>
        <Button
          disabled={busy}
          onClick={() => {
            void store.pickSource();
          }}
        >
          Выбрать…
        </Button>
      </form>
      <div className="flex flex-wrap items-center gap-2">
        {store.sources.length === 0 ? (
          <span className="text-[11.5px] text-main-500">
            Источники не заданы. Добавьте папку или файл, чтобы запустить индексацию.
          </span>
        ) : (
          store.sources.map((source) => (
            <span
              key={source.id}
              className="flex items-center gap-2 rounded-card border border-main-750 bg-main-800 py-1 pr-1 pl-2.5"
            >
              <Icon
                path={source.kind === "folder" ? mdiFolderOutline : mdiFileDocumentOutline}
                size={15}
                className="flex-none text-main-400"
              />
              <span className="max-w-110 truncate font-mono text-[11.5px] text-main-300">
                {source.path}
                {source.include.length > 0 ? ` · ${source.include.join(" ")}` : ""}
              </span>
              <Button
                tone="danger"
                disabled={busy}
                onClick={() => {
                  void store.removeSource(source.id);
                }}
              >
                ✕
              </Button>
            </span>
          ))
        )}
      </div>
      <div className="flex flex-wrap items-center gap-2 border-t border-main-750 pt-3">
        <Button
          tone="primary"
          disabled={busy || store.sources.length === 0}
          onClick={() => {
            void store.startIndex(false);
          }}
        >
          Индексировать
        </Button>
        <Button
          disabled={busy || store.sources.length === 0}
          title="Переиндексировать все файлы, даже неизменённые"
          onClick={() => {
            void store.startIndex(true);
          }}
        >
          Полная переиндексация
        </Button>
        {run ? (
          <Button
            tone="danger"
            onClick={() => {
              void store.cancelIndex();
            }}
          >
            Остановить
          </Button>
        ) : null}
        <Chip>{store.documents.length} документов</Chip>
      </div>
      {run ? (
        <div role="status" className="flex flex-col gap-1.5">
          <div className="flex flex-wrap items-center justify-between gap-2 text-[11.5px]">
            <span className="text-main-300">
              {run.done.toLocaleString("ru-RU")} из {run.total.toLocaleString("ru-RU")} фрагментов
              встроено
              <span className="ml-2 text-main-500">{percent} %</span>
            </span>
            <span className="max-w-110 truncate font-mono text-main-500">{run.note}</span>
          </div>
          <div className="flex flex-wrap items-center gap-x-4 gap-y-1 font-mono text-[10.5px] text-main-500">
            <span>
              осталось <span className="text-main-300">{formatDuration(store.indexEtaMs)}</span>
            </span>
            <span>{formatThroughput(store.indexChunksPerSecond)}</span>
            <span>готово {store.documents.length.toLocaleString("ru-RU")} док.</span>
          </div>
          <div className="h-1 rounded bg-main-700">
            <div className="h-1 rounded bg-accent-dark" style={{ width: `${String(percent)}%` }} />
          </div>
          {store.indexSample === null ? (
            <span className="text-[10.5px] text-main-600">Замер нагрузки…</span>
          ) : (
            <div className="mt-1 rounded-card border border-main-750 bg-main-800 px-3 py-2.5">
              <ResourceMeters sample={store.indexSample} />
            </div>
          )}
        </div>
      ) : null}
      {store.documents.length === 0 ? (
        <div className="flex min-h-0 flex-1 items-center justify-center">
          <EmptyState
            icon={mdiFileDocumentOutline}
            title={
              run !== null
                ? "Первый документ ещё не готов"
                : store.documentsLoading
                  ? "Загрузка документов…"
                  : "Документы не проиндексированы"
            }
            description={
              run !== null
                ? "Файл попадает сюда, когда все его фрагменты встроены. Большой документ может занять несколько минут."
                : "Добавьте источник и запустите индексацию — сюда попадут все файлы, чьи векторы лежат в хранилище."
            }
          />
        </div>
      ) : (
        <ScrollArea className="flex min-h-0 flex-1 flex-col gap-1.5">
          {store.documents.map((document) => (
            <article
              key={document.id}
              className="flex items-center gap-3 rounded-card border border-main-750 bg-main-800 px-3.25 py-2.5"
            >
              <div className="min-w-0 flex-1">
                <div className="truncate font-mono text-[11.5px] text-main-200">
                  {document.sourcePath}
                </div>
                <div className="mt-0.5 text-[10.5px] text-main-500">
                  {document.chunkCount.toLocaleString("ru-RU")} чанков ·{" "}
                  {formatBytes(document.bytes)} · {formatIndexedAt(document.indexedAt)}
                </div>
              </div>
              <Button
                tone="danger"
                disabled={busy}
                needConfirm
                modalSetup={{
                  title: "Удалить документ?",
                  content: `Векторы файла ${document.sourcePath} будут удалены из хранилища. Файл на диске останется.`,
                  tone: "danger",
                  confirmLabel: "Удалить",
                }}
                onClick={() => {
                  void store.removeDocument(document.id);
                }}
              >
                Удалить
              </Button>
            </article>
          ))}
        </ScrollArea>
      )}
    </div>
  );
}
export default observer(VectorDocumentsPanel);
