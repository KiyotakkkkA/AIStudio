import { useEffect } from "react";
import { observer } from "mobx-react-lite";
import { mdiChatOutline } from "@mdi/js";
import { useStore } from "../../stores/useStore";
import Icon from "../../ui/atoms/Icon";
import ConversationList from "./ConversationList";
import ChatThread from "./ChatThread";
import RunContext from "./RunContext";

export default observer(function ChatWorkspace() {
  const { chat } = useStore();
  useEffect(() => {
    void chat.mount();
    return chat.dispose;
  }, [chat]);
  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col">
      <header className="flex h-[56px] flex-none items-center gap-3 border-b border-main-750 bg-main-900 px-5">
        <Icon path={mdiChatOutline} size={20} className="flex-none text-accent-dark" />
        <h1 className="m-0 flex-none text-xl/7 font-semibold tracking-[-0.01em] text-main-50">
          Чат
        </h1>
        <span className="ml-1.5 truncate text-xs/4 text-main-400">
          Окно для общения с ассистентом
        </span>
      </header>
      <div className="relative flex min-h-0 min-w-0 flex-1 overflow-hidden">
        <ConversationList store={chat} />
        <ChatThread store={chat} />
        <RunContext store={chat} />
      </div>
    </div>
  );
});
