import { mdiAlertCircle, mdiCancel, mdiContentCopy, mdiDeleteOutline } from "@mdi/js";
import { ContextMenu } from "@kiyotakkkka/zvs-uikit-lib";
import { Handle, Position, type NodeProps } from "@xyflow/react";
import { memo } from "react";
import Icon from "../../ui/atoms/Icon";
import { useCanvasActions } from "./canvasActions";
import {
  IN_HANDLE,
  OUT_HANDLE,
  UP_HANDLE,
  type ScenarioFlowNode,
  type SlotPort,
} from "./flowModel";

const PORT =
  "!size-2.25 !min-h-0 !min-w-0 !rounded-full !border-[1.5px] !border-main-800 !bg-main-600 transition-colors hover:!bg-accent-dark";

function slotPortClass(port: SlotPort): string {
  if (port.drop === "accept")
    return `${PORT} !bg-accent-dark shadow-[0_0_0_4px_var(--color-accent-soft)]`;
  if (port.drop === "refuse") return `${PORT} !bg-err shadow-[0_0_0_4px_var(--color-err-soft)]`;
  return port.filled ? `${PORT} !bg-ok` : PORT;
}

function ScenarioNodeView({ id, data, selected }: NodeProps<ScenarioFlowNode>) {
  const actions = useCanvasActions();
  const trigger = data.role === "trigger";
  const sub = data.role === "subNode";
  const frame = [
    "relative flex items-center rounded-[11px] border bg-main-800 text-left",
    sub ? "gap-2 px-2.5" : "gap-2.5 px-3",
    selected
      ? "border-accent-dark shadow-[0_0_0_3px_var(--color-accent-soft),var(--shadow-pop)]"
      : "border-main-600 shadow-card",
    trigger ? "border-l-[3px] border-l-accent-dark" : "",
    data.disabled ? "border-dashed opacity-60" : "",
  ].join(" ");
  const problem = data.issues.length > 0;
  const tile = sub
    ? "size-6.5 rounded-[7px]"
    : data.slots.length > 0
      ? "size-8.5 rounded-[9px]"
      : "size-7.5 rounded-lg";

  return (
    <ContextMenu>
      <ContextMenu.Trigger>
        <div className={frame} style={{ width: data.width, height: data.height }}>
          <span
            className={`flex flex-none items-center justify-center bg-main-700 ${tile} ${
              data.role === "action" && data.slots.length === 0
                ? "text-main-200"
                : "text-accent-medium"
            }`}
          >
            <Icon path={data.icon} size={sub ? 14 : data.slots.length > 0 ? 18 : 16} />
          </span>
          <span className="flex min-w-0 flex-1 flex-col">
            <span
              className={`truncate leading-tight font-semibold text-main-50 ${
                sub ? "text-[11.5px]" : data.slots.length > 0 ? "text-[13px]" : "text-[12.5px]"
              }`}
            >
              {data.label}
            </span>
            <span className={`truncate text-main-400 ${sub ? "text-[10px]" : "text-[10.5px]"}`}>
              {data.subLabel}
            </span>
            {data.chips.length > 0 ? (
              <span className="mt-1.25 flex gap-1">
                {data.chips.map((chip) => (
                  <span
                    key={chip}
                    className="inline-flex h-4.25 items-center rounded-[5px] bg-main-700 px-1.5 text-[9.5px] font-medium text-main-300"
                  >
                    {chip}
                  </span>
                ))}
              </span>
            ) : null}
          </span>

          {problem ? (
            <span
              className="absolute -top-2 -right-2 flex size-4.5 items-center justify-center rounded-full bg-main-800 text-err"
              title={data.issues.join("\n")}
              aria-label={data.issues.join(". ")}
            >
              <Icon path={mdiAlertCircle} size={16} />
            </span>
          ) : data.ceiling !== null ? (
            <span
              className="absolute -top-2 -right-2 flex size-4.5 items-center justify-center rounded-full bg-main-800 text-warn"
              title={data.ceiling}
              aria-label={data.ceiling}
            >
              <Icon path={mdiCancel} size={16} />
            </span>
          ) : null}

          {data.hasIn ? (
            <Handle id={IN_HANDLE} type="target" position={Position.Left} className={PORT} />
          ) : null}
          {data.hasOut ? (
            <Handle id={OUT_HANDLE} type="source" position={Position.Right} className={PORT} />
          ) : null}
          {data.hasUp ? (
            <Handle id={UP_HANDLE} type="target" position={Position.Top} className={PORT} />
          ) : null}
          {data.slots.map((port) => (
            <Handle
              key={port.handle}
              id={port.handle}
              type="source"
              position={Position.Bottom}
              className={slotPortClass(port)}
              style={{ left: port.offset }}
            />
          ))}
          {data.slots.map((port) => (
            <span
              key={`${port.handle}-label`}
              className={`pointer-events-none absolute top-full mt-1.5 -translate-x-1/2 text-[9.5px] whitespace-nowrap ${
                port.drop === "refuse"
                  ? "text-err"
                  : port.drop === "accept"
                    ? "text-accent-medium"
                    : "text-main-500"
              }`}
              style={{ left: port.offset }}
            >
              {port.label}
            </span>
          ))}
        </div>
      </ContextMenu.Trigger>
      <ContextMenu.Content className="min-w-44 rounded-card border-line bg-main-900 shadow-pop">
        <ContextMenu.Item
          leftSlot={<Icon path={mdiContentCopy} size={15} />}
          className="text-[13px]"
          onClick={() => {
            actions.duplicate(id);
          }}
        >
          Дублировать
        </ContextMenu.Item>
        <ContextMenu.ItemDanger
          leftSlot={<Icon path={mdiDeleteOutline} size={15} />}
          className="text-[13px]"
          onClick={() => {
            actions.remove(id);
          }}
        >
          Удалить
        </ContextMenu.ItemDanger>
      </ContextMenu.Content>
    </ContextMenu>
  );
}

export default memo(ScenarioNodeView);
