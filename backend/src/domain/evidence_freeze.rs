use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimStatus {
    Supported,
    PartiallySupported,
    Unsupported,
    Contradicted,
    NotChecked,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GapPriority {
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GapStatus {
    Open,
    Resolved,
    Accepted,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceBoardClaim {
    pub claim_id: String,
    pub check_id: String,
    pub statement: String,
    pub status: ClaimStatus,
    pub confidence: f64,
    pub supports: Vec<Uuid>,
    pub contradicts: Vec<Uuid>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceBoardGap {
    pub gap_id: String,
    pub check_id: String,
    pub description: String,
    pub priority: GapPriority,
    pub status: GapStatus,
    pub resolution: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FreezeEvidenceBoardRequest {
    pub evidence_board_schema_version: String,
    pub version: u64,
    pub evidence_ids: Vec<Uuid>,
    pub claims: Vec<EvidenceBoardClaim>,
    pub gaps: Vec<EvidenceBoardGap>,
    pub freeze_reason: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FrozenEvidenceBoard {
    pub evidence_board_schema_version: String,
    pub run_id: Uuid,
    pub version: u64,
    pub standard_id: String,
    pub standard_version: String,
    pub profile_id: String,
    pub profile_version: String,
    pub evidence_ids: Vec<Uuid>,
    pub claims: Vec<EvidenceBoardClaim>,
    pub gaps: Vec<EvidenceBoardGap>,
    pub freeze_reason: String,
    pub frozen_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EvidenceManifestEntry {
    pub evidence_id: Uuid,
    pub content_hash: String,
    pub snapshot_artifact_id: Option<Uuid>,
    pub snapshot_hash: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FrozenEvidenceManifest {
    pub evidence_manifest_schema_version: String,
    pub run_id: Uuid,
    pub evidence_board_version: u64,
    pub entries: Vec<EvidenceManifestEntry>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EvidenceFreezeResult {
    pub evidence_board: FrozenEvidenceBoard,
    pub evidence_manifest: FrozenEvidenceManifest,
}
