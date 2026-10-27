import {
  mdiAlert,
  mdiAlertCircle,
  mdiCancel,
  mdiCheckCircle,
  mdiContentCopy,
  mdiDeleteOutline,
  mdiPlayCircle,
  mdiPlus,
} from "@mdi/js";
import { ContextMenu } from "@kiyotakkkka/zvs-uikit-lib";
import { Handle, Position, type NodeProps } from "@xyflow/react";
import { memo } from "react";
import Icon from "../../ui/atoms/Icon";
import { useCanvasActions } from "./canvasActions";
import { STEP_STATUS_LABELS } from "./runCopy";
import {
  IN_HANDLE,
  UP_HANDLE,
  type NextAnchor,
  type ScenarioFlowNode,
  type SlotPort,
} from "./flowModel";
import { NEXT_STEP } from "./scenarioCopy";

const PORT =
  "!size-2.25 !min-h-0 !min-w-0 !rounded-full !border-[1.5px] !border-main-800 !bg-main-600 transition-colors hover:!bg-accent-dark";

function slotPortClass(port: SlotPort): string {
  if (port.drop === "accept")
    return `${PORT} !bg-accent-dark shadow-[0_0_0_4px_var(--color-accent-soft)]`;
  if (port.drop === "refuse") return `${PORT} !bg-err shadow-[0_0_0_4px_var(--color-err-soft)]`;
  return port.filled ? `${PORT} !bg-ok` : PORT;
}

function Badge({
  path,
  tone,
  title,
  pulse = false,
}: {
  readonly path: string;
  readonly tone: string;
  readonly title: string;
  readonly pulse?: boolean;
}) {
  return (
    <span
      className={`absolute -top-2 -right-2 flex size-4.5 items-center justify-center rounded-full bg-main-800 ${tone} ${
        pulse ? "animate-pulse" : ""
      }`}
      title={title}
      aria-label={title}
    >
      <Icon path={path} size={16} />
    </span>
  );
}

const STUB = 26;

function NextButton({
  entry,
  width,
  height,
  slotted,
}: {
  readonly entry: NextAnchor;
  readonly width: number;
  readonly height: number;
  readonly slotted: boolean;
}) {
  const actions = useCanvasActions();
  const right = entry.side === "right";
  const reach = right ? STUB : STUB + (slotted ? 18 : 0);
  return (
    <>
      <span
        aria-hidden
        className="pointer-events-none absolute bg-main-500"
        style={
          right
            ? { left: width + 5, top: entry.offset, width: reach - 5, height: 1 }
            : { left: entry.offset, top: height + 5, width: 1, height: reach - 5 }
        }
      />
      <button
        type="button"
        aria-label={NEXT_STEP}
        title={NEXT_STEP}
        className="nodrag nopan absolute flex size-5 -translate-x-1/2 -translate-y-1/2 items-center justify-center rounded-[5px] border border-main-400 bg-main-800 text-main-200 hover:border-accent-dark hover:text-accent-medium"
        style={
          right
            ? { left: width + reach + 10, top: entry.offset }
            : { left: entry.offset, top: height + reach + 10 }
        }
        onClick={(event) => {
          event.stopPropagation();
          actions.next(entry.anchor);
        }}
      >
        <Icon path={mdiPlus} size={14} />
      </button>
    </>
  );
}

function ScenarioNodeView({ id, data, selected }: NodeProps<ScenarioFlowNode>) {
  const actions = useCanvasActions();
  const trigger = data.role === "trigger";
  const sub = data.role === "subNode";
  const frame = [
    "relative flex items-center rounded-[11px] border bg-main-800 text-left",
    sub ? "gap-2 px-2.5" : "gap-2.5 px-3",
    data.run === "failed"
      ? "border-err shadow-[0_0_0_3px_var(--color-err-soft),var(--shadow-card)]"
      : selected || data.run === "running"
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
            <Badge path={mdiAlertCircle} tone="text-err" title={data.issues.join("\n")} />
          ) : data.warnings.length > 0 ? (
            <Badge path={mdiAlert} tone="text-warn" title={data.warnings.join("\n")} />
          ) : data.run === "failed" ? (
            <Badge
              path={mdiAlertCircle}
              tone="text-err"
              title={data.runError ?? STEP_STATUS_LABELS.failed}
            />
          ) : data.ceiling !== null ? (
            <Badge path={mdiCancel} tone="text-warn" title={data.ceiling} />
          ) : data.run === "running" ? (
            <Badge
              path={mdiPlayCircle}
              tone="text-accent-medium"
              title={STEP_STATUS_LABELS.running}
              pulse
            />
          ) : data.run === "succeeded" ? (
            <Badge path={mdiCheckCircle} tone="text-ok" title={STEP_STATUS_LABELS.succeeded} />
          ) : null}

          {data.hasIn ? (
            <Handle id={IN_HANDLE} type="target" position={Position.Left} className={PORT} />
          ) : null}
          {data.outs.map((out) => (
            <Handle
              key={out.handle}
              id={out.handle}
              type="source"
              position={Position.Right}
              className={PORT}
              style={{ top: out.top }}
            />
          ))}
          {data.outs.map((out) =>
            out.label === null ? null : (
              <span
                key={`${out.handle}-label`}
                className="pointer-events-none absolute left-full ml-2 -translate-y-full pb-0.5 text-[9.5px] whitespace-nowrap text-main-400"
                style={{ top: out.top }}
              >
                {out.label}
              </span>
            ),
          )}
          {selected
            ? data.next.map((entry) => (
                <NextButton
                  key={entry.key}
                  entry={entry}
                  width={data.width}
                  height={data.height}
                  slotted={entry.side === "bottom"}
                />
              ))
            : null}
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
