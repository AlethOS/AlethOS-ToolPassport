use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use serde::Deserialize;
use thiserror::Error;
use uuid::Uuid;

use crate::domain::{
    CheckResult, CheckResults, CheckResultsSubmission, DimensionScores, Finding, Rating, ToolType,
};

const STANDARD_JSON: &str = include_str!("../../../standards/alethos-toolpassport/0.3.0.json");
const GENERIC_PROFILE_JSON: &str = include_str!("../../../profiles/generic/0.3.0.json");
const AGENT_FRAMEWORK_PROFILE_JSON: &str =
    include_str!("../../../profiles/agent_framework/0.3.0.json");
const MCP_SERVER_PROFILE_JSON: &str = include_str!("../../../profiles/mcp_server/0.3.0.json");
const CLI_API_TOOL_PROFILE_JSON: &str = include_str!("../../../profiles/cli_api_tool/0.3.0.json");

#[derive(Debug, Error, PartialEq)]
pub enum ScoringError {
    #[error("check_results_schema_version must be 0.1.0")]
    InvalidSchemaVersion,
    #[error("evidence_board_version must be at least 1")]
    InvalidEvidenceBoardVersion,
    #[error("finding for check {0} is duplicated")]
    DuplicateCheck(String),
    #[error("finding for unknown check {0}")]
    UnknownCheck(String),
    #[error("finding for required check {0} is missing")]
    MissingCheck(String),
    #[error("finding rationale for check {0} must not be empty")]
    EmptyRationale(String),
    #[error("evidence ID {evidence_id} is duplicated in check {check_id}")]
    DuplicateEvidenceId { check_id: String, evidence_id: Uuid },
    #[error("evidence ID {evidence_id} referenced by check {check_id} does not belong to the run")]
    UnknownEvidenceId { check_id: String, evidence_id: Uuid },
    #[error("not_applicable_reason is required for check {0}")]
    MissingNotApplicableReason(String),
    #[error("not_applicable_reason must be null unless check {0} is not_applicable")]
    UnexpectedNotApplicableReason(String),
    #[error("not_applicable finding for check {0} requires trusted approval")]
    NotApplicableApprovalRequired(String),
    #[error("embedded audit catalog is invalid: {0}")]
    InvalidCatalog(String),
}

#[derive(Debug, Deserialize)]
struct AuditStandard {
    standard_id: String,
    standard_version: String,
    dimensions: Vec<DimensionDefinition>,
    scoring_rules: Vec<ScoringRule>,
    rating_policy: RatingPolicy,
}

#[derive(Debug, Deserialize)]
struct DimensionDefinition {
    dimension_id: String,
}

#[derive(Debug, Deserialize)]
struct ScoringRule {
    scoring_rule_id: String,
    finding_points: FindingPoints,
    not_applicable_requires_approval: bool,
}

#[derive(Debug, Deserialize)]
struct FindingPoints {
    pass: f64,
    partial: f64,
    fail: f64,
    unknown: f64,
}

impl FindingPoints {
    const fn for_finding(&self, finding: Finding) -> Option<f64> {
        match finding {
            Finding::Pass => Some(self.pass),
            Finding::Partial => Some(self.partial),
            Finding::Fail => Some(self.fail),
            Finding::Unknown => Some(self.unknown),
            Finding::NotApplicable => None,
        }
    }
}

#[derive(Debug, Deserialize)]
struct RatingPolicy {
    thresholds: Vec<RatingThreshold>,
    high_risk_rating_caps: HighRiskRatingCaps,
}

#[derive(Debug, Deserialize)]
struct RatingThreshold {
    minimum_total_score: u8,
    rating: String,
}

#[derive(Debug, Deserialize)]
struct HighRiskRatingCaps {
    partial: String,
    fail: String,
    unknown: String,
}

impl HighRiskRatingCaps {
    fn for_finding(&self, finding: Finding) -> Option<&str> {
        match finding {
            Finding::Partial => Some(&self.partial),
            Finding::Fail => Some(&self.fail),
            Finding::Unknown => Some(&self.unknown),
            Finding::Pass | Finding::NotApplicable => None,
        }
    }
}

#[derive(Debug, Deserialize)]
struct AuditProfile {
    profile_id: String,
    profile_version: String,
    standard_id: String,
    standard_version: String,
    checks: Vec<ProfileCheck>,
}

#[derive(Debug, Deserialize)]
struct ProfileCheck {
    check_id: String,
    dimension: String,
    weight: f64,
    high_risk: bool,
    scoring_rule_id: String,
}

pub fn score_check_results(
    run_id: Uuid,
    tool_type: ToolType,
    submission: CheckResultsSubmission,
    available_evidence_ids: &HashSet<Uuid>,
    approved_not_applicable_check_ids: &HashSet<String>,
    computed_at: DateTime<Utc>,
) -> Result<CheckResults, ScoringError> {
    validate_submission_header(&submission)?;
    let (standard, profile) = load_catalog(tool_type)?;
    validate_catalog(&standard, &profile)?;

    let rules: HashMap<&str, &ScoringRule> = standard
        .scoring_rules
        .iter()
        .map(|rule| (rule.scoring_rule_id.as_str(), rule))
        .collect();
    let profile_checks: HashSet<&str> = profile
        .checks
        .iter()
        .map(|check| check.check_id.as_str())
        .collect();
    let mut findings = HashMap::new();

    for finding in submission.findings {
        let check_id = finding.check_id.clone();
        if !profile_checks.contains(check_id.as_str()) {
            return Err(ScoringError::UnknownCheck(check_id));
        }
        if findings.insert(check_id.clone(), finding).is_some() {
            return Err(ScoringError::DuplicateCheck(check_id));
        }
    }

    let mut results = Vec::with_capacity(profile.checks.len());
    let mut dimension_scores = DimensionScores::default();

    for check in &profile.checks {
        let finding = findings
            .remove(&check.check_id)
            .ok_or_else(|| ScoringError::MissingCheck(check.check_id.clone()))?;
        validate_finding(
            &finding,
            available_evidence_ids,
            approved_not_applicable_check_ids,
            rules
                .get(check.scoring_rule_id.as_str())
                .copied()
                .ok_or_else(|| {
                    ScoringError::InvalidCatalog(format!(
                        "check {} references unknown scoring rule {}",
                        check.check_id, check.scoring_rule_id
                    ))
                })?,
        )?;

        let rule = rules[check.scoring_rule_id.as_str()];
        let applicable = finding.finding != Finding::NotApplicable;
        let rule_points = rule
            .finding_points
            .for_finding(finding.finding)
            .unwrap_or(0.0);
        let weighted_points = if applicable {
            check.weight * rule_points
        } else {
            0.0
        };
        let dimension_score = dimension_scores.get_mut(&check.dimension).ok_or_else(|| {
            ScoringError::InvalidCatalog(format!(
                "check {} references unsupported dimension {}",
                check.check_id, check.dimension
            ))
        })?;
        if applicable {
            dimension_score.earned_weighted_points += weighted_points;
            dimension_score.applicable_weight += check.weight;
        }

        results.push(CheckResult {
            check_id: check.check_id.clone(),
            dimension: check.dimension.clone(),
            finding: finding.finding,
            rationale: finding.rationale,
            evidence_ids: finding.evidence_ids,
            not_applicable_reason: finding.not_applicable_reason,
            weight: check.weight,
            high_risk: check.high_risk,
            scoring_rule_id: check.scoring_rule_id.clone(),
            rule_points,
            weighted_points,
            applicable,
        });
    }

    finalize_dimension_scores(&mut dimension_scores);
    let total_score = ((dimension_scores
        .values()
        .iter()
        .map(|score| score.score as u16)
        .sum::<u16>()
        * 20)
        / 7) as u8;
    let mut rating = rating_for_total(&standard.rating_policy, total_score)?;
    for result in &results {
        if result.high_risk
            && let Some(cap) = standard
                .rating_policy
                .high_risk_rating_caps
                .for_finding(result.finding)
        {
            rating = rating.min(parse_rating(cap)?);
        }
    }

    Ok(CheckResults {
        check_results_schema_version: "0.1.0",
        check_results_id: Uuid::new_v4(),
        run_id,
        evidence_board_version: submission.evidence_board_version,
        standard_id: standard.standard_id,
        standard_version: standard.standard_version,
        profile_id: profile.profile_id,
        profile_version: profile.profile_version,
        results,
        dimension_scores,
        total_score,
        rating,
        computed_at,
    })
}

fn validate_submission_header(submission: &CheckResultsSubmission) -> Result<(), ScoringError> {
    if submission.check_results_schema_version != "0.1.0" {
        return Err(ScoringError::InvalidSchemaVersion);
    }
    if submission.evidence_board_version == 0 {
        return Err(ScoringError::InvalidEvidenceBoardVersion);
    }
    Ok(())
}

fn validate_finding(
    finding: &crate::domain::FindingSubmission,
    available_evidence_ids: &HashSet<Uuid>,
    approved_not_applicable_check_ids: &HashSet<String>,
    rule: &ScoringRule,
) -> Result<(), ScoringError> {
    if finding.rationale.trim().is_empty() {
        return Err(ScoringError::EmptyRationale(finding.check_id.clone()));
    }
    let mut seen = HashSet::new();
    for evidence_id in &finding.evidence_ids {
        if !seen.insert(*evidence_id) {
            return Err(ScoringError::DuplicateEvidenceId {
                check_id: finding.check_id.clone(),
                evidence_id: *evidence_id,
            });
        }
        if !available_evidence_ids.contains(evidence_id) {
            return Err(ScoringError::UnknownEvidenceId {
                check_id: finding.check_id.clone(),
                evidence_id: *evidence_id,
            });
        }
    }

    if finding.finding == Finding::NotApplicable {
        if finding
            .not_applicable_reason
            .as_deref()
            .is_none_or(|reason| reason.trim().is_empty())
        {
            return Err(ScoringError::MissingNotApplicableReason(
                finding.check_id.clone(),
            ));
        }
        if rule.not_applicable_requires_approval
            && !approved_not_applicable_check_ids.contains(&finding.check_id)
        {
            return Err(ScoringError::NotApplicableApprovalRequired(
                finding.check_id.clone(),
            ));
        }
    } else if finding.not_applicable_reason.is_some() {
        return Err(ScoringError::UnexpectedNotApplicableReason(
            finding.check_id.clone(),
        ));
    }
    Ok(())
}

fn finalize_dimension_scores(scores: &mut DimensionScores) {
    for dimension in [
        &mut scores.capability_clarity,
        &mut scores.interface_openness,
        &mut scores.automation_readiness,
        &mut scores.data_portability,
        &mut scores.permission_risk,
        &mut scores.evidence_quality,
        &mut scores.ecosystem_fit,
    ] {
        dimension.score = if dimension.applicable_weight == 0.0 {
            0
        } else {
            (5.0 * dimension.earned_weighted_points / dimension.applicable_weight).floor() as u8
        };
    }
}

fn rating_for_total(policy: &RatingPolicy, total_score: u8) -> Result<Rating, ScoringError> {
    policy
        .thresholds
        .iter()
        .rfind(|threshold| threshold.minimum_total_score <= total_score)
        .ok_or_else(|| ScoringError::InvalidCatalog("rating thresholds must start at zero".into()))
        .and_then(|threshold| parse_rating(&threshold.rating))
}

fn parse_rating(value: &str) -> Result<Rating, ScoringError> {
    Rating::parse(value)
        .ok_or_else(|| ScoringError::InvalidCatalog(format!("unknown rating {value}")))
}

fn load_catalog(tool_type: ToolType) -> Result<(AuditStandard, AuditProfile), ScoringError> {
    let profile_json = match tool_type {
        ToolType::Generic => GENERIC_PROFILE_JSON,
        ToolType::AgentFramework => AGENT_FRAMEWORK_PROFILE_JSON,
        ToolType::McpServer => MCP_SERVER_PROFILE_JSON,
        ToolType::CliApiTool => CLI_API_TOOL_PROFILE_JSON,
    };
    let standard = serde_json::from_str(STANDARD_JSON)
        .map_err(|error| ScoringError::InvalidCatalog(error.to_string()))?;
    let profile = serde_json::from_str(profile_json)
        .map_err(|error| ScoringError::InvalidCatalog(error.to_string()))?;
    Ok((standard, profile))
}

fn validate_catalog(standard: &AuditStandard, profile: &AuditProfile) -> Result<(), ScoringError> {
    if profile.standard_id != standard.standard_id
        || profile.standard_version != standard.standard_version
    {
        return Err(ScoringError::InvalidCatalog(
            "profile does not bind the embedded standard version".into(),
        ));
    }
    let dimensions: HashSet<&str> = standard
        .dimensions
        .iter()
        .map(|dimension| dimension.dimension_id.as_str())
        .collect();
    let rules: HashSet<&str> = standard
        .scoring_rules
        .iter()
        .map(|rule| rule.scoring_rule_id.as_str())
        .collect();
    for check in &profile.checks {
        if !dimensions.contains(check.dimension.as_str()) {
            return Err(ScoringError::InvalidCatalog(format!(
                "check {} references unknown dimension {}",
                check.check_id, check.dimension
            )));
        }
        if !rules.contains(check.scoring_rule_id.as_str()) {
            return Err(ScoringError::InvalidCatalog(format!(
                "check {} references unknown scoring rule {}",
                check.check_id, check.scoring_rule_id
            )));
        }
    }
    for window in standard.rating_policy.thresholds.windows(2) {
        if window[0].minimum_total_score >= window[1].minimum_total_score {
            return Err(ScoringError::InvalidCatalog(
                "rating thresholds must be strictly ascending".into(),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{load_catalog, validate_catalog};
    use crate::domain::ToolType;

    #[test]
    fn every_embedded_profile_binds_the_embedded_standard() {
        for tool_type in [
            ToolType::Generic,
            ToolType::AgentFramework,
            ToolType::McpServer,
            ToolType::CliApiTool,
        ] {
            let (standard, profile) = load_catalog(tool_type).unwrap();
            validate_catalog(&standard, &profile).unwrap();
        }
    }
}
