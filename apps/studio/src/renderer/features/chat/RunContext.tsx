import { observer } from "mobx-react-lite";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import { mdiClose } from "@mdi/js";
import IconButton from "../../ui/atoms/buttons/IconButton";
import TextArea from "../../ui/atoms/TextArea";
import type ChatStore from "./ChatStore";

export default observer(function RunContext({ store }: { readonly store: ChatStore }) {
  const locked = !store.canConfigure;
  return (
    <aside
      aria-label="Контекст запуска"
      className={`flex w-76 shrink-0 flex-col bg-main-800 max-[1250px]:w-60 ${store.contextOpen ? "max-[1100px]:absolute max-[1100px]:inset-y-0 max-[1100px]:right-0 max-[1100px]:z-10 max-[1100px]:w-76 max-[1100px]:shadow-pop" : "max-[1100px]:hidden"}`}
    >
      <div className="flex h-14 shrink-0 items-center gap-2 pr-2.5 pl-4">
        <h2 className="min-w-0 flex-1 truncate text-[14px] font-medium text-main-100">
          Контекст запуска
        </h2>
        <span className="min-[1101px]:hidden">
          <IconButton
            aria-label="Закрыть контекст запуска"
            title="Закрыть"
            path={mdiClose}
            iconSize={17}
            onClick={store.toggleContext}
          />
        </span>
      </div>
      <ScrollArea className="flex min-h-0 flex-1 flex-col gap-3 px-4 pt-1 pb-4.5">
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
          <p className="rounded-control bg-main-900 px-3 py-2 text-xs/snug  text-main-500">
            Начните новый чат, чтобы изменить модель, хранилища или настройки.
          </p>
        )}
      </ScrollArea>
    </aside>
  );
});
