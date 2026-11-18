import { useEffect, type ReactNode } from "react";
import { observer } from "mobx-react-lite";
import { AnimatePresence, motion, useIsPresent } from "motion/react";
import { useStore } from "../../stores/useStore";
import { useFx, useTransition } from "../../ui/motion/fx";
import ConversationList from "./ConversationList";
import ChatThread from "./ChatThread";
import RunContext from "./RunContext";
import ChatExplorer from "./explorer/ChatExplorer";
import ExplorerFileView from "./explorer/ExplorerFileView";
import NotesPanel from "./notes/NotesPanel";
import NoteView from "./notes/NoteView";
import ViewerDock from "../../viewer/ViewerDock";
import ChatBrowserDock from "./ChatBrowserDock";
import { JUDGE_ROW } from "../execPolicy/execPolicyCopy";
import { VOICE_DEVICE_ROW } from "../voice/voiceCopy";

function RightPanelMotion({
  panelKey,
  browser = false,
  children,
}: {
  readonly panelKey: string;
  readonly browser?: boolean;
  readonly children: ReactNode;
}) {
  const isPresent = useIsPresent();
  const { motion: motionLevel } = useFx();
  const reveal = useTransition("reveal");
  return (
    <motion.div
      key={panelKey}
      initial={motionLevel === "none" ? false : { x: "100%", opacity: 0 }}
      animate={{ x: 0, opacity: 1 }}
      exit={motionLevel === "none" ? { x: 0, opacity: 1 } : { x: "100%", opacity: 0 }}
      transition={reveal}
      className={
        isPresent
          ? browser
            ? "col-start-3 row-start-1 flex h-full min-h-0 min-w-0"
            : "col-start-3 row-start-1 flex h-full min-h-0 w-full flex-none"
          : "absolute inset-y-0 right-0 z-10 flex h-full min-h-0 w-76 shadow-pop max-[1250px]:w-60"
      }
    >
      {children}
    </motion.div>
  );
}

export default observer(function ChatWorkspace() {
  const { chat, viewer, execPolicy, ui, voice, browserSettings } = useStore();
  useEffect(() => {
    void chat.mount();
    return chat.unmount;
  }, [chat]);
  useEffect(() => {
    void execPolicy.load();
    const refresh = (): void => void execPolicy.refresh();
    window.addEventListener("focus", refresh);
    return () => window.removeEventListener("focus", refresh);
  }, [execPolicy]);
  const activeId = chat.active?.id ?? null;
  useEffect(() => {
    void chat.terminal.show(activeId);
    void chat.backgroundShells.show(activeId);
  }, [chat, activeId]);
  return (
    <div
      className="chat-workspace-grid relative min-h-0 min-w-0 flex-1 overflow-hidden"
      data-list-open={chat.listOpen ? "true" : "false"}
      data-right-open={chat.panelOpen ? "true" : "false"}
    >
      <AnimatePresence initial={false}>
        {chat.listOpen ? <ConversationList key="conversation-list" store={chat} /> : null}
      </AnimatePresence>
      <div className="col-start-2 row-start-1 flex h-full min-h-0 min-w-0">
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
      </div>
      <AnimatePresence initial={false}>
        {chat.activePanel === "browser" && browserSettings.enabled && !viewer.isOpen ? (
          chat.panelOpen ? (
            <RightPanelMotion key="browser-panel" panelKey="browser-panel" browser>
              <ChatBrowserDock store={chat} />
            </RightPanelMotion>
          ) : null
        ) : chat.panelOpen ? (
          <RightPanelMotion key={chat.activePanel} panelKey={chat.activePanel}>
            {viewer.isOpen ? (
              <ViewerDock />
            ) : chat.isPanelShown("files") ? (
              <ExplorerFileView explorer={chat.explorer} />
            ) : chat.isPanelShown("notes") ? (
              <NoteView notes={chat.notes} />
            ) : null}
            {chat.activePanel === "files" ? (
              <ChatExplorer store={chat} />
            ) : chat.activePanel === "notes" ? (
              <NotesPanel store={chat} />
            ) : (
              <RunContext store={chat} />
            )}
          </RightPanelMotion>
        ) : null}
      </AnimatePresence>
    </div>
  );
});
