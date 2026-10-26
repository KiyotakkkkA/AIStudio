import {
  mdiClose,
  mdiFileDocumentOutline,
  mdiFilePlusOutline,
  mdiFolderOutline,
  mdiFolderPlusOutline,
  mdiStop,
  mdiTrashCan,
} from "@mdi/js";
import { observer } from "mobx-react-lite";
import { plural } from "../connections/connectionPresentation";
import { useEffect, useState } from "react";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import { useStore } from "../../stores/useStore";
import Button from "../../ui/atoms/buttons/Button";
import IconButton from "../../ui/atoms/buttons/IconButton";
import Icon from "../../ui/atoms/Icon";
import TextInput from "../../ui/atoms/TextInput";
import EmptyState from "../../ui/molecules/EmptyState";
import Field from "../../ui/molecules/Field";
import ResourceMeters from "./ResourceMeters";
import Disclosure from "./Disclosure";
import {
  formatBytes,
  formatDuration,
  formatIndexedAt,
  formatThroughput,
  splitPath,
} from "./vectorPresentation";
import ListRowSkeleton from "../../ui/molecules/ListRowSkeleton";
import { LoadingStatus } from "../../ui/molecules/LoadingSkeleton";

const DOCUMENT_ROW_HEIGHT = 52;
const DOCUMENT_OVERSCAN = 8;
const EYEBROW = "text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase";

function VectorDocumentsPanel() {
  const { vectorStores: store } = useStore();
  const busy = store.busy || store.indexing;
  return (
    <div role="tabpanel" className="flex min-h-0 flex-1 flex-col gap-5 px-6 py-5">
      <section className="flex flex-none flex-col gap-2">
        <div className="flex flex-wrap items-center gap-2">
          <h3 className={`flex-1 ${EYEBROW}`}>Источники</h3>
          <Button
            size="sm"
            disabled={busy}
            onClick={() => {
              void store.pickSource("folder");
            }}
          >
            <Icon path={mdiFolderPlusOutline} size={14} />
            Папки
          </Button>
          <Button
            size="sm"
            disabled={busy}
            onClick={() => {
              void store.pickSource("file");
            }}
          >
            <Icon path={mdiFilePlusOutline} size={14} />
            Файлы
          </Button>
        </div>
        {store.sources.length === 0 ? (
          <button
            type="button"
            disabled={busy}
            className="flex flex-col items-center gap-1 rounded-card border border-dashed border-line-strong px-4 py-6 text-center transition-colors hover:bg-main-850 disabled:opacity-50"
            onClick={() => {
              void store.pickSource("folder");
            }}
          >
            <Icon path={mdiFolderPlusOutline} size={22} className="text-main-400" />
            <span className="text-[13px] font-medium text-main-200">
              Выберите папки с документами
            </span>
            <span className="text-[12px] text-main-500">
              Можно выбрать несколько папок сразу. Файлы остаются на месте: хранилище читает их при
              индексации.
            </span>
          </button>
        ) : (
          <ul className="m-0 flex list-none flex-col divide-y divide-line p-0">
            {store.sources.map((source) => (
              <li key={source.id} className="flex min-h-10 items-center gap-2.5">
                <Icon
                  path={source.kind === "folder" ? mdiFolderOutline : mdiFileDocumentOutline}
                  size={16}
                  className="flex-none text-main-400"
                />
                <span
                  className="min-w-0 flex-1 truncate font-mono text-[12px] text-main-200"
                  title={source.path}
                >
                  {source.path}
                </span>
                {source.include.length > 0 ? (
                  <span className="max-w-60 truncate font-mono text-[11px] text-main-500">
                    {source.include.join(" ")}
                  </span>
                ) : null}
                <IconButton
                  size="md"
                  path={mdiClose}
                  iconSize={15}
                  label="Убрать источник"
                  disabled={busy}
                  onClick={() => {
                    void store.removeSource(source.id);
                  }}
                />
              </li>
            ))}
          </ul>
        )}
        <Disclosure summary="Фильтры и путь вручную">
          <form
            className="flex flex-wrap items-end gap-2.5"
            onSubmit={(e) => {
              e.preventDefault();
              store.setSourceKind("folder");
              void store.addSource();
            }}
          >
            <div className="w-48">
              <Field label="Включать файлы" htmlFor="vs-source-include">
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
            <div className="w-56">
              <Field label="Пропускать" htmlFor="vs-source-exclude">
                <TextInput
                  id="vs-source-exclude"
                  value={store.sourceExclude}
                  onChange={(e) => store.set("sourceExclude", e.target.value)}
                  disabled={busy}
                  mono
                />
              </Field>
            </div>
            <div className="min-w-50 flex-1">
              <Field label="Путь к папке" htmlFor="vs-source-path">
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
            <Button type="submit" disabled={busy || store.sourcePath.trim() === ""}>
              Добавить
            </Button>
          </form>
          <p className="mt-2 text-[11px] text-main-500">
            Маски через запятую применяются к источникам, добавленным после изменения.
          </p>
        </Disclosure>
      </section>
      <IndexProgress />
      <DocumentList />
    </div>
  );
}

const IndexProgress = observer(function IndexProgress() {
  const { vectorStores: store } = useStore();
  const run = store.indexRun;
  if (!run) return null;
  const percent = run.total > 0 ? Math.min(100, Math.round((run.done / run.total) * 100)) : 0;
  return (
    <section role="status" className="flex flex-none flex-col gap-2">
      <div className="flex items-center gap-3">
        <div className="min-w-0 flex-1">
          <div className="text-[13px] font-medium text-main-100">
            Индексация · {percent} %
            <span className="ml-2 font-normal text-main-400">
              осталось {formatDuration(store.indexEtaMs)}
            </span>
          </div>
          <div className="truncate text-[12px] text-main-500" title={run.note}>
            {run.done.toLocaleString("ru-RU")} из {run.total.toLocaleString("ru-RU")} фрагментов ·{" "}
            {run.note}
          </div>
        </div>
        <Button
          size="sm"
          tone="danger-soft"
          onClick={() => {
            void store.cancelIndex();
          }}
        >
          <Icon path={mdiStop} size={14} />
          Остановить
        </Button>
      </div>
      <div className="h-1 rounded bg-main-700">
        <div
          className="h-1 rounded bg-accent-dark transition-[width]"
          style={{ width: `${String(percent)}%` }}
        />
      </div>
      <Disclosure summary="Нагрузка на систему">
        <div className="flex flex-col gap-2">
          <span className="font-mono text-[11px] text-main-500">
            {formatThroughput(store.indexChunksPerSecond)}
          </span>
          {store.indexSample === null ? (
            <span className="text-[11px] text-main-500">Замер нагрузки…</span>
          ) : (
            <ResourceMeters sample={store.indexSample} />
          )}
        </div>
      </Disclosure>
    </section>
  );
});

const DocumentList = observer(function DocumentList() {
  const { vectorStores: store } = useStore();
  const [viewport, setViewport] = useState<HTMLDivElement | null>(null);
  const [viewportHeight, setViewportHeight] = useState(0);
  const [scrollTop, setScrollTop] = useState(0);
  useEffect(() => {
    if (viewport === null) return;
    setViewportHeight(viewport.clientHeight);
    const observer = new ResizeObserver(() => setViewportHeight(viewport.clientHeight));
    observer.observe(viewport);
    return () => observer.disconnect();
  }, [viewport]);
  const busy = store.busy || store.indexing;
  const documents = store.documents;
  if (documents.length === 0 && store.indexRun === null && store.documentsLoading)
    return (
      <LoadingStatus label="Загрузка документов…" className="flex flex-col gap-2.5 py-1">
        <ListRowSkeleton />
        <ListRowSkeleton />
        <ListRowSkeleton />
      </LoadingStatus>
    );
  if (documents.length === 0)
    return (
      <div className="flex min-h-0 flex-1 items-center justify-center">
        <EmptyState
          icon={mdiFileDocumentOutline}
          title={store.indexRun !== null ? "Первый документ ещё не готов" : "Документов пока нет"}
          description={
            store.indexRun !== null
              ? "Файл появляется в списке, когда обработаны все его фрагменты."
              : "Документы появятся здесь после индексации источников."
          }
        />
      </div>
    );
  const first = Math.max(0, Math.floor(scrollTop / DOCUMENT_ROW_HEIGHT) - DOCUMENT_OVERSCAN);
  const last = Math.min(
    documents.length,
    Math.ceil((scrollTop + viewportHeight) / DOCUMENT_ROW_HEIGHT) + DOCUMENT_OVERSCAN,
  );
  return (
    <section className="flex min-h-0 flex-1 flex-col gap-1">
      <h3 className={EYEBROW}>
        Документы
        <span className="ml-2 font-mono tracking-normal text-main-400">
          {documents.length.toLocaleString("ru-RU")}
        </span>
      </h3>
      <ScrollArea
        ref={setViewport}
        className="min-h-0 flex-1"
        onScroll={(event) => setScrollTop(event.currentTarget.scrollTop)}
      >
        <div
          className="relative"
          style={{ height: `${String(documents.length * DOCUMENT_ROW_HEIGHT)}px` }}
        >
          {documents.slice(first, last).map((document, offset) => {
            const { name, folder } = splitPath(document.sourcePath);
            return (
              <article
                key={document.id}
                className="group absolute inset-x-0 flex items-center gap-3 border-b border-line"
                style={{
                  top: `${String((first + offset) * DOCUMENT_ROW_HEIGHT)}px`,
                  height: `${String(DOCUMENT_ROW_HEIGHT)}px`,
                }}
              >
                <Icon path={mdiFileDocumentOutline} size={16} className="flex-none text-main-400" />
                <div className="min-w-0 flex-1" title={document.sourcePath}>
                  <div className="truncate text-[12.5px] font-medium text-main-100">{name}</div>
                  <div className="truncate text-[11px] text-main-500">
                    {folder === "" ? "" : `${folder} · `}
                    {plural(document.chunkCount, "фрагмент", "фрагмента", "фрагментов")} ·{" "}
                    {formatBytes(document.bytes)} · {formatIndexedAt(document.indexedAt)}
                  </div>
                </div>
                <IconButton
                  size="md"
                  tooltip="top-right"
                  toneClassName="text-main-400 hover:bg-err-soft hover:text-err"
                  path={mdiTrashCan}
                  iconSize={16}
                  label="Удалить документ"
                  disabled={busy}
                  needConfirm
                  modalSetup={{
                    title: "Удалить документ?",
                    content: `Документ ${document.sourcePath} будет удалён из хранилища. Файл на диске останется.`,
                    tone: "danger",
                    confirmLabel: "Удалить",
                  }}
                  onClick={() => {
                    void store.removeDocument(document.id);
                  }}
                />
              </article>
            );
          })}
        </div>
      </ScrollArea>
    </section>
  );
});

export default observer(VectorDocumentsPanel);
