import { mdiPlay, mdiRestore } from "@mdi/js";
import { observer } from "mobx-react-lite";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import { useNavigate } from "react-router-dom";
import { ROUTES } from "../../app/routes";
import Button from "../../ui/atoms/buttons/Button";
import Icon from "../../ui/atoms/Icon";
import ConfirmDialog from "../../ui/molecules/ConfirmDialog";
import type ToolStore from "./ToolStore";
import { resultLine, resultPreview } from "./toolPresentation";

function TryItPanel({ store }: { readonly store: ToolStore }) {
  const navigate = useNavigate();
  const tool = store.selected;
  const parsed = store.parsedInput;
  const result = store.result;
  const blocked = store.blockedReason;

  if (tool === null) return null;

  return (
    <div className="flex flex-none flex-col gap-2.5 rounded-card border border-line bg-main-900 p-3">
      <textarea
        aria-label="Входные данные (JSON)"
        spellCheck={false}
        value={store.inputText}
        onChange={(event) => {
          store.setInputText(event.target.value);
        }}
        rows={Math.min(10, Math.max(3, store.inputText.split("\n").length))}
        className={`w-full resize-y rounded-control border bg-main-850 px-2.5 py-2.25 font-mono text-[11px] leading-[1.7] text-main-200 outline-none ${
          parsed.ok ? "border-line focus:border-accent-dark" : "border-err focus:border-err"
        }`}
      />
      {parsed.ok ? null : <p className="m-0 text-[11px] text-err">{parsed.error}</p>}

      <div className="flex items-center gap-2.5">
        <Button
          type="button"
          tone="primary"
          disabled={!store.canRun}
          title={blocked ?? undefined}
          onClick={store.requestRun}
        >
          <Icon path={mdiPlay} size={15} />
          {store.running ? "Выполняется…" : "Запустить"}
        </Button>
        {store.inputDirty ? (
          <button
            type="button"
            title="Вернуть значения из схемы"
            aria-label="Вернуть значения из схемы"
            className="flex size-7 items-center justify-center rounded-control text-main-400 hover:bg-main-800 hover:text-main-100"
            onClick={store.resetInput}
          >
            <Icon path={mdiRestore} size={15} />
          </button>
        ) : null}
        <span className="min-w-0 flex-1 truncate text-right font-mono text-[11px]">
          {blocked !== null ? (
            <span className="text-main-500">{blocked}</span>
          ) : result === null ? (
            store.needsConfirmation ? (
              <span className="text-warn">потребуется подтверждение</span>
            ) : null
          ) : (
            <span className={result.ok ? "text-ok" : "text-err"}>{resultLine(result)}</span>
          )}
        </span>
      </div>

      {store.runError === null ? null : (
        <p
          role="alert"
          className="m-0 rounded-control bg-err-soft px-2.5 py-2 text-[11.5px] text-err"
        >
          {store.runError}
        </p>
      )}

      {result === null ? null : (
        <div className="flex flex-col gap-1.5">
          {result.ok || result.error === null ? null : (
            <p className="m-0 text-[11.5px] wrap-break-word text-err">{result.error}</p>
          )}
          <ScrollArea className="max-h-56 rounded-control bg-main-700 px-2.5 py-2">
            <pre className="m-0 font-mono text-[11px] leading-relaxed whitespace-pre-wrap wrap-break-word text-main-300">
              {resultPreview(result)}
            </pre>
          </ScrollArea>
          <button
            type="button"
            className="self-start text-[11.5px] font-medium text-accent-medium hover:underline"
            onClick={() => void navigate(`${ROUTES.runs}?run=${result.runId}`)}
          >
            Открыть запуск
          </button>
        </div>
      )}

      <ConfirmDialog
        open={store.confirming}
        title={tool.destructive ? "Запустить разрушающий инструмент?" : "Подтвердите вызов"}
        confirmLabel="Запустить"
        confirmTone={tool.destructive ? "danger" : "primary"}
        onConfirm={store.confirmRun}
        onCancel={store.cancelRun}
      >
        {tool.destructive ? (
          <>
            <span className="font-mono text-main-100">{tool.key}</span> помечен как разрушающий:
            вызов может удалить или перезаписать данные. Он выполнится с введёнными параметрами
            прямо сейчас — так же, как в запуске после одобрения.
          </>
        ) : (
          <>
            Для <span className="font-mono text-main-100">{tool.key}</span> задано разрешение «ask»:
            каждый вызов требует подтверждения — так же, как в запуске.
          </>
        )}
      </ConfirmDialog>
    </div>
  );
}

export default observer(TryItPanel);
