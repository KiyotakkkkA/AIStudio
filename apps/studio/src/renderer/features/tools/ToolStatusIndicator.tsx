import { mdiAlertCircleOutline, mdiAlertOutline } from "@mdi/js";
import { Floating } from "@kiyotakkkka/zvs-uikit-lib";
import { useNavigate } from "react-router-dom";
import type { McpServerDto, ToolDto } from "@zvs/shared";
import { ROUTES } from "../../app/routes";
import Icon from "../../ui/atoms/Icon";
import { unavailability } from "./toolPresentation";

export default function ToolStatusIndicator({
  tool,
  server,
}: {
  readonly tool: ToolDto;
  readonly server: McpServerDto | null;
}) {
  const navigate = useNavigate();
  const problem = unavailability(tool, server);
  if (problem === null) return null;
  return (
    <Floating
      anchor="bottom-right"
      className="flex-none"
      onClick={(event) => event.stopPropagation()}
      onKeyDown={(event) => event.stopPropagation()}
    >
      <Floating.Trigger>
        <span
          tabIndex={0}
          aria-label={problem.title}
          className={`flex size-6 items-center justify-center rounded-control outline-none hover:bg-main-700 focus-visible:outline-2 focus-visible:outline-accent-dark ${problem.tone === "err" ? "text-err" : "text-warn"}`}
        >
          <Icon path={problem.tone === "err" ? mdiAlertCircleOutline : mdiAlertOutline} size={16} />
        </span>
      </Floating.Trigger>
      <Floating.Content className="w-72 max-w-[calc(100vw-2rem)] rounded-card border border-line bg-main-900 p-3.5 text-xs/relaxed text-main-300 shadow-pop">
        <p className="m-0 font-medium text-main-100">{problem.title}</p>
        {problem.detail === null ? null : (
          <p className="m-0 mt-1 wrap-break-word text-main-400">{problem.detail}</p>
        )}
        {tool.source === "mcp" ? (
          <button
            type="button"
            className="mt-2 text-[11.5px] font-medium text-accent-medium hover:underline"
            onClick={(event) => {
              event.stopPropagation();
              void navigate(ROUTES.connections);
            }}
          >
            Открыть «Коннекторы»
          </button>
        ) : null}
      </Floating.Content>
    </Floating>
  );
}
