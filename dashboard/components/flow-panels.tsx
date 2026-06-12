"use client";

import { Background, MarkerType, ReactFlow, type Edge, type Node } from "@xyflow/react";

import type { TranslationKey } from "@/lib/i18n";

type Translator = (key: TranslationKey) => string;

const nodeStyle = {
  background: "#0c1723",
  border: "1px solid #27435d",
  borderRadius: 8,
  color: "#dcecff",
  fontSize: 11,
  padding: "9px 12px",
  width: 138,
};

function activeNodeStyle(active: boolean) {
  return active
    ? {
        ...nodeStyle,
        border: "1px solid #3198ff",
        boxShadow: "0 0 18px rgba(49, 152, 255, 0.24)",
        color: "#ffffff",
      }
    : nodeStyle;
}

export function ExecutionFlow({ currentNode, t }: { currentNode: string | null; t: Translator }) {
  const nodes: Node[] = [
    { id: "intake_normalization", position: { x: 10, y: 80 }, data: { label: "Intake" }, style: activeNodeStyle(currentNode === "intake_normalization") },
    { id: "profile_selector", position: { x: 190, y: 20 }, data: { label: "Profile selector" }, style: activeNodeStyle(currentNode === "profile_selector") },
    { id: "audit_plan_builder", position: { x: 190, y: 140 }, data: { label: "Audit plan" }, style: activeNodeStyle(currentNode === "audit_plan_builder" || currentNode === "plan_audit") },
    { id: "investigation_round", position: { x: 380, y: 20 }, data: { label: "Investigation" }, style: activeNodeStyle(currentNode === "investigation_round") },
    { id: "gap_analysis", position: { x: 380, y: 140 }, data: { label: "Gap analysis" }, style: activeNodeStyle(currentNode === "gap_analysis") },
    { id: "skeptic_review", position: { x: 570, y: 80 }, data: { label: "Skeptic review" }, style: activeNodeStyle(currentNode === "skeptic_review") },
  ];
  const edges: Edge[] = [
    { id: "e1", source: "intake_normalization", target: "profile_selector", markerEnd: { type: MarkerType.ArrowClosed }, animated: true },
    { id: "e2", source: "intake_normalization", target: "audit_plan_builder", markerEnd: { type: MarkerType.ArrowClosed } },
    { id: "e3", source: "profile_selector", target: "investigation_round", markerEnd: { type: MarkerType.ArrowClosed } },
    { id: "e4", source: "audit_plan_builder", target: "gap_analysis", markerEnd: { type: MarkerType.ArrowClosed } },
    { id: "e5", source: "investigation_round", target: "skeptic_review", markerEnd: { type: MarkerType.ArrowClosed } },
    { id: "e6", source: "gap_analysis", target: "skeptic_review", markerEnd: { type: MarkerType.ArrowClosed }, animated: true },
  ];

  return (
    <FlowFrame title={t("executionPreview")} detail={t("executionPreviewDetail")}>
      <ReactFlow
        aria-label={t("executionPreview")}
        nodes={nodes}
        edges={edges}
        fitView
        nodesConnectable={false}
        nodesDraggable={false}
        elementsSelectable={false}
        panOnDrag={false}
        zoomOnScroll={false}
        proOptions={{ hideAttribution: true }}
      >
        <Background color="#20364c" gap={24} size={1} />
      </ReactFlow>
    </FlowFrame>
  );
}

export function ProvenanceFlow({ t }: { t: Translator }) {
  const nodes: Node[] = [
    { id: "tool", position: { x: 20, y: 75 }, data: { label: "Tool identity" }, style: nodeStyle },
    { id: "run", position: { x: 210, y: 75 }, data: { label: "Audit run" }, style: nodeStyle },
    { id: "events", position: { x: 400, y: 15 }, data: { label: "Append-only events" }, style: nodeStyle },
    { id: "evidence", position: { x: 400, y: 135 }, data: { label: "Evidence board" }, style: nodeStyle },
    { id: "passport", position: { x: 590, y: 75 }, data: { label: "Frozen passport" }, style: nodeStyle },
  ];
  const edges: Edge[] = [
    { id: "p1", source: "tool", target: "run", markerEnd: { type: MarkerType.ArrowClosed }, animated: true },
    { id: "p2", source: "run", target: "events", markerEnd: { type: MarkerType.ArrowClosed } },
    { id: "p3", source: "run", target: "evidence", markerEnd: { type: MarkerType.ArrowClosed } },
    { id: "p4", source: "events", target: "passport", markerEnd: { type: MarkerType.ArrowClosed }, animated: true },
    { id: "p5", source: "evidence", target: "passport", markerEnd: { type: MarkerType.ArrowClosed } },
  ];

  return (
    <FlowFrame title={t("provenancePreview")} detail={t("provenancePreviewDetail")}>
      <ReactFlow
        aria-label={t("provenancePreview")}
        nodes={nodes}
        edges={edges}
        fitView
        nodesConnectable={false}
        nodesDraggable={false}
        elementsSelectable={false}
        panOnDrag={false}
        zoomOnScroll={false}
        proOptions={{ hideAttribution: true }}
      >
        <Background color="#20364c" gap={24} size={1} />
      </ReactFlow>
    </FlowFrame>
  );
}

function FlowFrame({ title, detail, children }: { title: string; detail: string; children: React.ReactNode }) {
  return (
    <section className="flow-frame">
      <div className="section-heading">
        <div>
          <span className="preview-pill">Preview</span>
          <h2>{title}</h2>
          <p>{detail}</p>
        </div>
      </div>
      <div className="flow-canvas">{children}</div>
    </section>
  );
}
