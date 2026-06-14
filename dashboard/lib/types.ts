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

export type DashboardTab = "overview" | "findings" | "evidence" | "execution" | "provenance";
export type Locale = "en" | "zh-CN";

// ── Stage 4: Evidence / Artifact ────────────────────────────────────

export interface Artifact {
  artifact_schema_version: string;
  artifact_id: string;
  run_id: string;
  filename: string;
  content_type: string;
  size_bytes: number;
  sha256_hash: string;
  storage_key: string;
  created_at: string;
}

export interface ArtifactListResponse {
  artifacts: Artifact[];
}

export interface Evidence {
  evidence_schema_version: string;
  evidence_id: string;
  run_id: string;
  source_type: string;
  source_url: string;
  source_revision: string | null;
  title: string;
  excerpt: string;
  retrieved_at: string;
  snapshot_artifact_id: string | null;
  supports: string[];
  contradicts: string[];
  metadata: Record<string, unknown>;
  size_bytes: number;
  content_hash: string;
  storage_key: string;
  created_at: string;
}

export interface EvidenceListResponse {
  evidence: Evidence[];
}

// ── Stage 6: Check Results ──────────────────────────────────────────

export interface Finding {
  check_id: string;
  finding: "pass" | "partial" | "fail" | "unknown" | "not_applicable";
  rationale: string;
  evidence_ids: string[];
  not_applicable_reason: string | null;
}

export interface DimensionScore {
  dimension_id: string;
  score: number;
  applicable_check_count: number;
  weighted_points: number;
  max_weighted_points: number;
}

export interface DimensionScores {
  capability_clarity: DimensionScore;
  interface_openness: DimensionScore;
  automation_readiness: DimensionScore;
  data_portability: DimensionScore;
  permission_risk: DimensionScore;
  evidence_quality: DimensionScore;
  ecosystem_fit: DimensionScore;
}

export type Rating =
  | "not_recommended"
  | "manual_only"
  | "trial"
  | "low_risk"
  | "core_candidate";

export interface CheckResults {
  check_results_schema_version: string;
  check_results_id: string;
  run_id: string;
  evidence_board_version: number;
  standard_id: string;
  standard_version: string;
  profile_id: string;
  profile_version: string;
  results: Finding[];
  dimension_scores: DimensionScores;
  total_score: number;
  rating: Rating;
  created_at: string;
}

// ── Stage 6: Frozen Evidence Board / Manifest ───────────────────────

export interface EvidenceBoardClaim {
  claim_id: string;
  check_id: string;
  statement: string;
  confidence: number;
  supports: string[];
  contradicts: string[];
}

export interface EvidenceBoardGap {
  gap_id: string;
  check_id: string;
  description: string;
  priority: "high" | "medium" | "low";
  status: "open" | "resolved" | "accepted";
  resolution: string | null;
}

export interface FrozenEvidenceBoard {
  evidence_board_schema_version: string;
  run_id: string;
  version: number;
  standard_id: string;
  standard_version: string;
  profile_id: string;
  profile_version: string;
  evidence_ids: string[];
  claims: EvidenceBoardClaim[];
  gaps: EvidenceBoardGap[];
  freeze_reason: string;
  frozen_at: string;
}

export interface EvidenceManifestEntry {
  evidence_id: string;
  content_hash: string;
  snapshot_artifact_id: string | null;
  snapshot_hash: string | null;
}

export interface FrozenEvidenceManifest {
  evidence_manifest_schema_version: string;
  run_id: string;
  evidence_board_version: number;
  entries: EvidenceManifestEntry[];
}

export interface EvidenceFreezeResult {
  evidence_board: FrozenEvidenceBoard;
  evidence_manifest: FrozenEvidenceManifest;
}

// ── Stage 6: Passport / Provenance ──────────────────────────────────

export interface PassportStatement {
  statement_id: string;
  statement: string;
  evidence_ids: string[];
}

export interface PassportRisk {
  risk_id: string;
  title: string;
  description: string;
  evidence_ids: string[];
  mitigation: string | null;
}

export interface PassportDimensionScores {
  capability_clarity: number;
  interface_openness: number;
  automation_readiness: number;
  data_portability: number;
  permission_risk: number;
  evidence_quality: number;
  ecosystem_fit: number;
}

export interface PassportScores {
  dimensions: PassportDimensionScores;
  total_score: number;
  rating: Rating;
}

export interface Recommendation {
  summary: string;
  conditions: string[];
}

export interface Passport {
  passport_version: string;
  passport_sequence: number;
  tool_id: string;
  run_id: string;
  tool_type: string;
  target_revision: string | null;
  audit_scope: string;
  standard_id: string;
  standard_version: string;
  profile_id: string;
  profile_version: string;
  evidence_board_version: number;
  check_results_id: string;
  capability_claims: PassportStatement[];
  interfaces: PassportStatement[];
  risks: PassportRisk[];
  known_gaps: string[];
  scores: PassportScores;
  recommendation: Recommendation;
}

export interface Provenance {
  provenance_schema_version: string;
  run_id: string;
  freeze_version: number;
  evidence_board_version: number;
  passport_sequence: number;
  passport_hash: string;
  audit_log_hash: string;
  evidence_manifest_hash: string;
  onchain_run_id: string;
  frozen_at: string;
}

export interface PassportFreezeResult {
  passport: Passport;
  provenance: Provenance;
}

export type ApprovalDecision = "approve_offchain" | "approve_testnet_attestation" | "reject";

export interface Approval {
  approval_schema_version: "0.1.0";
  approval_id: string;
  run_id: string;
  decision: ApprovalDecision;
  passport_sequence: number;
  passport_hash: string;
  audit_log_hash: string;
  evidence_manifest_hash: string;
  chain_id: number | null;
  registry_contract: string | null;
  decided_at: string;
}

export interface AttestationReceipt {
  attestation_receipt_schema_version: "0.1.0";
  attestation_id: string;
  run_id: string;
  tool_id: string;
  passport_hash: string;
  audit_log_hash: string;
  evidence_manifest_hash: string;
  onchain_run_id: string;
  chain_id: number;
  registry_contract: string;
  status: "submitted" | "confirmed" | "failed";
  transaction_hash: string | null;
  submitted_at: string;
  confirmed_at: string | null;
}

// ── Events list ─────────────────────────────────────────────────────

export interface EventListResponse {
  events: RunEvent[];
}
