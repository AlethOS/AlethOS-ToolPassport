use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunEventType {
    // v0.1 lifecycle events
    RunCreated,
    RunStatusChanged,
    NodeStarted,
    NodeFinished,
    ArtifactCreated,
    EvidenceCreated,
    ApprovalRequired,
    ApprovalResolved,
    AttestationSubmitted,
    AttestationConfirmed,
    Error,
    // v0.2 decision events
    ProfileSelected,
    HypothesisCreated,
    HypothesisUpdated,
    ResearchQueryPlanned,
    GapDetected,
    EvidenceLinked,
    ClaimContradicted,
    EvidenceBoardFrozen,
    ReviewIssueFound,
    ScoreChanged,
    DirectivesAccepted,
    HumanFeedbackReceived,
    ProvenanceFrozen,
}

impl RunEventType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RunCreated => "run_created",
            Self::RunStatusChanged => "run_status_changed",
            Self::NodeStarted => "node_started",
            Self::NodeFinished => "node_finished",
            Self::ArtifactCreated => "artifact_created",
            Self::EvidenceCreated => "evidence_created",
            Self::ApprovalRequired => "approval_required",
            Self::ApprovalResolved => "approval_resolved",
            Self::AttestationSubmitted => "attestation_submitted",
            Self::AttestationConfirmed => "attestation_confirmed",
            Self::Error => "error",
            Self::ProfileSelected => "profile_selected",
            Self::HypothesisCreated => "hypothesis_created",
            Self::HypothesisUpdated => "hypothesis_updated",
            Self::ResearchQueryPlanned => "research_query_planned",
            Self::GapDetected => "gap_detected",
            Self::EvidenceLinked => "evidence_linked",
            Self::ClaimContradicted => "claim_contradicted",
            Self::EvidenceBoardFrozen => "evidence_board_frozen",
            Self::ReviewIssueFound => "review_issue_found",
            Self::ScoreChanged => "score_changed",
            Self::DirectivesAccepted => "directives_accepted",
            Self::HumanFeedbackReceived => "human_feedback_received",
            Self::ProvenanceFrozen => "provenance_frozen",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "run_created" => Some(Self::RunCreated),
            "run_status_changed" => Some(Self::RunStatusChanged),
            "node_started" => Some(Self::NodeStarted),
            "node_finished" => Some(Self::NodeFinished),
            "artifact_created" => Some(Self::ArtifactCreated),
            "evidence_created" => Some(Self::EvidenceCreated),
            "approval_required" => Some(Self::ApprovalRequired),
            "approval_resolved" => Some(Self::ApprovalResolved),
            "attestation_submitted" => Some(Self::AttestationSubmitted),
            "attestation_confirmed" => Some(Self::AttestationConfirmed),
            "error" => Some(Self::Error),
            "profile_selected" => Some(Self::ProfileSelected),
            "hypothesis_created" => Some(Self::HypothesisCreated),
            "hypothesis_updated" => Some(Self::HypothesisUpdated),
            "research_query_planned" => Some(Self::ResearchQueryPlanned),
            "gap_detected" => Some(Self::GapDetected),
            "evidence_linked" => Some(Self::EvidenceLinked),
            "claim_contradicted" => Some(Self::ClaimContradicted),
            "evidence_board_frozen" => Some(Self::EvidenceBoardFrozen),
            "review_issue_found" => Some(Self::ReviewIssueFound),
            "score_changed" => Some(Self::ScoreChanged),
            "directives_accepted" => Some(Self::DirectivesAccepted),
            "human_feedback_received" => Some(Self::HumanFeedbackReceived),
            "provenance_frozen" => Some(Self::ProvenanceFrozen),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppendRunEventRequest {
    pub node_id: String,
    pub event_type: RunEventType,
    pub payload: Map<String, Value>,
}

/// Zero-value hash for the first event in a run's hash chain.
pub const ZERO_HASH: &str = "0x0000000000000000000000000000000000000000000000000000000000000000";

#[derive(Debug, Clone, Serialize)]
pub struct RunEvent {
    pub event_id: Uuid,
    pub run_id: Uuid,
    pub sequence: i64,
    pub node_id: String,
    pub event_type: RunEventType,
    pub payload: Map<String, Value>,
    pub created_at: DateTime<Utc>,
    pub event_hash: String,
    pub prev_event_hash: String,
}
