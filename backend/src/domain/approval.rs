use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalDecision {
    ApproveOffchain,
    ApproveTestnetAttestation,
    Reject,
}

impl ApprovalDecision {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ApproveOffchain => "approve_offchain",
            Self::ApproveTestnetAttestation => "approve_testnet_attestation",
            Self::Reject => "reject",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateApprovalRequest {
    pub approval_schema_version: String,
    pub decision: ApprovalDecision,
    pub passport_sequence: u64,
    pub passport_hash: String,
    pub audit_log_hash: String,
    pub evidence_manifest_hash: String,
    pub chain_id: Option<u64>,
    pub registry_contract: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Approval {
    pub approval_schema_version: String,
    pub approval_id: Uuid,
    pub run_id: Uuid,
    pub decision: ApprovalDecision,
    pub passport_sequence: u64,
    pub passport_hash: String,
    pub audit_log_hash: String,
    pub evidence_manifest_hash: String,
    pub chain_id: Option<u64>,
    pub registry_contract: Option<String>,
    pub decided_at: DateTime<Utc>,
}
