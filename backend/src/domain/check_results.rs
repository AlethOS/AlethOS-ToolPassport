use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Finding {
    Pass,
    Partial,
    Fail,
    Unknown,
    NotApplicable,
}

impl Finding {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Partial => "partial",
            Self::Fail => "fail",
            Self::Unknown => "unknown",
            Self::NotApplicable => "not_applicable",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FindingSubmission {
    pub check_id: String,
    pub finding: Finding,
    pub rationale: String,
    pub evidence_ids: Vec<Uuid>,
    pub not_applicable_reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckResultsSubmission {
    pub check_results_schema_version: String,
    pub evidence_board_version: u64,
    pub findings: Vec<FindingSubmission>,
}

#[derive(Debug, Clone, Copy, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Rating {
    NotRecommended,
    ManualOnly,
    Trial,
    LowRisk,
    CoreCandidate,
}

impl Rating {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotRecommended => "not_recommended",
            Self::ManualOnly => "manual_only",
            Self::Trial => "trial",
            Self::LowRisk => "low_risk",
            Self::CoreCandidate => "core_candidate",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "not_recommended" => Some(Self::NotRecommended),
            "manual_only" => Some(Self::ManualOnly),
            "trial" => Some(Self::Trial),
            "low_risk" => Some(Self::LowRisk),
            "core_candidate" => Some(Self::CoreCandidate),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckResult {
    pub check_id: String,
    pub dimension: String,
    pub finding: Finding,
    pub rationale: String,
    pub evidence_ids: Vec<Uuid>,
    pub not_applicable_reason: Option<String>,
    pub weight: f64,
    pub high_risk: bool,
    pub scoring_rule_id: String,
    pub rule_points: f64,
    pub weighted_points: f64,
    pub applicable: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct DimensionScore {
    pub score: u8,
    pub earned_weighted_points: f64,
    pub applicable_weight: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct DimensionScores {
    pub capability_clarity: DimensionScore,
    pub interface_openness: DimensionScore,
    pub automation_readiness: DimensionScore,
    pub data_portability: DimensionScore,
    pub permission_risk: DimensionScore,
    pub evidence_quality: DimensionScore,
    pub ecosystem_fit: DimensionScore,
}

impl DimensionScores {
    pub fn get_mut(&mut self, dimension: &str) -> Option<&mut DimensionScore> {
        match dimension {
            "capability_clarity" => Some(&mut self.capability_clarity),
            "interface_openness" => Some(&mut self.interface_openness),
            "automation_readiness" => Some(&mut self.automation_readiness),
            "data_portability" => Some(&mut self.data_portability),
            "permission_risk" => Some(&mut self.permission_risk),
            "evidence_quality" => Some(&mut self.evidence_quality),
            "ecosystem_fit" => Some(&mut self.ecosystem_fit),
            _ => None,
        }
    }

    pub fn values(&self) -> [&DimensionScore; 7] {
        [
            &self.capability_clarity,
            &self.interface_openness,
            &self.automation_readiness,
            &self.data_portability,
            &self.permission_risk,
            &self.evidence_quality,
            &self.ecosystem_fit,
        ]
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckResults {
    pub check_results_schema_version: &'static str,
    pub check_results_id: Uuid,
    pub run_id: Uuid,
    pub evidence_board_version: u64,
    pub standard_id: String,
    pub standard_version: String,
    pub profile_id: String,
    pub profile_version: String,
    pub results: Vec<CheckResult>,
    pub dimension_scores: DimensionScores,
    pub total_score: u8,
    pub rating: Rating,
    pub computed_at: DateTime<Utc>,
}
