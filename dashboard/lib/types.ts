export type RunStatus =
  | "pending"
  | "running"
  | "success"
  | "failed"
  | "waiting_approval"
  | "cancelled";

export type RunEventType =
  // v0.1 lifecycle events
  | "run_created"
  | "run_status_changed"
  | "node_started"
  | "node_finished"
  | "artifact_created"
  | "evidence_created"
  | "approval_required"
  | "approval_resolved"
  | "attestation_submitted"
  | "attestation_confirmed"
  | "error"
  // v0.2 decision events
  | "profile_selected"
  | "hypothesis_created"
  | "hypothesis_updated"
  | "research_query_planned"
  | "gap_detected"
  | "evidence_linked"
  | "claim_contradicted"
  | "evidence_board_frozen"
  | "review_issue_found"
  | "score_changed"
  | "directives_accepted"
  | "human_feedback_received"
  | "provenance_frozen";

export interface ToolInput {
  name: string;
  tool_type: string;
  urls: string[];
}

export interface Run {
  run_id: string;
  goal: string;
  tool_id: string;
  canonical_url: string;
  tool: ToolInput;
  status: RunStatus;
  current_node: string | null;
  created_at: string;
  updated_at: string;
}

export interface RunEvent {
  event_id: string;
  run_id: string;
  sequence: number;
  node_id: string;
  event_type: RunEventType;
  payload: Record<string, unknown>;
  created_at: string;
  // v0.2 hash chain
  event_hash: string;
  prev_event_hash: string;
}

export interface RunListResponse {
  runs: Run[];
}

export interface RunDetails {
  run: Run;
  events: RunEvent[];
}

export interface HealthResponse {
  status: "ok";
  service: string;
}

export interface ApiErrorBody {
  code: string;
  message: string;
  details: unknown;
}

export type PreviewSeverity = "critical" | "high" | "medium" | "low";

export interface PreviewDimension {
  id: string;
  score: number;
  labelKey: string;
}

export interface PreviewFinding {
  id: string;
  titleKey: string;
  detailKey: string;
  severity: PreviewSeverity;
  evidence: string;
}

export interface PreviewPassportResult {
  kind: "preview";
  score: number;
  coverage: number;
  confidenceKey: string;
  assessmentKey: string;
  dimensions: PreviewDimension[];
  findings: PreviewFinding[];
  capabilities: string[];
  gaps: string[];
  limitations: string[];
  hashes: {
    passport: string;
    auditLog: string;
    evidenceManifest: string;
  };
}

export type DashboardTab = "overview" | "findings" | "evidence" | "execution" | "provenance";
export type Locale = "en" | "zh-CN";
