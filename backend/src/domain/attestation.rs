use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AttestationStatus {
    Submitted,
    Confirmed,
    Failed,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AttestationReceipt {
    pub attestation_receipt_schema_version: String,
    pub attestation_id: Uuid,
    pub run_id: Uuid,
    pub tool_id: String,
    pub passport_hash: String,
    pub audit_log_hash: String,
    pub evidence_manifest_hash: String,
    pub onchain_run_id: String,
    pub chain_id: u64,
    pub registry_contract: String,
    pub status: AttestationStatus,
    pub transaction_hash: Option<String>,
    pub submitted_at: DateTime<Utc>,
    pub confirmed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct AttestationCommitment {
    pub run_id: Uuid,
    pub tool_id: String,
    pub tool_type: String,
    pub passport_hash: String,
    pub audit_log_hash: String,
    pub evidence_manifest_hash: String,
    pub onchain_run_id: String,
    pub chain_id: u64,
    pub registry_contract: String,
}
