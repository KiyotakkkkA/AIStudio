import { observer } from "mobx-react-lite";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import TextArea from "../../ui/atoms/TextArea";
import type ChatStore from "./ChatStore";
import SidePanelFrame from "./SidePanelFrame";

export default observer(function RunContext({ store }: { readonly store: ChatStore }) {
  const locked = !store.canConfigure;
  return (
    <SidePanelFrame store={store} title="Контекст запуска">
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
    </SidePanelFrame>
  );
});
