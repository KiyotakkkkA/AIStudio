import { useEffect } from "react";
import { observer } from "mobx-react-lite";
import { useStore } from "../../stores/useStore";
import ConversationList from "./ConversationList";
import ChatThread from "./ChatThread";
import RunContext from "./RunContext";
import ChatExplorer from "./explorer/ChatExplorer";
import ExplorerFileView from "./explorer/ExplorerFileView";
import ViewerDock from "../../viewer/ViewerDock";
import ChatBrowserDock from "./ChatBrowserDock";

export default observer(function ChatWorkspace() {
  const { chat, viewer } = useStore();
  useEffect(() => {
    void chat.mount();
    return chat.unmount;
  }, [chat]);
  const activeId = chat.active?.id ?? null;
  useEffect(() => {
    void chat.terminal.show(activeId);
  }, [chat, activeId]);
  return (
    <div className="relative flex min-h-0 min-w-0 flex-1 overflow-hidden">
      <ConversationList store={chat} />
      <ChatThread store={chat} />
      {chat.activePanel === "browser" && !viewer.isOpen ? (
        chat.panelOpen ? (
          <ChatBrowserDock store={chat} />
        ) : null
      ) : (
        <>
          {viewer.isOpen ? (
            <ViewerDock />
          ) : chat.isPanelShown("files") ? (
            <ExplorerFileView explorer={chat.explorer} />
          ) : null}
          {chat.activePanel === "files" ? (
            <ChatExplorer store={chat} />
          ) : (
            <RunContext store={chat} />
          )}
        </>
      )}
    </div>
  );
});
