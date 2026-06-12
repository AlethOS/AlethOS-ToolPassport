use serde::{Deserialize, Serialize};

use super::{ExternalIdentifier, ToolType};

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolveToolRequest {
    pub intake_version: String,
    pub name: String,
    pub tool_type: ToolType,
    #[serde(default)]
    pub urls: Vec<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionStatus {
    Resolved,
    CreateCandidate,
    NeedsReview,
}

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasonCode {
    ExistingIdentifierMatch,
    NewStrongIdentifier,
    NameOnly,
    NameMatchOnly,
    InvalidOrAmbiguousUrl,
    MultipleStrongIdentifiers,
    ConflictingExistingIdentifiers,
    PossibleForkOrSourceMigration,
    AdditionalIdentifierRequiresReview,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResolutionResponse {
    pub resolution_version: &'static str,
    pub status: ResolutionStatus,
    pub normalized_identifiers: Vec<ExternalIdentifier>,
    pub tool_id: Option<String>,
    pub candidate_tool_ids: Vec<String>,
    pub reason_codes: Vec<ReasonCode>,
}
