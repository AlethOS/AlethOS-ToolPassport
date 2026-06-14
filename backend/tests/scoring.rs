use std::collections::HashSet;

use chrono::{TimeZone, Utc};
use toolpassport_backend::{
    CheckResultsSubmission, Finding, FindingSubmission, Rating, ScoringError, ToolType,
    current_audit_binding, score_check_results,
};
use uuid::Uuid;

const GENERIC_CHECKS: [&str; 7] = [
    "generic.capability_scope",
    "generic.interface_contract",
    "generic.automation_contract",
    "generic.export_path",
    "generic.permission_boundary",
    "generic.claim_traceability",
    "generic.maintenance_signal",
];

#[test]
fn identical_inputs_produce_identical_aggregates_in_profile_order() {
    let submission = submission_with(Finding::Pass);
    let computed_at = Utc.with_ymd_and_hms(2026, 6, 13, 12, 0, 0).unwrap();

    let first = score_check_results(
        Uuid::nil(),
        &current_audit_binding(ToolType::Generic),
        submission.clone(),
        &HashSet::new(),
        &HashSet::new(),
        computed_at,
    )
    .unwrap();
    let second = score_check_results(
        Uuid::nil(),
        &current_audit_binding(ToolType::Generic),
        submission,
        &HashSet::new(),
        &HashSet::new(),
        computed_at,
    )
    .unwrap();

    assert_eq!(
        first
            .results
            .iter()
            .map(|result| result.check_id.as_str())
            .collect::<Vec<_>>(),
        GENERIC_CHECKS
    );
    assert_eq!(first.dimension_scores, second.dimension_scores);
    assert_eq!(first.standard_version, "0.3.0");
    assert_eq!(first.profile_version, "0.3.0");
    assert_eq!(first.total_score, 100);
    assert_eq!(first.total_score, second.total_score);
    assert_eq!(first.rating, Rating::CoreCandidate);
    assert_eq!(first.rating, second.rating);
}

#[test]
fn unknown_scores_zero_and_caps_high_risk_rating() {
    let mut submission = submission_with(Finding::Pass);
    let permission = find_mut(&mut submission, "generic.permission_boundary");
    permission.finding = Finding::Unknown;

    let result = score(submission, &HashSet::new(), &HashSet::new()).unwrap();
    let permission = result
        .results
        .iter()
        .find(|result| result.check_id == "generic.permission_boundary")
        .unwrap();

    assert_eq!(permission.rule_points, 0.0);
    assert_eq!(permission.weighted_points, 0.0);
    assert_eq!(result.dimension_scores.permission_risk.score, 0);
    assert_eq!(result.total_score, 85);
    assert_eq!(result.rating, Rating::ManualOnly);
}

#[test]
fn high_risk_partial_caps_otherwise_high_rating_to_trial() {
    let mut submission = submission_with(Finding::Pass);
    find_mut(&mut submission, "generic.permission_boundary").finding = Finding::Partial;

    let result = score(submission, &HashSet::new(), &HashSet::new()).unwrap();

    assert_eq!(result.total_score, 88);
    assert_eq!(result.rating, Rating::Trial);
}

#[test]
fn approved_not_applicable_is_removed_from_dimension_denominator() {
    let mut submission = submission_with(Finding::Pass);
    let permission = find_mut(&mut submission, "generic.permission_boundary");
    permission.finding = Finding::NotApplicable;
    permission.not_applicable_reason = Some("Permission check excluded by approved scope.".into());
    let approvals = HashSet::from(["generic.permission_boundary".to_owned()]);

    let result = score(submission, &HashSet::new(), &approvals).unwrap();
    let permission = result
        .results
        .iter()
        .find(|result| result.check_id == "generic.permission_boundary")
        .unwrap();

    assert!(!permission.applicable);
    assert_eq!(
        result.dimension_scores.permission_risk.applicable_weight,
        0.0
    );
    assert_eq!(result.dimension_scores.permission_risk.score, 0);
}

#[test]
fn not_applicable_requires_reason_and_trusted_approval() {
    let mut submission = submission_with(Finding::Pass);
    let permission = find_mut(&mut submission, "generic.permission_boundary");
    permission.finding = Finding::NotApplicable;

    assert_eq!(
        score(submission.clone(), &HashSet::new(), &HashSet::new()).unwrap_err(),
        ScoringError::MissingNotApplicableReason("generic.permission_boundary".into())
    );

    find_mut(&mut submission, "generic.permission_boundary").not_applicable_reason =
        Some("Approved scope exclusion".into());
    assert_eq!(
        score(submission, &HashSet::new(), &HashSet::new()).unwrap_err(),
        ScoringError::NotApplicableApprovalRequired("generic.permission_boundary".into())
    );
}

#[test]
fn duplicate_missing_and_unknown_checks_are_rejected() {
    let mut duplicate = submission_with(Finding::Pass);
    duplicate.findings.push(duplicate.findings[0].clone());
    assert_eq!(
        score(duplicate, &HashSet::new(), &HashSet::new()).unwrap_err(),
        ScoringError::DuplicateCheck("generic.capability_scope".into())
    );

    let mut missing = submission_with(Finding::Pass);
    missing.findings.pop();
    assert_eq!(
        score(missing, &HashSet::new(), &HashSet::new()).unwrap_err(),
        ScoringError::MissingCheck("generic.maintenance_signal".into())
    );

    let mut unknown = submission_with(Finding::Pass);
    unknown.findings[0].check_id = "generic.unversioned_check".into();
    assert_eq!(
        score(unknown, &HashSet::new(), &HashSet::new()).unwrap_err(),
        ScoringError::UnknownCheck("generic.unversioned_check".into())
    );
}

#[test]
fn evidence_references_must_be_unique_and_belong_to_the_run() {
    let evidence_id = Uuid::new_v4();
    let mut unknown = submission_with(Finding::Pass);
    unknown.findings[0].evidence_ids.push(evidence_id);
    assert_eq!(
        score(unknown, &HashSet::new(), &HashSet::new()).unwrap_err(),
        ScoringError::UnknownEvidenceId {
            check_id: "generic.capability_scope".into(),
            evidence_id,
        }
    );

    let mut duplicate = submission_with(Finding::Pass);
    duplicate.findings[0].evidence_ids = vec![evidence_id, evidence_id];
    assert_eq!(
        score(duplicate, &HashSet::from([evidence_id]), &HashSet::new()).unwrap_err(),
        ScoringError::DuplicateEvidenceId {
            check_id: "generic.capability_scope".into(),
            evidence_id,
        }
    );
}

#[test]
fn non_not_applicable_finding_cannot_supply_not_applicable_reason() {
    let mut submission = submission_with(Finding::Pass);
    submission.findings[0].not_applicable_reason = Some("not allowed".into());

    assert_eq!(
        score(submission, &HashSet::new(), &HashSet::new()).unwrap_err(),
        ScoringError::UnexpectedNotApplicableReason("generic.capability_scope".into())
    );
}

fn submission_with(finding: Finding) -> CheckResultsSubmission {
    CheckResultsSubmission {
        check_results_schema_version: "0.1.0".into(),
        evidence_board_version: 1,
        findings: GENERIC_CHECKS
            .iter()
            .map(|check_id| FindingSubmission {
                check_id: (*check_id).into(),
                finding,
                rationale: "Evidence-bound rationale.".into(),
                evidence_ids: Vec::new(),
                not_applicable_reason: None,
            })
            .collect(),
    }
}

fn find_mut<'a>(
    submission: &'a mut CheckResultsSubmission,
    check_id: &str,
) -> &'a mut FindingSubmission {
    submission
        .findings
        .iter_mut()
        .find(|finding| finding.check_id == check_id)
        .unwrap()
}

fn score(
    submission: CheckResultsSubmission,
    evidence_ids: &HashSet<Uuid>,
    approvals: &HashSet<String>,
) -> Result<toolpassport_backend::domain::CheckResults, ScoringError> {
    score_check_results(
        Uuid::nil(),
        &current_audit_binding(ToolType::Generic),
        submission,
        evidence_ids,
        approvals,
        Utc.with_ymd_and_hms(2026, 6, 13, 12, 0, 0).unwrap(),
    )
}
