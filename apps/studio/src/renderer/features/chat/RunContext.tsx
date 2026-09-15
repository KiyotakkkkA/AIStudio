import { observer } from "mobx-react-lite";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import Button from "../../ui/atoms/buttons/Button";
import TextArea from "../../ui/atoms/TextArea";
import type ChatStore from "./ChatStore";

export default observer(function RunContext({ store }: { readonly store: ChatStore }) {
  const locked = !store.canConfigure;
  return (
    <aside
      aria-label="Контекст запуска"
      className={`flex w-73 shrink-0 flex-col border-l border-main-750 bg-main-900 max-[1250px]:w-57.5 ${store.contextOpen ? "max-[1100px]:absolute max-[1100px]:inset-y-0 max-[1100px]:right-0 max-[1100px]:z-10 max-[1100px]:w-73" : "max-[1100px]:hidden"}`}
    >
      <ScrollArea className="flex min-h-0 flex-1 flex-col gap-4 p-4">
        <div className="flex items-center justify-between">
          <h2 className="text-[11px] font-semibold tracking-wider text-main-400 uppercase">
            Контекст запуска
          </h2>
          <span className="min-[1101px]:hidden">
            <Button onClick={store.toggleContext}>Закрыть</Button>
          </span>
        </div>
        <h2 className="text-[11px] font-semibold tracking-wider text-main-400 uppercase">
          Системная инструкция
        </h2>
        <TextArea
          aria-label="Системная инструкция"
          placeholder="Дополнительные инструкции для этого диалога"
          value={store.active?.systemPrompt ?? store.composer.systemPrompt}
          maxLength={100000}
          disabled={locked}
          onChange={(event) => store.composer.setSystemPrompt(event.target.value)}
        />
        {locked && (
          <p className="text-[11px] text-main-500">
            Начните новый чат, чтобы изменить модель, хранилища или настройки.
          </p>
        )}
        <div className="mt-auto space-y-2 rounded-card border border-main-750 bg-main-800 p-3">
          <div className="flex justify-between text-xs text-main-400">
            <span>Использование за сеанс</span>
            <span>{store.sessionTokens.toLocaleString()} ток.</span>
          </div>
          <meter
            aria-label="Токены сеанса относительно окна контекста"
            min={0}
            max={store.model?.contextWindow || Math.max(1, store.sessionTokens)}
            value={store.sessionTokens}
            className="h-2 w-full accent-accent-dark"
          />
          <p className="text-[10.5px] text-main-500">
            Ответов:{" "}
            {store.active?.messages.filter((message) => message.role === "assistant").length ?? 0} ·
            примерный расход
          </p>
        </div>
      </ScrollArea>
    </aside>
  );
});
