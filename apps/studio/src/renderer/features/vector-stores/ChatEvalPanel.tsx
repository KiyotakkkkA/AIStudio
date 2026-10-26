import { useEffect } from "react";
import { observer } from "mobx-react-lite";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import Button from "../../ui/atoms/buttons/Button";
import SelectInput from "../../ui/atoms/SelectInput";
import type ChatEvalStore from "./ChatEvalStore";
import { CHAT_EVAL_MODE_LABELS } from "./ChatEvalStore";
import Skeleton from "../../ui/atoms/Skeleton";
import { LoadingStatus } from "../../ui/molecules/LoadingSkeleton";

function percent(value: number): string {
  return `${(value * 100).toFixed(0)}%`;
}

function ChatEvalPanel({
  chat,
  cases,
  locked,
}: {
  readonly chat: ChatEvalStore;
  readonly cases: number;
  readonly locked: boolean;
}) {
  useEffect(() => {
    void chat.loadModels();
  }, [chat]);
  const run = chat.run;
  return (
    <section className="flex flex-col gap-3">
      <div>
        <h3 className="text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase">
          Проверка в чате
        </h3>
        <p className="mt-1 text-[12px] leading-relaxed text-main-500">
          Каждый вопрос задаётся модели во временном диалоге с этим хранилищем — сначала с поиском
          перед ответом, затем в режиме, где поиском управляет модель. Проверка считается
          пройденной, если ответ сослался на ожидаемый документ. Временные диалоги удаляются после
          проверки.
        </p>
      </div>
      <div className="flex flex-wrap items-center gap-2.5">
        <div className="min-w-60 flex-1">
          {chat.loaded ? (
            <SelectInput
              label="Модель для проверки"
              value={chat.modelKey}
              options={chat.models.map((model) => ({ value: model.key, label: model.label }))}
              placeholder="Нет доступных текстовых моделей"
              disabled={chat.running || chat.models.length === 0}
              onChange={chat.setModel}
            />
          ) : (
            <LoadingStatus label="Загрузка моделей…">
              <Skeleton rounded="lg" className="h-9 w-full" />
            </LoadingStatus>
          )}
        </div>
        <Button
          tone="secondary"
          disabled={!chat.canRun(cases) || locked}
          onClick={() => {
            void chat.evaluate();
          }}
        >
          {chat.running ? "Проверка в чате…" : "Проверить в чате"}
        </Button>
      </div>
      {chat.error === null ? null : (
        <p role="alert" className="text-[12px] text-err">
          {chat.error}
        </p>
      )}
      {run === null ? null : (
        <ScrollArea orientation="horizontal" className="rounded-card border border-line">
          <table className="w-full text-[12px]">
            <thead>
              <tr className="border-b border-line text-left text-[10.5px] tracking-[0.12em] text-main-500 uppercase">
                <th className="px-3 py-2 font-semibold">Вопрос</th>
                {run.modes.map((mode) => (
                  <th key={mode.mode} className="px-3 py-2 font-semibold">
                    {CHAT_EVAL_MODE_LABELS[mode.mode]}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              <tr className="border-b border-line">
                <td className="px-3 py-1.75 font-mono text-main-300">Найден ожидаемый</td>
                {run.modes.map((mode) => (
                  <td key={mode.mode} className="px-3 py-1.75 font-mono text-main-100">
                    {percent(mode.hitRate)}
                  </td>
                ))}
              </tr>
              <tr className="border-b border-line">
                <td className="px-3 py-1.75 font-mono text-main-300">Поисков на вопрос</td>
                {run.modes.map((mode) => (
                  <td key={mode.mode} className="px-3 py-1.75 font-mono text-main-100">
                    {mode.averageSearches.toFixed(1)}
                  </td>
                ))}
              </tr>
              {(run.modes[0]?.cases ?? []).map((entry, index) => (
                <tr key={entry.caseId} className="border-b border-line last:border-b-0">
                  <td
                    className="max-w-72 truncate px-3 py-1.75 text-main-300"
                    title={entry.question}
                  >
                    {entry.question}
                  </td>
                  {run.modes.map((mode) => {
                    const result = mode.cases[index];
                    if (result === undefined) return <td key={mode.mode} />;
                    return (
                      <td
                        key={mode.mode}
                        title={result.error ?? undefined}
                        className={`px-3 py-1.75 font-mono ${
                          result.error !== null ? "text-warn" : result.hit ? "text-ok" : "text-err"
                        }`}
                      >
                        {result.error !== null ? "ошибка" : result.hit ? "да" : "нет"}
                        {result.searches > 0 ? ` · ${String(result.searches)} поиск.` : ""}
                      </td>
                    );
                  })}
                </tr>
              ))}
            </tbody>
          </table>
        </ScrollArea>
      )}
    </section>
  );
}

export default observer(ChatEvalPanel);
