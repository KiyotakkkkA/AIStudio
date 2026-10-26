import { mdiArrowLeftThin, mdiArrowRightThin, mdiGraphOutline, mdiTrashCan } from "@mdi/js";
import { observer } from "mobx-react-lite";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import type { VectorDocumentDto } from "@zvs/shared";
import { useStore } from "../../stores/useStore";
import Button from "../../ui/atoms/buttons/Button";
import IconButton from "../../ui/atoms/buttons/IconButton";
import Chip from "../../ui/atoms/Chip";
import Icon from "../../ui/atoms/Icon";
import SelectInput from "../../ui/atoms/SelectInput";
import EmptyState from "../../ui/molecules/EmptyState";
import Field from "../../ui/molecules/Field";

function VectorRelationsPanel() {
  const { vectorStores: store } = useStore();
  const graph = store.graph;
  const documents = store.documents;
  const documentOptions = documents.map((document) => ({
    value: document.id as string,
    label: document.sourcePath,
  }));
  const relationOptions = graph.relations.map((relation) => ({
    value: relation.id as string,
    label: graph.relationName(relation.name),
  }));
  const groups = graph.groups(documents);
  const paths = new Map(documents.map((document) => [document.id as string, document.sourcePath]));
  const busy = store.busy || graph.busy;
  return (
    <div role="tabpanel" className="flex min-h-0 flex-1 flex-col gap-3 px-6 py-5">
      <form
        className="flex flex-wrap items-end gap-2.5"
        onSubmit={(e) => {
          e.preventDefault();
          void graph.addEdge();
        }}
      >
        <div className="min-w-50 flex-1">
          <Field label="Откуда">
            <SelectInput
              label="Документ-источник"
              value={graph.fromDocumentId}
              options={documentOptions}
              placeholder="Выберите документ"
              disabled={busy || documents.length === 0}
              onChange={(value) => graph.set("fromDocumentId", value)}
            />
          </Field>
        </div>
        <div className="w-44">
          <Field label="Тип связи">
            <SelectInput
              label="Тип связи"
              value={graph.relationTypeId}
              options={relationOptions}
              placeholder="Тип связи"
              disabled={busy || relationOptions.length === 0}
              onChange={(value) => graph.set("relationTypeId", value)}
            />
          </Field>
        </div>
        <div className="min-w-50 flex-1">
          <Field label="Куда">
            <SelectInput
              label="Документ-цель"
              value={graph.toDocumentId}
              options={documentOptions}
              placeholder="Выберите документ"
              disabled={busy || documents.length === 0}
              onChange={(value) => graph.set("toDocumentId", value)}
            />
          </Field>
        </div>
        <Button type="submit" tone="primary" disabled={!graph.canAdd || busy}>
          Связать
        </Button>
      </form>
      <div className="flex flex-wrap items-center gap-2">
        <Chip>{graph.edges.length} связей</Chip>
        {store.detail?.graphStale ? (
          <Chip tone="accent" title="Граф в памяти расходится с таблицей связей">
            граф требует сверки
          </Chip>
        ) : null}
        <span className="text-[12px] text-main-500">
          Связи задаются вручную и действуют внутри одного хранилища. Направление важно для всех
          типов, кроме «Встречается вместе».
        </span>
      </div>
      {graph.error === null ? null : (
        <p role="alert" className="text-[12px] text-err">
          {graph.error}
        </p>
      )}
      {documents.length < 2 ? (
        <div className="flex min-h-0 flex-1 items-center justify-center">
          <EmptyState
            icon={mdiGraphOutline}
            title="Связывать пока нечего"
            description="В хранилище меньше двух документов. Проиндексируйте источники на вкладке «Документы»."
          />
        </div>
      ) : groups.length === 0 ? (
        <div className="flex min-h-0 flex-1 items-center justify-center">
          <EmptyState
            icon={mdiGraphOutline}
            title={graph.loading ? "Загрузка связей…" : "Связей пока нет"}
            description={
              graph.loading
                ? "Загрузка таблицы связей хранилища…"
                : "Связей нет. Добавьте связь вручную или запустите индексацию."
            }
          />
        </div>
      ) : (
        <ScrollArea className="flex min-h-0 flex-1 flex-col">
          {groups.map((group) => (
            <article key={group.document.id} className="border-b border-line py-3">
              <div className="mb-2 flex flex-wrap items-center gap-2">
                <span className="break-all font-mono text-[12px] text-main-200">
                  {group.document.sourcePath}
                </span>
                <Chip>{group.outgoing.length + group.incoming.length} связей</Chip>
              </div>
              <div className="flex flex-col gap-1.5">
                {group.outgoing.map((edge) => (
                  <EdgeLine
                    key={`out-${edge.id}`}
                    outgoing
                    relation={graph.relationName(edge.relationTypeId)}
                    other={paths.get(edge.toDocumentId) ?? edge.toDocumentId}
                    document={group.document}
                    busy={busy}
                    onRemove={() => {
                      void graph.removeEdge(edge);
                    }}
                  />
                ))}
                {group.incoming.map((edge) => (
                  <EdgeLine
                    key={`in-${edge.id}`}
                    outgoing={false}
                    relation={graph.relationName(edge.relationTypeId)}
                    other={paths.get(edge.fromDocumentId) ?? edge.fromDocumentId}
                    document={group.document}
                    busy={busy}
                    onRemove={() => {
                      void graph.removeEdge(edge);
                    }}
                  />
                ))}
              </div>
            </article>
          ))}
        </ScrollArea>
      )}
    </div>
  );
}

interface EdgeLineProps {
  readonly outgoing: boolean;
  readonly relation: string;
  readonly other: string;
  readonly document: VectorDocumentDto;
  readonly busy: boolean;
  readonly onRemove: () => void;
}

function EdgeLine({ outgoing, relation, other, document, busy, onRemove }: EdgeLineProps) {
  return (
    <div className="flex items-center gap-2">
      <Icon
        path={outgoing ? mdiArrowRightThin : mdiArrowLeftThin}
        size={16}
        className="flex-none text-main-500"
      />
      <Chip tone="accent">{relation}</Chip>
      <span className="min-w-0 flex-1 truncate font-mono text-[12px] text-main-300" title={other}>
        {other}
      </span>
      <IconButton
        tone="danger"
        size="md"
        path={mdiTrashCan}
        label="Удалить связь"
        disabled={busy}
        needConfirm
        modalSetup={{
          title: "Удалить связь?",
          content: `Связь «${relation}» ${outgoing ? "от" : "к"} ${document.sourcePath} ${outgoing ? "к" : "от"} ${other} будет удалена. Документы и их векторы останутся.`,
          tone: "danger",
          confirmLabel: "Удалить",
        }}
        onClick={onRemove}
      />
    </div>
  );
}

export default observer(VectorRelationsPanel);
