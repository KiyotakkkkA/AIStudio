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
      className={`flex w-76 shrink-0 flex-col border-l border-main-750 bg-main-850 max-[1250px]:w-60 ${store.contextOpen ? "max-[1100px]:absolute max-[1100px]:inset-y-0 max-[1100px]:right-0 max-[1100px]:z-10 max-[1100px]:w-76 max-[1100px]:shadow-pop" : "max-[1100px]:hidden"}`}
    >
      <ScrollArea className="flex min-h-0 flex-1 flex-col gap-3 px-4 py-4.5">
        <div className="flex items-center justify-between">
          <h2 className="text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase">
            Контекст запуска
          </h2>
          <span className="min-[1101px]:hidden">
            <Button onClick={store.toggleContext}>Закрыть</Button>
          </span>
        </div>
        <h2 className="text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase">
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
          <p className="rounded-control border border-main-750 bg-main-900 px-3 py-2 text-xs/snug  text-main-500">
            Начните новый чат, чтобы изменить модель, хранилища или настройки.
          </p>
        )}
      </ScrollArea>
    </aside>
  );
});
