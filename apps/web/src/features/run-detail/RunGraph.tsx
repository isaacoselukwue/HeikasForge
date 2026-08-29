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

const NODE_LAYOUT: Record<string, [number, number]> = {
  prepare: [0, 0],
  plan: [1, 0],
  approval: [2, 0],
  fan_out: [3, 0],
  implement_candidate: [0, 1],
  test_candidate: [1, 1],
  review_candidate: [2, 1],
  join: [3, 1],
  repair_candidate: [1, 2],
  integrate_winner: [0, 3],
  final_test: [1, 3],
  final_review: [2, 3],
  commit_approval: [3, 3],
  commit: [4, 3],
};

const COLUMN_WIDTH = 196;
const ROW_HEIGHT = 118;
const NODE_WIDTH = 168;

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
        const [column, row] = NODE_LAYOUT[node.id] ?? [0, 0];
        const colour = STATE_COLOUR[node.state] ?? "var(--state-neutral)";
        return {
          id: node.id,
          position: { x: column * COLUMN_WIDTH, y: row * ROW_HEIGHT },
          sourcePosition: Position.Right,
          targetPosition: Position.Left,
          data: {
            label: (
              <div className="flex flex-col gap-0.5 text-left" style={{ width: NODE_WIDTH - 26 }}>
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
            padding: "8px 12px",
            width: NODE_WIDTH,
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
      className="h-[460px] w-full overflow-hidden rounded-[var(--radius-medium)] border border-[var(--border-subtle)] bg-[var(--surface-sunken)]"
      role="group"
      aria-label="Run graph. Use the tab key to reach the graph controls."
    >
      <ReactFlow
        nodes={renderedNodes.length === flowNodes.length ? flowNodes : applyLatest()}
        edges={flowEdges.length === renderedEdges.length ? flowEdges : renderedEdges}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        fitView
        fitViewOptions={{ padding: 0.08 }}
        proOptions={{ hideAttribution: true }}
        nodesDraggable={false}
        nodesConnectable={false}
        elementsSelectable
        minZoom={0.3}
        maxZoom={1.8}
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
