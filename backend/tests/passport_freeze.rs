use std::{
    path::{Path, PathBuf},
    str::FromStr,
};

use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode, header::CONTENT_TYPE},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};
use toolpassport_backend::{StorageService, app_with_storage, canonical_sha256, migrate};
use tower::ServiceExt;
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

struct TestApp {
    router: Router,
    pool: SqlitePool,
    _root: TestDirectory,
}

struct TestDirectory {
    path: PathBuf,
}

impl TestDirectory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("toolpassport-passport-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

#[tokio::test]
async fn migration_creates_immutable_passport_and_provenance_tables() {
    let app = test_app().await;
    let objects: Vec<String> = sqlx::query_scalar(
        r#"
        SELECT name FROM sqlite_master
        WHERE name IN (
            'passports',
            'provenances',
            'passports_prevent_update',
            'passports_prevent_delete',
            'provenances_prevent_update',
            'provenances_prevent_delete'
        )
        ORDER BY name
        "#,
    )
    .fetch_all(&app.pool)
    .await
    .unwrap();

    assert_eq!(objects.len(), 6);
}

#[tokio::test]
async fn api_freezes_passport_with_stable_hashes_and_event() {
    let app = test_app().await;
    let run_id = create_run(&app.router).await;
    let evidence_id = create_evidence(&app.router, &run_id).await;
    assert_eq!(
        freeze_board(&app.router, &run_id, vec![json!(evidence_id)])
            .await
            .0,
        StatusCode::CREATED
    );
    assert_eq!(
        post_results(&app.router, &run_id, &evidence_id).await.0,
        StatusCode::CREATED
    );

    let response = freeze_passport(&app.router, &run_id, &evidence_id, None).await;
    assert_eq!(response.0, StatusCode::CREATED);

    let passport = &response.1["passport"];
    let provenance = &response.1["provenance"];
    assert_eq!(passport["passport_version"], "0.2.0");
    assert_eq!(passport["passport_sequence"], 1);
    assert_eq!(passport["run_id"], run_id);
    assert_eq!(passport["tool_type"], "generic");
    assert_eq!(passport["standard_version"], "0.3.0");
    assert_eq!(passport["profile_version"], "0.3.0");
    assert!(passport["scores"]["dimensions"]["capability_clarity"].is_u64());
    assert!(passport["scores"]["total_score"].is_u64());

    // The four commitment hashes are all well-formed and distinct.
    for field in [
        "passport_hash",
        "audit_log_hash",
        "evidence_manifest_hash",
        "onchain_run_id",
    ] {
        let hash = provenance[field].as_str().unwrap();
        assert!(hash.starts_with("0x"), "{field} must be 0x-prefixed");
        assert_eq!(hash.len(), 66, "{field} must be 64 hex chars");
        assert!(hash[2..].chars().all(|c| c.is_ascii_hexdigit()));
    }

    // passportHash is a pure, reproducible function of the Passport document.
    let recomputed = canonical_sha256(passport);
    assert_eq!(recomputed, provenance["passport_hash"].as_str().unwrap());

    // auditLogHash is the event_hash of the appended provenance_frozen event.
    let details = send_json(
        &app.router,
        Method::GET,
        &format!("/api/runs/{run_id}"),
        json!({}),
    )
    .await;
    let freeze_event = details.1["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|event| event["event_type"] == "provenance_frozen")
        .expect("provenance_frozen event must be appended");
    assert_eq!(
        freeze_event["event_hash"],
        provenance["audit_log_hash"].as_str().unwrap()
    );
    // The event payload carries passport_hash but never the not-yet-computed
    // audit_log_hash (anti-circular-commitment).
    assert_eq!(
        freeze_event["payload"]["passport_hash"],
        provenance["passport_hash"]
    );
    assert!(freeze_event["payload"].get("audit_log_hash").is_none());
}

#[tokio::test]
async fn approval_api_binds_latest_provenance_and_cannot_be_forged() {
    let app = test_app().await;
    let run_id = create_run(&app.router).await;
    let evidence_id = create_evidence(&app.router, &run_id).await;
    assert_eq!(
        freeze_board(&app.router, &run_id, vec![json!(evidence_id)])
            .await
            .0,
        StatusCode::CREATED
    );
    assert_eq!(
        post_results(&app.router, &run_id, &evidence_id).await.0,
        StatusCode::CREATED
    );
    let freeze = freeze_passport(&app.router, &run_id, &evidence_id, None).await;
    assert_eq!(freeze.0, StatusCode::CREATED);

    for event_type in ["node_started", "node_finished", "approval_required"] {
        let event = send_json(
            &app.router,
            Method::POST,
            &format!("/api/runs/{run_id}/events"),
            json!({"node_id": "human_review_gate", "event_type": event_type, "payload": {}}),
        )
        .await;
        assert_eq!(event.0, StatusCode::CREATED);
    }

    let provenance = &freeze.1["provenance"];
    let request = json!({
        "approval_schema_version": "0.1.0",
        "decision": "approve_testnet_attestation",
        "passport_sequence": 1,
        "passport_hash": provenance["passport_hash"],
        "audit_log_hash": provenance["audit_log_hash"],
        "evidence_manifest_hash": provenance["evidence_manifest_hash"],
        "chain_id": 11155111,
        "registry_contract": "0x1111111111111111111111111111111111111111"
    });
    let approved = send_json(
        &app.router,
        Method::POST,
        &format!("/api/runs/{run_id}/approval"),
        request.clone(),
    )
    .await;
    assert_eq!(approved.0, StatusCode::CREATED);
    assert_eq!(approved.1["decision"], "approve_testnet_attestation");

    let fetched = send_json(
        &app.router,
        Method::GET,
        &format!("/api/runs/{run_id}/approval"),
        json!({}),
    )
    .await;
    assert_eq!(fetched.0, StatusCode::OK);
    assert_eq!(fetched.1["passport_hash"], provenance["passport_hash"]);

    let duplicate = send_json(
        &app.router,
        Method::POST,
        &format!("/api/runs/{run_id}/approval"),
        request,
    )
    .await;
    assert_eq!(duplicate.0, StatusCode::CONFLICT);

    let forged = send_json(
        &app.router,
        Method::POST,
        &format!("/api/runs/{run_id}/events"),
        json!({"node_id": "forged", "event_type": "approval_resolved", "payload": {}}),
    )
    .await;
    assert_eq!(forged.0, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn get_passport_returns_the_frozen_result() {
    let app = test_app().await;
    let run_id = create_run(&app.router).await;
    let evidence_id = create_evidence(&app.router, &run_id).await;
    freeze_board(&app.router, &run_id, vec![json!(evidence_id)]).await;
    post_results(&app.router, &run_id, &evidence_id).await;
    freeze_passport(&app.router, &run_id, &evidence_id, None).await;

    let fetched = send_json(
        &app.router,
        Method::GET,
        &format!("/api/runs/{run_id}/passport/1"),
        json!({}),
    )
    .await;
    assert_eq!(fetched.0, StatusCode::OK);
    assert_eq!(fetched.1["passport"]["passport_sequence"], 1);
    assert_eq!(
        fetched.1["provenance"]["passport_hash"],
        fetched.1["provenance"]["passport_hash"]
    );

    let missing = send_json(
        &app.router,
        Method::GET,
        &format!("/api/runs/{run_id}/passport/99"),
        json!({}),
    )
    .await;
    assert_error(&missing, StatusCode::NOT_FOUND, "passport_not_found");
}

#[tokio::test]
async fn second_freeze_advances_sequence() {
    let app = test_app().await;
    let run_id = create_run(&app.router).await;
    let evidence_id = create_evidence(&app.router, &run_id).await;
    freeze_board(&app.router, &run_id, vec![json!(evidence_id)]).await;
    post_results(&app.router, &run_id, &evidence_id).await;

    let first = freeze_passport(&app.router, &run_id, &evidence_id, None).await;
    assert_eq!(first.1["passport"]["passport_sequence"], 1);
    let second = freeze_passport(&app.router, &run_id, &evidence_id, None).await;
    assert_eq!(second.0, StatusCode::CREATED);
    assert_eq!(second.1["passport"]["passport_sequence"], 2);
    assert_ne!(
        first.1["provenance"]["passport_hash"],
        second.1["provenance"]["passport_hash"]
    );
}

#[tokio::test]
async fn api_requires_frozen_board_and_check_results() {
    let app = test_app().await;
    let run_id = create_run(&app.router).await;
    let evidence_id = create_evidence(&app.router, &run_id).await;

    let before_board = freeze_passport(&app.router, &run_id, &evidence_id, None).await;
    assert_error(&before_board, StatusCode::BAD_REQUEST, "invalid_request");
    assert!(
        before_board.1["message"]
            .as_str()
            .unwrap()
            .contains("is not frozen")
    );

    freeze_board(&app.router, &run_id, vec![json!(evidence_id)]).await;
    let before_results = freeze_passport(&app.router, &run_id, &evidence_id, None).await;
    assert_error(&before_results, StatusCode::BAD_REQUEST, "invalid_request");
    assert!(
        before_results.1["message"]
            .as_str()
            .unwrap()
            .contains("are not computed")
    );
}

#[tokio::test]
async fn api_rejects_evidence_outside_the_frozen_board() {
    let app = test_app().await;
    let run_id = create_run(&app.router).await;
    let evidence_id = create_evidence(&app.router, &run_id).await;
    let other_evidence = Uuid::new_v4();
    freeze_board(&app.router, &run_id, vec![json!(evidence_id)]).await;
    post_results(&app.router, &run_id, &evidence_id).await;

    let rejected = freeze_passport(&app.router, &run_id, &evidence_id, Some(other_evidence)).await;
    assert_error(&rejected, StatusCode::BAD_REQUEST, "invalid_request");
    assert!(
        rejected.1["message"]
            .as_str()
            .unwrap()
            .contains("outside the board")
    );
}

#[tokio::test]
async fn api_rejects_invalid_and_duplicate_statement_ids() {
    let app = test_app().await;
    let run_id = create_run(&app.router).await;
    let evidence_id = create_evidence(&app.router, &run_id).await;
    freeze_board(&app.router, &run_id, vec![json!(evidence_id)]).await;
    post_results(&app.router, &run_id, &evidence_id).await;

    let mut invalid = passport_request(&evidence_id);
    invalid["capability_claims"][0]["statement_id"] = json!("Bad-ID");
    let rejected = freeze(&app.router, &run_id, invalid).await;
    assert_error(&rejected, StatusCode::BAD_REQUEST, "invalid_request");

    let mut duplicate = passport_request(&evidence_id);
    let cloned_claim = duplicate["capability_claims"][0].clone();
    duplicate["capability_claims"]
        .as_array_mut()
        .unwrap()
        .push(cloned_claim);
    let rejected = freeze(&app.router, &run_id, duplicate).await;
    assert_error(&rejected, StatusCode::BAD_REQUEST, "invalid_request");
}

#[tokio::test]
async fn forged_provenance_frozen_event_is_rejected() {
    let app = test_app().await;
    let run_id = create_run(&app.router).await;
    let forged = send_json(
        &app.router,
        Method::POST,
        &format!("/api/runs/{run_id}/events"),
        json!({
            "node_id": "orchestrator",
            "event_type": "provenance_frozen",
            "payload": {"passport_sequence": 1}
        }),
    )
    .await;
    assert_error(&forged, StatusCode::BAD_REQUEST, "invalid_request");
}

#[tokio::test]
async fn passport_and_provenance_rows_are_immutable() {
    let app = test_app().await;
    let run_id = create_run(&app.router).await;
    let evidence_id = create_evidence(&app.router, &run_id).await;
    freeze_board(&app.router, &run_id, vec![json!(evidence_id)]).await;
    post_results(&app.router, &run_id, &evidence_id).await;
    freeze_passport(&app.router, &run_id, &evidence_id, None).await;

    let update = sqlx::query("UPDATE passports SET frozen_at = '1999-01-01T00:00:00Z'")
        .execute(&app.pool)
        .await;
    assert!(update.is_err());
    let delete = sqlx::query("DELETE FROM provenances")
        .execute(&app.pool)
        .await;
    assert!(delete.is_err());
}

async fn test_app() -> TestApp {
    let options = SqliteConnectOptions::from_str("sqlite::memory:")
        .unwrap()
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    let root = TestDirectory::new();
    let router = app_with_storage(pool.clone(), StorageService::new(root.path(), 1024 * 1024));
    TestApp {
        router,
        pool,
        _root: root,
    }
}

async fn create_run(router: &Router) -> String {
    let tool_id = format!("github:example/passport-{}", Uuid::new_v4());
    let repo = tool_id.strip_prefix("github:").unwrap();
    let tool = send_json(
        router,
        Method::POST,
        "/api/tools",
        json!({
            "tool_id": tool_id,
            "name": "Passport Tool",
            "tool_type": "generic",
            "canonical_url": format!("https://github.com/{repo}"),
            "external_identifiers": [{
                "namespace": "github",
                "value": repo,
                "canonical_url": format!("https://github.com/{repo}")
            }],
            "aliases": []
        }),
    )
    .await;
    assert_eq!(tool.0, StatusCode::CREATED);

    let run = send_json(
        router,
        Method::POST,
        "/api/runs",
        json!({"goal": "Freeze passport", "tool_id": tool_id}),
    )
    .await;
    assert_eq!(run.0, StatusCode::CREATED);
    run.1["run_id"].as_str().unwrap().to_owned()
}

async fn create_evidence(router: &Router, run_id: &str) -> String {
    let response = send_json(
        router,
        Method::POST,
        &format!("/api/runs/{run_id}/evidence"),
        json!({
            "evidence_schema_version": "0.2.0",
            "source_type": "official_docs",
            "source_url": "https://example.com/docs",
            "source_revision": null,
            "title": "Docs",
            "excerpt": "Evidence",
            "retrieved_at": "2026-06-13T12:00:00Z",
            "snapshot_artifact_id": null,
            "supports": [],
            "contradicts": [],
            "metadata": {}
        }),
    )
    .await;
    assert_eq!(response.0, StatusCode::CREATED);
    response.1["evidence_id"].as_str().unwrap().to_owned()
}

async fn freeze_board(
    router: &Router,
    run_id: &str,
    evidence_ids: Vec<Value>,
) -> (StatusCode, Value) {
    send_json(
        router,
        Method::POST,
        &format!("/api/runs/{run_id}/evidence-board/freeze"),
        json!({
            "evidence_board_schema_version": "0.1.0",
            "version": 1,
            "evidence_ids": evidence_ids,
            "claims": [],
            "gaps": [],
            "freeze_reason": "Ready for scoring."
        }),
    )
    .await
}

async fn post_results(router: &Router, run_id: &str, evidence_id: &str) -> (StatusCode, Value) {
    send_json(
        router,
        Method::POST,
        &format!("/api/runs/{run_id}/check-results"),
        all_findings("pass", evidence_id),
    )
    .await
}

fn all_findings(finding: &str, evidence_id: &str) -> Value {
    let evidence_ids = vec![json!(evidence_id)];
    json!({
        "check_results_schema_version": "0.1.0",
        "evidence_board_version": 1,
        "findings": GENERIC_CHECKS.map(|check_id| json!({
            "check_id": check_id,
            "finding": finding,
            "rationale": "Evidence-bound rationale.",
            "evidence_ids": evidence_ids,
            "not_applicable_reason": null
        }))
    })
}

fn passport_request(evidence_id: &str) -> Value {
    json!({
        "passport_version": "0.2.0",
        "evidence_board_version": 1,
        "target_revision": "commit:abc123",
        "audit_scope": "Mock audit of documented behavior.",
        "capability_claims": [{
            "statement_id": "claim.structured_io",
            "statement": "The tool exposes structured output.",
            "status": "supported",
            "evidence_ids": [evidence_id]
        }],
        "interfaces": [],
        "risks": [],
        "known_gaps": ["A second independent example is missing."],
        "recommendation": {
            "summary": "Use after validating recovery behavior.",
            "conditions": ["Validate recovery behavior."]
        }
    })
}

async fn freeze_passport(
    router: &Router,
    run_id: &str,
    evidence_id: &str,
    outside_evidence: Option<Uuid>,
) -> (StatusCode, Value) {
    let mut payload = passport_request(evidence_id);
    if let Some(outside) = outside_evidence {
        payload["capability_claims"][0]["evidence_ids"] = json!([outside]);
    }
    freeze(router, run_id, payload).await
}

async fn freeze(router: &Router, run_id: &str, payload: Value) -> (StatusCode, Value) {
    send_json(
        router,
        Method::POST,
        &format!("/api/runs/{run_id}/passport/freeze"),
        payload,
    )
    .await
}

async fn send_json(
    router: &Router,
    method: Method,
    uri: &str,
    payload: Value,
) -> (StatusCode, Value) {
    let body = Body::from(serde_json::to_vec(&payload).unwrap());
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .header(CONTENT_TYPE, "application/json")
                .body(body)
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&body).unwrap())
}

fn assert_error(response: &(StatusCode, Value), status: StatusCode, code: &str) {
    assert_eq!(response.0, status);
    assert_eq!(response.1["code"], code);
}
