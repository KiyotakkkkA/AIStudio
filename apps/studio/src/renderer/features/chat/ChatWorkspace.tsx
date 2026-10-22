import { useEffect } from "react";
import { observer } from "mobx-react-lite";
import { useStore } from "../../stores/useStore";
import ConversationList from "./ConversationList";
import ChatThread from "./ChatThread";
import RunContext from "./RunContext";
import ChatExplorer from "./explorer/ChatExplorer";
import ExplorerFileView from "./explorer/ExplorerFileView";

export default observer(function ChatWorkspace() {
  const { chat } = useStore();
  useEffect(() => {
    void chat.mount();
    return chat.dispose;
  }, [chat]);
  return (
    <div className="relative flex min-h-0 min-w-0 flex-1 overflow-hidden">
      <ConversationList store={chat} />
      <ChatThread store={chat} />
      {chat.activePanel === "files" ? <ExplorerFileView explorer={chat.explorer} /> : null}
      {chat.activePanel === "files" ? <ChatExplorer store={chat} /> : <RunContext store={chat} />}
    </div>
  );
});
