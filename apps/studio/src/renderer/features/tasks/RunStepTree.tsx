import type { RunGraph, StepDto } from "@zvs/shared";
import StatusDot from "../../ui/atoms/StatusDot";
import { formatDuration, STEP_TONES } from "./runPresentation";

export interface RunStepTreeProps {
  readonly graph: RunGraph | null;
  readonly steps: readonly StepDto[];
  readonly now: number;
}

interface StepRow {
  readonly nodeId: string;
  readonly type: string;
  readonly step: StepDto | undefined;
}

function rowsOf(graph: RunGraph | null, steps: readonly StepDto[]): StepRow[] {
  const latest = new Map<string, StepDto>();
  for (const step of steps) {
    const known = latest.get(step.nodeId);
    if (!known || step.attempt >= known.attempt) latest.set(step.nodeId, step);
  }
  if (graph)
    return graph.nodes.map((node) => ({
      nodeId: node.id,
      type: node.type,
      step: latest.get(node.id),
    }));
  return [...latest.values()].map((step) => ({
    nodeId: step.nodeId,
    type: step.type,
    step,
  }));
}

export default function RunStepTree({ graph, steps, now }: RunStepTreeProps) {
  const rows = rowsOf(graph, steps);
  if (rows.length === 0)
    return <p className="text-[11.5px] text-main-500">У этого запуска нет шагов.</p>;
  return (
    <ol className="m-0 flex list-none flex-col gap-1.5 p-0">
      {rows.map((row) => {
        const step = row.step;
        const pending = step === undefined;
        const timing =
          step === undefined
            ? ""
            : step.status === "running"
              ? formatDuration(Math.max(0, now - step.startedAt))
              : formatDuration(Math.max(0, (step.finishedAt ?? step.startedAt) - step.startedAt));
        return (
          <li
            key={row.nodeId}
            className={`flex items-center gap-2 text-[11.5px] ${pending ? "opacity-50" : ""}`}
          >
            <StatusDot tone={step === undefined ? "idle" : (STEP_TONES[step.status] ?? "idle")} />
            <span
              className={`min-w-0 flex-1 truncate ${
                step?.status === "running" ? "font-medium text-accent-medium" : "text-main-300"
              }`}
              title={`${row.nodeId} · ${row.type}`}
            >
              {row.nodeId}
            </span>
            <span className="flex-none font-mono text-[10.5px] text-main-500">{timing}</span>
          </li>
        );
      })}
    </ol>
  );
}
