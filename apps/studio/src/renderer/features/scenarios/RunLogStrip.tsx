import { mdiChevronDown, mdiChevronUp, mdiPinOutline, mdiReplay } from "@mdi/js";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import { observer } from "mobx-react-lite";
import type { PointerEvent } from "react";
import type { Json } from "@zvs/shared";
import Button from "../../ui/atoms/buttons/Button";
import IconButton from "../../ui/atoms/buttons/IconButton";
import Icon from "../../ui/atoms/Icon";
import StatusDot from "../../ui/atoms/StatusDot";
import type { StatusTone } from "../../ui/atoms/statusTone";
import JsonView from "./JsonView";
import type ScenarioEditor from "./ScenarioEditor";
import { STRIP_HEIGHT_COLLAPSED } from "./ScenarioRunState";
import {
  attemptLabel,
  clockLabel,
  durationLabel,
  LAST_RUN,
  NO_RUNS,
  NO_STEP_DATA,
  NO_STEP_LOGS,
  NO_STEP_SELECTED,
  RUN_PHASE_LABELS,
  RUN_TAB_LABELS,
  STEP_STATUS_LABELS,
  TOOL_CALL_LABEL,
} from "./runCopy";
import {
  isNested,
  runDuration,
  stepLogs,
  type RunPhase,
  type RunStep,
  type RunTab,
  type StepStatus,
} from "./runModel";

const SECTION = "m-0 text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase";
const TABS: readonly RunTab[] = ["input", "output", "logs"];

const STEP_TONES: Record<StepStatus, StatusTone> = {
  running: "accent",
  succeeded: "ok",
  failed: "err",
  cancelled: "idle",
  abandoned: "warn",
};

const PHASE_TONES: Record<RunPhase, StatusTone> = {
  running: "accent",
  succeeded: "ok",
  failed: "err",
  cancelled: "idle",
};

const PHASE_TEXT: Record<RunPhase, string> = {
  running: "text-accent-medium",
  succeeded: "text-ok",
  failed: "text-err",
  cancelled: "text-main-400",
};

function stepName(editor: ScenarioEditor, step: RunStep): string {
  if (!isNested(step.nodeId)) return editor.nameOf(step.nodeId);
  return `${TOOL_CALL_LABEL} · ${editor.labelOf(step.type)}`;
}

const StepTree = observer(function StepTree({ editor }: { readonly editor: ScenarioEditor }) {
  const run = editor.execution;
  const rows = run.rows;
  if (rows.length === 0) {
    return (
      <p className="m-0 px-3.5 py-2.5 text-[11.5px] text-main-500">
        {run.view === null ? NO_RUNS : NO_STEP_DATA}
      </p>
    );
  }
  return (
    <ul className="m-0 flex list-none flex-col gap-0.5 p-0 px-2 py-1.5">
      {rows.map(({ step, depth, duration }) => {
        const selected = run.selectedStepId === step.id;
        return (
          <li key={step.id}>
            <button
              type="button"
              aria-current={selected}
              className={`flex w-full items-center gap-2 rounded-[6px] py-1 pr-1.5 text-left text-[11.5px] ${
                selected ? "bg-main-800 text-main-50" : "text-main-200 hover:bg-main-850"
              } ${step.status === "failed" ? "text-err" : ""}`}
              style={{ paddingLeft: 6 + depth * 14 }}
              onClick={() => {
                run.select(step.id);
              }}
            >
              <StatusDot tone={STEP_TONES[step.status]} title={STEP_STATUS_LABELS[step.status]} />
              <span className="min-w-0 flex-1 truncate">{stepName(editor, step)}</span>
              {step.attempt > 1 ? (
                <span className="font-mono text-[10px] text-main-500">
                  {attemptLabel(step.attempt)}
                </span>
              ) : null}
              <span className="font-mono text-[10.5px] text-main-500">
                {durationLabel(duration)}
              </span>
            </button>
          </li>
        );
      })}
    </ul>
  );
});

const StepDetail = observer(function StepDetail({ editor }: { readonly editor: ScenarioEditor }) {
  const run = editor.execution;
  const step = run.selected;
  if (step === null) {
    return <p className="m-0 px-3.5 py-3 text-[11.5px] text-main-500">{NO_STEP_SELECTED}</p>;
  }
  if (run.tab === "logs") {
    const logs = stepLogs(run.view, step.nodeId);
    const failure = run.stepError(step);
    return (
      <div className="flex flex-col gap-1.5 px-3.5 py-2.5 font-mono text-[11.5px] leading-relaxed">
        {failure === null ? null : <p className="m-0 font-sans text-err">{failure}</p>}
        {logs.length === 0 && failure === null ? (
          <p className="m-0 font-sans text-main-500">{NO_STEP_LOGS}</p>
        ) : (
          logs.map((log, index) => (
            <p
              key={`${String(index)}-${log.message}`}
              className={`m-0 ${log.level === "error" ? "text-err" : log.level === "warn" ? "text-warn" : "text-main-300"}`}
            >
              {log.message}
            </p>
          ))
        )}
      </div>
    );
  }
  const value: Json | null = run.tab === "input" ? step.input : step.output;
  return (
    <div className="px-3.5 py-2.5">
      {value === null ? (
        <p className="m-0 text-[11.5px] text-main-500">{NO_STEP_DATA}</p>
      ) : (
        <JsonView value={value} />
      )}
    </div>
  );
});

const StepActions = observer(function StepActions({ editor }: { readonly editor: ScenarioEditor }) {
  const run = editor.execution;
  const step = run.selected;
  if (step === null || isNested(step.nodeId)) return null;
  const node = editor.graph.nodes.find((entry) => entry.id === step.nodeId);
  const idempotent = editor.catalogue.get(step.type)?.idempotent === true;
  const retry =
    step.status === "failed" && run.view?.phase === "failed" && idempotent && !run.running;
  const pin = node !== undefined && run.tab === "input";
  return (
    <>
      {pin ? (
        <IconButton
          path={mdiPinOutline}
          label="Закрепить как входные данные шага"
          tooltip="top-right"
          iconSize={15}
          className="size-7"
          onClick={() => {
            editor.pinFromRun(step.nodeId);
          }}
        />
      ) : null}
      {retry ? (
        <Button type="button" tone="secondary" onClick={() => void editor.retryStep(step.nodeId)}>
          <Icon path={mdiReplay} size={14} />
          Повторить с этого шага
        </Button>
      ) : null}
    </>
  );
});

const CANVAS_MIN = 220;
const RESIZE_LABEL = "Изменить высоту журнала";

function RunLogStrip({ editor }: { readonly editor: ScenarioEditor }) {
  const run = editor.execution;
  const view = run.view;
  const step = run.selected;
  const now = Date.now();

  const startResize = (event: PointerEvent<HTMLDivElement>) => {
    const handle = event.currentTarget;
    const strip = handle.parentElement;
    const area = strip?.parentElement;
    if (strip === null || area === null || area === undefined) return;
    event.preventDefault();
    handle.setPointerCapture(event.pointerId);
    const bottom = strip.getBoundingClientRect().bottom;
    const limit = area.getBoundingClientRect().height - CANVAS_MIN;
    const move = (next: globalThis.PointerEvent) => {
      run.setStripHeight(bottom - next.clientY, limit);
    };
    const stop = () => {
      handle.removeEventListener("pointermove", move);
      handle.removeEventListener("pointerup", stop);
      handle.removeEventListener("pointercancel", stop);
    };
    handle.addEventListener("pointermove", move);
    handle.addEventListener("pointerup", stop);
    handle.addEventListener("pointercancel", stop);
  };

  return (
    <section
      aria-label={LAST_RUN}
      style={{ height: `${String(run.stripOpen ? run.stripHeight : STRIP_HEIGHT_COLLAPSED)}px` }}
      className="relative flex flex-none border-t border-line bg-main-900"
    >
      <div
        role="separator"
        aria-orientation="horizontal"
        aria-label={RESIZE_LABEL}
        aria-valuenow={run.stripHeight}
        onPointerDown={startResize}
        onDoubleClick={run.toggleStrip}
        className="absolute inset-x-0 -top-1 z-10 h-2 cursor-row-resize hover:bg-accent-line"
      />
      <div className="flex w-82.5 flex-none flex-col border-r border-line">
        <div className="flex h-9.5 flex-none items-center gap-2.25 border-b border-line px-3.5">
          <h3 className={`${SECTION} flex-1`}>{LAST_RUN}</h3>
          {view === null ? null : (
            <span
              className={`inline-flex h-5 items-center gap-1.25 rounded-[5px] bg-main-700 px-1.75 text-[10.5px] font-medium ${PHASE_TEXT[view.phase]}`}
            >
              <StatusDot tone={PHASE_TONES[view.phase]} />
              {RUN_PHASE_LABELS[view.phase]}
            </span>
          )}
          {view === null ? null : (
            <span className="font-mono text-[10.5px] text-main-500">
              {durationLabel(runDuration(view, now))}
            </span>
          )}
          <IconButton
            path={run.stripOpen ? mdiChevronDown : mdiChevronUp}
            label={run.stripOpen ? "Свернуть журнал" : "Развернуть журнал"}
            tooltip="top-right"
            iconSize={16}
            className="size-6"
            onClick={run.toggleStrip}
          />
        </div>
        {run.stripOpen ? (
          <ScrollArea className="min-h-0 flex-1">
            <StepTree editor={editor} />
          </ScrollArea>
        ) : null}
      </div>
      <div className="flex min-w-0 flex-1 flex-col">
        <div className="flex h-9.5 flex-none items-center gap-1.5 border-b border-line px-3.5">
          {TABS.map((tab) => (
            <button
              key={tab}
              type="button"
              aria-pressed={run.tab === tab}
              className={`inline-flex h-7 items-center rounded-[6px] px-3 text-[12.5px] font-medium ${
                run.tab === tab ? "bg-main-700 text-main-50" : "text-main-400 hover:text-main-100"
              }`}
              onClick={() => {
                run.setTab(tab);
                if (!run.stripOpen) run.toggleStrip();
              }}
            >
              {RUN_TAB_LABELS[tab]}
            </button>
          ))}
          <span className="flex-1" />
          <StepActions editor={editor} />
          {step === null ? null : (
            <span className="truncate font-mono text-[10.5px] text-main-500">
              {`${stepName(editor, step)} · начат ${clockLabel(step.startedAt)}`}
            </span>
          )}
        </div>
        {run.stripOpen ? (
          <ScrollArea className="min-h-0 flex-1">
            <StepDetail editor={editor} />
          </ScrollArea>
        ) : null}
      </div>
    </section>
  );
}

export default observer(RunLogStrip);
