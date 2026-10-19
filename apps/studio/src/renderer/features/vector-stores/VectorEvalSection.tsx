import { mdiClose, mdiTrashCan } from "@mdi/js";
import { observer } from "mobx-react-lite";
import { Accordion, ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import { useStore } from "../../stores/useStore";
import Button from "../../ui/atoms/buttons/Button";
import Chip from "../../ui/atoms/Chip";
import Icon from "../../ui/atoms/Icon";
import SelectInput from "../../ui/atoms/SelectInput";
import TextInput from "../../ui/atoms/TextInput";
import Field from "../../ui/molecules/Field";
import { EVAL_MODE_LABELS, evalCaseRows, evalMetricRows } from "./evalRows";

function VectorEvalSection() {
  const { vectorStores: store } = useStore();
  const evaluation = store.evaluation;
  const documents = store.documents;
  const paths = new Map(documents.map((document) => [document.id as string, document.sourcePath]));
  const documentOptions = documents
    .filter((document) => !evaluation.expected.includes(document.id))
    .map((document) => ({ value: document.id as string, label: document.sourcePath }));
  const run = evaluation.lastRun;
  const metrics = evalMetricRows(run);
  const cases = evalCaseRows(evaluation.cases, run, documents);
  const locked = store.busy || store.searching || evaluation.busy || evaluation.running;
  return (
    <Accordion
      defaultOpen={false}
      className="flex-none rounded-[9px] border border-line bg-main-900"
    >
      <Accordion.Summary className="min-h-12 px-4 text-left text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase hover:text-main-200">
        Проверка качества
        {evaluation.cases.length > 0 ? (
          <span className="ml-2 font-mono normal-case tracking-normal text-main-400">
            {evaluation.cases.length} вопр.
            {run === null
              ? ""
              : ` · MRR ${run.modes.map((mode) => mode.metrics.mrr.toFixed(3)).join(" → ")}`}
          </span>
        ) : null}
      </Accordion.Summary>
      <Accordion.Content className="border-t border-line">
        <ScrollArea className="flex max-h-[46vh] flex-col gap-3.25 p-4">
          <p className="text-[11px] leading-relaxed text-main-500">
            Вопросы с заранее известным ответом. Прогон ищет каждый вопрос и смотрит, на каком месте
            оказался ожидаемый документ. Если у хранилища включено переранжирование, результаты
            считаются дважды — без него и с ним.
          </p>
          <form
            className="flex flex-col gap-2.5"
            onSubmit={(e) => {
              e.preventDefault();
              void evaluation.add();
            }}
          >
            <Field label="Вопрос" htmlFor="vs-eval-question">
              <TextInput
                id="vs-eval-question"
                value={evaluation.question}
                onChange={(e) => evaluation.set("question", e.target.value)}
                disabled={locked}
              />
            </Field>
            <div className="flex flex-wrap items-end gap-2.5">
              <div className="min-w-50 flex-1">
                <Field label="Ожидаемые документы">
                  <SelectInput
                    label="Ожидаемый документ"
                    value=""
                    options={documentOptions}
                    placeholder={
                      documents.length === 0 ? "В хранилище нет документов" : "Добавить документ"
                    }
                    disabled={locked || documentOptions.length === 0}
                    onChange={evaluation.addExpected}
                  />
                </Field>
              </div>
              <Button type="submit" tone="primary" disabled={!evaluation.canAdd || locked}>
                Добавить вопрос
              </Button>
            </div>
            {evaluation.expected.length > 0 ? (
              <div className="flex flex-wrap gap-1.5">
                {evaluation.expected.map((id) => (
                  <Chip
                    key={id}
                    tone="accent"
                    title="Убрать из ожидаемых"
                    onClick={() => evaluation.removeExpected(id)}
                  >
                    <span className="max-w-72 truncate font-mono">{paths.get(id) ?? id}</span>
                    <Icon path={mdiClose} size={12} />
                  </Chip>
                ))}
              </div>
            ) : null}
          </form>
          <div className="flex flex-wrap items-end gap-2.5 border-t border-line pt-3.25">
            <div className="w-30">
              <Field label="Глубина, чанков" htmlFor="vs-eval-k">
                <TextInput
                  id="vs-eval-k"
                  value={evaluation.k}
                  onChange={(e) => evaluation.set("k", e.target.value)}
                  disabled={locked}
                  mono
                  inputMode="numeric"
                />
              </Field>
            </div>
            <Button
              tone="primary"
              disabled={!evaluation.canRun || locked || store.detail?.status === "broken"}
              onClick={() => {
                void evaluation.run();
              }}
            >
              {evaluation.running ? "Проверка…" : "Запустить проверку"}
            </Button>
            {run === null ? null : (
              <span className="pb-1.5 text-[11px] text-main-500">
                Последний прогон: {new Date(run.ranAt).toLocaleString("ru-RU")} · top-{run.k} ·{" "}
                {(run.durationMs / 1000).toFixed(1)} с
              </span>
            )}
          </div>
          {evaluation.error === null ? null : (
            <p role="alert" className="text-[12px] text-err">
              {evaluation.error}
            </p>
          )}
          {run === null ? null : (
            <ScrollArea orientation="horizontal" className="rounded-card border border-line">
              <table className="w-full text-[12px]">
                <thead>
                  <tr className="border-b border-line text-left text-[10.5px] tracking-[0.12em] text-main-500 uppercase">
                    <th className="px-3 py-2 font-semibold">Метрика</th>
                    {run.modes.map((mode) => (
                      <th key={mode.mode} className="px-3 py-2 font-semibold">
                        {EVAL_MODE_LABELS[mode.mode]}
                      </th>
                    ))}
                    {run.modes.length > 1 ? <th className="px-3 py-2 font-semibold">Δ</th> : null}
                  </tr>
                </thead>
                <tbody>
                  {metrics.map((row) => (
                    <tr key={row.key} className="border-b border-line last:border-b-0">
                      <td className="px-3 py-1.75 font-mono text-main-300">{row.label}</td>
                      {row.values.map((value, index) => (
                        <td key={index} className="px-3 py-1.75 font-mono text-main-100">
                          {value}
                        </td>
                      ))}
                      {row.delta === null ? null : (
                        <td
                          className={`px-3 py-1.75 font-mono ${row.improved === null ? "text-main-500" : row.improved ? "text-ok" : "text-err"}`}
                        >
                          {row.delta}
                        </td>
                      )}
                    </tr>
                  ))}
                </tbody>
              </table>
            </ScrollArea>
          )}
          {run !== null && run.modes.some((mode) => mode.fallbacks > 0) ? (
            <p role="status" className="text-[12px] text-warn">
              Переранжирование не сработало в{" "}
              {run.modes.reduce((total, mode) => total + mode.fallbacks, 0)} поисках — для них учтён
              порядок векторного поиска. Подробности в журнале.
            </p>
          ) : null}
          {cases.length === 0 ? (
            <p className="text-[12px] text-main-400">
              {evaluation.loading
                ? "Загрузка вопросов…"
                : "Вопросов пока нет. Добавьте несколько вопросов, ответ на которые вы знаете."}
            </p>
          ) : (
            <div className="flex flex-col gap-1.5">
              {cases.map((row) => (
                <div
                  key={row.id}
                  className="flex items-start gap-3 rounded-card border border-line bg-main-900 px-3.25 py-2.5"
                >
                  <div className="min-w-0 flex-1">
                    <p className="text-[12.5px] text-main-200">{row.question}</p>
                    <div className="mt-1 flex flex-wrap gap-x-3 gap-y-0.5">
                      {row.expected.map((path) => (
                        <span key={path} className="break-all font-mono text-[11px] text-main-500">
                          {path}
                        </span>
                      ))}
                    </div>
                  </div>
                  <div className="flex flex-none flex-wrap items-center justify-end gap-1.5">
                    {row.ranks.map((rank) => (
                      <Chip
                        key={rank.mode}
                        tone={rank.missed ? "neutral" : "accent"}
                        title={rank.error ?? EVAL_MODE_LABELS[rank.mode]}
                      >
                        {rank.mode === "rerank" ? "rr " : "vec "}
                        {rank.label}
                      </Chip>
                    ))}
                    <Button
                      tone="danger"
                      disabled={locked}
                      needConfirm
                      modalSetup={{
                        title: "Удалить вопрос?",
                        content: `Вопрос «${row.question}» больше не будет участвовать в проверке.`,
                        tone: "danger",
                        confirmLabel: "Удалить",
                      }}
                      onClick={() => {
                        void evaluation.remove(row.id);
                      }}
                    >
                      <Icon path={mdiTrashCan} size={15} />
                    </Button>
                  </div>
                </div>
              ))}
            </div>
          )}
        </ScrollArea>
      </Accordion.Content>
    </Accordion>
  );
}

export default observer(VectorEvalSection);
