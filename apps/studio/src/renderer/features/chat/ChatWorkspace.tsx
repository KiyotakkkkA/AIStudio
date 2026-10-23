import { useEffect } from "react";
import { observer } from "mobx-react-lite";
import { useStore } from "../../stores/useStore";
import ConversationList from "./ConversationList";
import ChatThread from "./ChatThread";
import RunContext from "./RunContext";
import ChatExplorer from "./explorer/ChatExplorer";
import ExplorerFileView from "./explorer/ExplorerFileView";
import ViewerDock from "../../viewer/ViewerDock";

export default observer(function ChatWorkspace() {
  const { chat, viewer } = useStore();
  useEffect(() => {
    void chat.mount();
    return chat.dispose;
  }, [chat]);
  return (
    <div className="relative flex min-h-0 min-w-0 flex-1 overflow-hidden">
      <ConversationList store={chat} />
      <ChatThread store={chat} />
      {viewer.isOpen ? (
        <ViewerDock />
      ) : chat.activePanel === "files" ? (
        <ExplorerFileView explorer={chat.explorer} />
      ) : null}
      {chat.activePanel === "files" ? <ChatExplorer store={chat} /> : <RunContext store={chat} />}
    </div>
  );
});
