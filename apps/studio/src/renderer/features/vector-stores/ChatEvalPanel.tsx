import { useEffect } from "react";
import { observer } from "mobx-react-lite";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import Button from "../../ui/atoms/buttons/Button";
import SelectInput from "../../ui/atoms/SelectInput";
import Field from "../../ui/molecules/Field";
import type ChatEvalStore from "./ChatEvalStore";
import { CHAT_EVAL_MODE_LABELS } from "./ChatEvalStore";

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
    <div className="flex flex-col gap-2.5 border-t border-line pt-3.25">
      <div>
        <p className="text-[12px] font-medium text-main-200">Сквозная проверка в чате</p>
        <p className="mt-0.5 text-[11px] leading-relaxed text-main-500">
          Каждый вопрос задаётся модели во временном разговоре с этим хранилищем — сначала с поиском
          перед ответом, затем в режиме, где модель ищет сама. Засчитывается, если ответ сослался на
          ожидаемый документ. Разговоры удаляются после проверки.
        </p>
      </div>
      <div className="flex flex-wrap items-end gap-2.5">
        <div className="min-w-60 flex-1">
          <Field label="Модель">
            <SelectInput
              label="Модель для проверки"
              value={chat.modelKey}
              options={chat.models.map((model) => ({ value: model.key, label: model.label }))}
              placeholder={chat.loaded ? "Нет доступных текстовых моделей" : "Загрузка моделей…"}
              disabled={chat.running || chat.models.length === 0}
              onChange={chat.setModel}
            />
          </Field>
        </div>
        <Button
          tone="secondary"
          disabled={!chat.canRun(cases) || locked}
          onClick={() => {
            void chat.evaluate();
          }}
        >
          {chat.running ? "Спрашиваем модель…" : "Проверить в чате"}
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
    </div>
  );
}

export default observer(ChatEvalPanel);
