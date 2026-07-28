import {
  BaseEdge,
  getSmoothStepPath,
  getStraightPath,
  Position,
  type EdgeProps,
} from "@xyflow/react";

/**
 * Structure (Parent) edges always leave the parent **bottom** and enter the child
 * **top**, even when the child sits far left/right of the parent (root fan-out).
 *
 * Default smoothstep + side handle inference made the last path segment run
 * horizontally into 意图/结论, so markerEnd arrows pointed sideways instead of down.
 */
export function StructureEdge({
  id,
  sourceX,
  sourceY,
  targetX,
  targetY,
  style,
  markerEnd,
}: EdgeProps) {
  const sameColumn = Math.abs(targetX - sourceX) < 24;
  const childBelow = targetY >= sourceY - 4;

  if (sameColumn && childBelow) {
    const [path] = getStraightPath({
      sourceX,
      sourceY,
      targetX,
      targetY,
    });
    return <BaseEdge id={id} path={path} style={style} markerEnd={markerEnd} />;
  }

  // Fan-out / offset: force Bottom → Top so the final segment is always a
  // downward stub into the child; arrow tip then points down into the card.
  const [path] = getSmoothStepPath({
    sourceX,
    sourceY,
    targetX,
    targetY,
    sourcePosition: Position.Bottom,
    targetPosition: Position.Top,
    borderRadius: 14,
    // Push the horizontal run away from the cards so the vertical drop onto
    // the child is long enough for a clear downward arrow.
    offset: 28,
  });

  return <BaseEdge id={id} path={path} style={style} markerEnd={markerEnd} />;
}
