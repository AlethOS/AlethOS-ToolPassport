use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::check_results::Rating;
use super::evidence_freeze::ClaimStatus;

/// Risk severity reported in a Passport. Mirrors the `risk.severity` enum in
/// `passport-v0.2.schema.json`.
#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Critical,
    High,
    Medium,
    Low,
    Unknown,
}

/// A narrative statement bound to Evidence, reused for `capability_claims` and
/// `interfaces`.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PassportStatement {
    pub statement_id: String,
    pub statement: String,
    pub status: ClaimStatus,
    pub evidence_ids: Vec<Uuid>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PassportRisk {
    pub risk_id: String,
    pub title: String,
    pub severity: Severity,
    pub description: String,
    pub mitigation: Option<String>,
    pub evidence_ids: Vec<Uuid>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Recommendation {
    pub summary: String,
    pub conditions: Vec<String>,
}

/// Per-dimension integer scores (0..5) as required by the Passport v0.2 schema.
/// This is the integer projection of `CheckResults`' `DimensionScores`, which
/// also carry the weighted-point bookkeeping that does not belong in a Passport.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PassportDimensionScores {
    pub capability_clarity: u8,
    pub interface_openness: u8,
    pub automation_readiness: u8,
    pub data_portability: u8,
    pub permission_risk: u8,
    pub evidence_quality: u8,
    pub ecosystem_fit: u8,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PassportScores {
    pub dimensions: PassportDimensionScores,
    pub total_score: u8,
    pub rating: Rating,
}

/// Immutable Passport v0.2 content. Carries audit content and Rust-owned scores
/// but no commitment hashes; hashes live in the separate `Provenance` record.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Passport {
    pub passport_version: String,
    pub passport_sequence: u64,
    pub tool_id: String,
    pub run_id: Uuid,
    pub tool_type: String,
    pub target_revision: Option<String>,
    pub audit_scope: String,
    pub standard_id: String,
    pub standard_version: String,
    pub profile_id: String,
    pub profile_version: String,
    pub evidence_board_version: u64,
    pub check_results_id: Uuid,
    pub capability_claims: Vec<PassportStatement>,
    pub interfaces: Vec<PassportStatement>,
    pub risks: Vec<PassportRisk>,
    pub known_gaps: Vec<String>,
    pub scores: PassportScores,
    pub recommendation: Recommendation,
}

/// Frozen audit provenance binding the four Rust-owned commitment hashes to a
/// Run, Passport sequence and Evidence Board version. `audit_log_hash` is the
/// `event_hash` of the Trust-Core-owned `provenance_frozen` event and is filled
/// by the repository after that event is appended in the freeze transaction.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Provenance {
    pub provenance_schema_version: String,
    pub run_id: Uuid,
    pub freeze_version: u64,
    pub evidence_board_version: u64,
    pub passport_sequence: u64,
    pub passport_hash: String,
    pub audit_log_hash: String,
    pub evidence_manifest_hash: String,
    pub onchain_run_id: String,
    pub frozen_at: DateTime<Utc>,
}

/// Untrusted Passport content submitted for Rust-owned freeze. Callers supply
/// only narrative content plus the Evidence Board version to build from; Rust
/// owns the envelope, scores, `check_results_id`, `passport_sequence` and every
/// commitment hash.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FreezePassportRequest {
    pub passport_version: String,
    pub evidence_board_version: u64,
    pub target_revision: Option<String>,
    pub audit_scope: String,
    pub capability_claims: Vec<PassportStatement>,
    pub interfaces: Vec<PassportStatement>,
    pub risks: Vec<PassportRisk>,
    pub known_gaps: Vec<String>,
    pub recommendation: Recommendation,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PassportFreezeResult {
    pub passport: Passport,
    pub provenance: Provenance,
}
