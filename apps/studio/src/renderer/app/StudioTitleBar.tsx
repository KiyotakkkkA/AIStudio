import { Dropdown } from "@kiyotakkkka/zvs-uikit-lib";
import {
  mdiArrowLeft,
  mdiArrowRight,
  mdiDockLeft,
  mdiHistory,
  mdiInformationOutline,
} from "@mdi/js";
import { useEffect, useState, type MouseEvent } from "react";
import { useLocation, useNavigate, useNavigationType } from "react-router-dom";
import IconButton from "../ui/atoms/buttons/IconButton";
import { ActionMenu, ActionMenuItem, ActionMenuSeparator } from "../ui/molecules/ActionMenu";
import {
  canGoBack,
  canGoForward,
  historyIndex,
  startPosition,
  stepPosition,
} from "./historyPosition";
import {
  TITLE_BAR_ABOUT,
  TITLE_BAR_BACK,
  TITLE_BAR_COLLAPSE_RAIL,
  TITLE_BAR_EXPAND_RAIL,
  TITLE_BAR_FORWARD,
  TITLE_BAR_HELP,
  TITLE_BAR_LOGS,
} from "./titleBarCopy";
import WindowControls, { useWindowFrame } from "./WindowControls";

interface StudioTitleBarProps {
  readonly railCollapsed: boolean;
  readonly onToggleRail: () => void;
  readonly onAbout: () => void;
  readonly onLogs: () => void;
}

const BAR_ICON_TONE = "text-main-400 hover:bg-main-800 hover:text-main-50";
const INTERACTIVE = "button, a, input, [role='menu'], [role='menuitem'], [role='dialog']";

function useHistoryPosition() {
  const location = useLocation();
  const step = useNavigationType();
  const [position, setPosition] = useState(() => startPosition(historyIndex(window.history.state)));
  useEffect(() => {
    setPosition((current) => stepPosition(current, step, historyIndex(window.history.state)));
  }, [location.key, step]);
  return position;
}

export default function StudioTitleBar({
  railCollapsed,
  onToggleRail,
  onAbout,
  onLogs,
}: StudioTitleBarProps) {
  const navigate = useNavigate();
  const position = useHistoryPosition();
  const { maximized, send } = useWindowFrame();

  const startDrag = (event: MouseEvent<HTMLElement>) => {
    if (event.button !== 0) return;
    if (event.target instanceof Element && event.target.closest(INTERACTIVE)) return;
    send(event.detail === 2 ? "window.toggleMaximize" : "window.drag");
  };

  return (
    <header
      className="flex h-10 flex-none items-center bg-main-950 select-none"
      onMouseDown={startDrag}
    >
      <div className="flex flex-none items-center gap-0.5 pl-2">
        <IconButton
          label={TITLE_BAR_BACK}
          tooltip="bottom-left"
          path={mdiArrowLeft}
          iconSize={16}
          toneClassName={BAR_ICON_TONE}
          disabled={!canGoBack(position)}
          onClick={() => void navigate(-1)}
        />
        <IconButton
          label={TITLE_BAR_FORWARD}
          tooltip="bottom-left"
          path={mdiArrowRight}
          iconSize={16}
          toneClassName={BAR_ICON_TONE}
          disabled={!canGoForward(position)}
          onClick={() => void navigate(1)}
        />
        <IconButton
          label={railCollapsed ? TITLE_BAR_EXPAND_RAIL : TITLE_BAR_COLLAPSE_RAIL}
          tooltip="bottom-left"
          path={mdiDockLeft}
          iconSize={16}
          toneClassName={BAR_ICON_TONE}
          onClick={onToggleRail}
        />
        <Dropdown menuWidth={224} menuPlacement="bottom-left">
          <Dropdown.Trigger
            rounded="rounded-lg"
            className="ml-1 h-7 gap-1.5 whitespace-nowrap border-none bg-transparent px-2.5 py-0 text-[12px] font-medium text-main-400 hover:bg-main-800 hover:text-main-50 aria-expanded:bg-main-800 aria-expanded:text-main-50"
          >
            {TITLE_BAR_HELP}
          </Dropdown.Trigger>
          <ActionMenu label={TITLE_BAR_HELP}>
            <ActionMenuItem icon={mdiHistory} onClick={onLogs}>
              {TITLE_BAR_LOGS}
            </ActionMenuItem>
            <ActionMenuSeparator />
            <ActionMenuItem icon={mdiInformationOutline} onClick={onAbout}>
              {TITLE_BAR_ABOUT}
            </ActionMenuItem>
          </ActionMenu>
        </Dropdown>
      </div>
      <div className="h-full min-w-0 flex-1" />
      <WindowControls maximized={maximized} send={send} />
    </header>
  );
}
