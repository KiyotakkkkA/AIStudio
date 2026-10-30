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
import { JUDGE_ROW } from "../execPolicy/execPolicyCopy";
import { VOICE_DEVICE_ROW } from "../voice/voiceCopy";

export default observer(function ChatWorkspace() {
  const { chat, viewer, execPolicy, ui, voice } = useStore();
  useEffect(() => {
    void chat.mount();
    return chat.unmount;
  }, [chat]);
  useEffect(() => {
    void execPolicy.load();
  }, [execPolicy]);
  const activeId = chat.active?.id ?? null;
  useEffect(() => {
    void chat.terminal.show(activeId);
  }, [chat, activeId]);
  return (
    <div className="relative flex min-h-0 min-w-0 flex-1 overflow-hidden">
      <ConversationList store={chat} />
      <ChatThread
        store={chat}
        policy={{
          store: execPolicy,
          setup: () => ui.revealPreference("general", JUDGE_ROW.key),
        }}
        voice={{
          store: voice,
          setup: () => ui.revealPreference("voice", VOICE_DEVICE_ROW.key),
        }}
      />
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
