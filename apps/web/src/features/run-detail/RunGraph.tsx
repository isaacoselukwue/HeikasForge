import { useCallback, useMemo } from "react";
import {
  Background,
  Controls,
  ReactFlow,
  useEdgesState,
  useNodesState,
  Position,
} from "@xyflow/react";
import type { Edge, Node } from "@xyflow/react";

import { useReducedMotion } from "@/hooks/useReducedMotion";
import type { GraphEdgeView, GraphNodeView } from "@/generated/api-types";

const COLUMN_ORDER: Record<string, [number, number]> = {
  prepare: [0, 0],
  plan: [1, 0],
  approval: [2, 0],
  fan_out: [3, 0],
  implement_candidate: [4, -1],
  test_candidate: [5, -1],
  review_candidate: [6, -1],
  repair_candidate: [5, 1],
  join: [7, 0],
  integrate_winner: [8, 0],
  final_test: [9, 0],
  final_review: [10, 0],
  commit_approval: [11, 0],
  commit: [12, 0],
};

const STATE_COLOUR: Record<string, string> = {
  pending: "var(--state-neutral)",
  active: "var(--accent-primary)",
  succeeded: "var(--state-success)",
  failed: "var(--state-failure)",
  paused: "var(--state-warning)",
  skipped: "var(--state-neutral)",
};

export interface RunGraphProps {
  nodes: GraphNodeView[];
  edges: GraphEdgeView[];
}

export function RunGraph({ nodes, edges }: RunGraphProps) {
  const reducedMotion = useReducedMotion();

  const flowNodes = useMemo<Node[]>(
    () =>
      nodes.map((node) => {
        const [column, lane] = COLUMN_ORDER[node.id] ?? [0, 0];
        const colour = STATE_COLOUR[node.state] ?? "var(--state-neutral)";
        return {
          id: node.id,
          position: { x: column * 190, y: lane * 130 + 140 },
          sourcePosition: Position.Right,
          targetPosition: Position.Left,
          data: {
            label: (
              <div className="flex w-40 flex-col gap-1 text-left">
                <span className="text-[13px] font-semibold text-[var(--text-primary)]">
                  {node.label}
                </span>
                <span className="text-[11px] text-[var(--text-muted)]">
                  {node.attempts > 0
                    ? `${String(node.attempts)} attempt${node.attempts === 1 ? "" : "s"}`
                    : "not started"}
                </span>
              </div>
            ),
          },
          className: node.state === "active" && !reducedMotion ? "node-pulse" : undefined,
          style: {
            background: "var(--surface-raised)",
            border: `1px solid ${colour}`,
            borderRadius: "10px",
            padding: "10px 12px",
            width: 176,
          },
          ariaLabel: `${node.label}, ${node.state.replace(/_/g, " ")}, ${String(node.attempts)} attempts`,
        } satisfies Node;
      }),
    [nodes, reducedMotion],
  );

  const flowEdges = useMemo<Edge[]>(
    () =>
      edges.map((edge, index) => ({
        id: `${edge.from}-${edge.to}-${String(index)}`,
        source: edge.from,
        target: edge.to,
        label: edge.label,
        animated: edge.traversed && !reducedMotion,
        style: {
          stroke: edge.traversed ? "var(--accent-primary)" : "var(--border-subtle)",
          strokeWidth: edge.traversed ? 2 : 1,
        },
        labelStyle: { fill: "var(--text-muted)", fontSize: 10 },
        labelBgStyle: { fill: "var(--surface-raised)" },
      })),
    [edges, reducedMotion],
  );

  const [renderedNodes, , onNodesChange] = useNodesState(flowNodes);
  const [renderedEdges, , onEdgesChange] = useEdgesState(flowEdges);

  const applyLatest = useCallback(() => flowNodes, [flowNodes]);

  return (
    <div
      className="h-[420px] w-full overflow-hidden rounded-[var(--radius-medium)] border border-[var(--border-subtle)] bg-[var(--surface-sunken)]"
      role="group"
      aria-label="Run graph. Use the tab key to reach the graph controls."
    >
      <ReactFlow
        nodes={renderedNodes.length === flowNodes.length ? flowNodes : applyLatest()}
        edges={flowEdges.length === renderedEdges.length ? flowEdges : renderedEdges}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        fitView
        fitViewOptions={{ padding: 0.18 }}
        proOptions={{ hideAttribution: true }}
        nodesDraggable={false}
        nodesConnectable={false}
        elementsSelectable
        minZoom={0.4}
        maxZoom={1.6}
      >
        <Background color="var(--border-subtle)" gap={18} />
        <Controls
          showInteractive={false}
          className="rounded-[var(--radius-medium)] border border-[var(--border-subtle)] bg-[var(--surface-raised)]"
        />
      </ReactFlow>
    </div>
  );
}
