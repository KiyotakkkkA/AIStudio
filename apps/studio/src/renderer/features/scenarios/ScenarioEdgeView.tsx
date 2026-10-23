import { BaseEdge, getBezierPath, type EdgeProps } from "@xyflow/react";
import { memo } from "react";
import type { ScenarioFlowEdge } from "./flowModel";

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
  const [path] = getBezierPath({
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
    : attachment
      ? "var(--color-main-500)"
      : "var(--color-main-600)";
  return (
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
  );
}

export default memo(ScenarioEdgeView);
