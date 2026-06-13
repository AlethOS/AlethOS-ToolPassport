use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceSourceType {
    UserMaterial,
    OfficialWebsite,
    OfficialDocs,
    GithubReadme,
    GithubMetadata,
    PublicExample,
}

impl EvidenceSourceType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UserMaterial => "user_material",
            Self::OfficialWebsite => "official_website",
            Self::OfficialDocs => "official_docs",
            Self::GithubReadme => "github_readme",
            Self::GithubMetadata => "github_metadata",
            Self::PublicExample => "public_example",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "user_material" => Some(Self::UserMaterial),
            "official_website" => Some(Self::OfficialWebsite),
            "official_docs" => Some(Self::OfficialDocs),
            "github_readme" => Some(Self::GithubReadme),
            "github_metadata" => Some(Self::GithubMetadata),
            "public_example" => Some(Self::PublicExample),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    pub evidence_schema_version: &'static str,
    pub evidence_id: Uuid,
    pub run_id: Uuid,
    pub source_type: EvidenceSourceType,
    pub source_url: String,
    pub source_revision: Option<String>,
    pub title: String,
    pub excerpt: String,
    pub retrieved_at: DateTime<Utc>,
    pub snapshot_artifact_id: Option<Uuid>,
    pub supports: Vec<String>,
    pub contradicts: Vec<String>,
    pub metadata: BTreeMap<String, Value>,
    pub size_bytes: i64,
    pub content_hash: String,
    #[serde(skip_serializing)]
    pub storage_key: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateEvidenceRequest {
    pub evidence_schema_version: String,
    pub source_type: EvidenceSourceType,
    pub source_url: String,
    pub source_revision: Option<String>,
    pub title: String,
    pub excerpt: String,
    pub retrieved_at: DateTime<Utc>,
    pub snapshot_artifact_id: Option<Uuid>,
    pub supports: Vec<String>,
    pub contradicts: Vec<String>,
    pub metadata: BTreeMap<String, Value>,
}
