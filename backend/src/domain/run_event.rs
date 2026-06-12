use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunEventType {
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

#[derive(Debug, Clone, Serialize)]
pub struct RunEvent {
    pub event_id: Uuid,
    pub run_id: Uuid,
    #[serde(skip_serializing)]
    pub sequence: i64,
    pub node_id: String,
    pub event_type: RunEventType,
    pub payload: Map<String, Value>,
    pub created_at: DateTime<Utc>,
}
