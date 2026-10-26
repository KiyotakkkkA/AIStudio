import { mdiClose, mdiTrashCan } from "@mdi/js";
import { observer } from "mobx-react-lite";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import { useStore } from "../../stores/useStore";
import Button from "../../ui/atoms/buttons/Button";
import IconButton from "../../ui/atoms/buttons/IconButton";
import Chip from "../../ui/atoms/Chip";
import Icon from "../../ui/atoms/Icon";
import SelectInput from "../../ui/atoms/SelectInput";
import TextInput from "../../ui/atoms/TextInput";
import ChatEvalPanel from "./ChatEvalPanel";
import { EVAL_MODE_LABELS, evalCaseRows, evalMetricRows } from "./evalRows";
import { LoadingStatus, SkeletonLines } from "../../ui/molecules/LoadingSkeleton";

const EYEBROW = "text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase";

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
    <ScrollArea role="tabpanel" className="min-h-0 flex-1 px-6 py-5">
      <div className="flex max-w-4xl flex-col gap-8">
        <section className="flex flex-col gap-3">
          <div>
            <h3 className={EYEBROW}>
              Контрольные вопросы
              {evaluation.cases.length > 0 ? (
                <span className="ml-2 font-mono tracking-normal text-main-400">
                  {evaluation.cases.length.toLocaleString("ru-RU")}
                </span>
              ) : null}
            </h3>
            <p className="mt-1 text-[12px] leading-relaxed text-main-500">
              Вопросы с заранее известным ответом: для каждого укажите документы, в которых
              находится ответ.
            </p>
          </div>
          <form
            className="flex flex-col gap-2.5"
            onSubmit={(e) => {
              e.preventDefault();
              void evaluation.add();
            }}
          >
            <TextInput
              id="vs-eval-question"
              aria-label="Вопрос"
              placeholder="Вопрос"
              value={evaluation.question}
              onChange={(e) => evaluation.set("question", e.target.value)}
              disabled={locked}
            />
            <div className="flex flex-wrap items-center gap-2.5">
              <div className="min-w-50 flex-1">
                <SelectInput
                  label="Ожидаемый документ"
                  value=""
                  options={documentOptions}
                  placeholder={
                    documents.length === 0 ? "В хранилище нет документов" : "Ожидаемый документ"
                  }
                  disabled={locked || documentOptions.length === 0}
                  onChange={evaluation.addExpected}
                />
              </div>
              <Button type="submit" disabled={!evaluation.canAdd || locked}>
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
          {cases.length === 0 && evaluation.loading ? (
            <LoadingStatus label="Загрузка вопросов…">
              <SkeletonLines count={3} />
            </LoadingStatus>
          ) : cases.length === 0 ? (
            <p className="text-[12px] text-main-500">Вопросов пока нет.</p>
          ) : (
            <ul className="m-0 flex list-none flex-col divide-y divide-line border-y border-line p-0">
              {cases.map((row) => (
                <li key={row.id} className="flex items-start gap-3 py-2.5">
                  <div className="min-w-0 flex-1">
                    <p className="text-[12.5px] text-main-100">{row.question}</p>
                    <div className="mt-0.5 flex flex-wrap gap-x-3 gap-y-0.5">
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
                    <IconButton
                      size="md"
                      path={mdiTrashCan}
                      label="Удалить вопрос"
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
                    />
                  </div>
                </li>
              ))}
            </ul>
          )}
        </section>
        <section className="flex flex-col gap-3">
          <div>
            <h3 className={EYEBROW}>Проверка поиска</h3>
            <p className="mt-1 text-[12px] leading-relaxed text-main-500">
              Каждый вопрос ищется в хранилище; оценка показывает, на каком месте оказался ожидаемый
              документ. С переранжированием результаты считаются дважды — без него и с ним.
            </p>
          </div>
          <div className="flex flex-wrap items-center gap-2.5 text-[12px] text-main-400">
            <label htmlFor="vs-eval-k">Глубина, фрагментов</label>
            <span className="w-16">
              <TextInput
                id="vs-eval-k"
                value={evaluation.k}
                onChange={(e) => evaluation.set("k", e.target.value)}
                disabled={locked}
                mono
                inputMode="numeric"
              />
            </span>
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
              <span className="text-[11px] text-main-500">
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
        </section>
        <ChatEvalPanel chat={evaluation.chat} cases={evaluation.cases.length} locked={locked} />
      </div>
    </ScrollArea>
  );
}

export default observer(VectorEvalSection);
