import { BaseEdge, EdgeLabelRenderer, getBezierPath, type EdgeProps } from "@xyflow/react";
import { memo } from "react";
import type { ScenarioFlowEdge } from "./flowModel";
import { itemsLabel } from "./runCopy";

function ScenarioEdgeView({
  id,
  sourceX,
  sourceY,
  targetX,
  targetY,
  sourcePosition,
  targetPosition,
  selected,
  data,
}: EdgeProps<ScenarioFlowEdge>) {
  const attachment = data?.kind === "attachment";
  const traversed = data?.traversed === true;
  const [path, labelX, labelY] = getBezierPath({
    sourceX,
    sourceY,
    targetX,
    targetY,
    sourcePosition,
    targetPosition,
    curvature: attachment ? 0.35 : 0.4,
  });
  const stroke = selected
    ? "var(--color-accent-dark)"
    : traversed
      ? "var(--color-ok)"
      : attachment
        ? "var(--color-main-500)"
        : "var(--color-main-600)";
  const items = data?.items ?? null;
  return (
    <>
      <BaseEdge
        id={id}
        path={path}
        interactionWidth={18}
        style={{
          stroke,
          strokeWidth: attachment ? 1.4 : 1.6,
          strokeDasharray: attachment ? "4 4" : undefined,
        }}
      />
      {traversed && items !== null && !attachment ? (
        <EdgeLabelRenderer>
          <span
            className="nodrag nopan pointer-events-none absolute font-mono text-[10px] text-main-500"
            style={{
              transform: `translate(-50%, -100%) translate(${String(labelX)}px, ${String(labelY - 4)}px)`,
            }}
          >
            {itemsLabel(items)}
          </span>
        </EdgeLabelRenderer>
      ) : null}
    </>
  );
}

export default memo(ScenarioEdgeView);
