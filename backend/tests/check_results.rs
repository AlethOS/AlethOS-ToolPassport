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
use toolpassport_backend::{StorageService, app_with_storage, migrate};
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
        let path =
            std::env::temp_dir().join(format!("toolpassport-check-results-{}", Uuid::new_v4()));
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
async fn migration_creates_immutable_check_results_and_run_binding() {
    let app = test_app().await;
    let objects: Vec<String> = sqlx::query_scalar(
        r#"
        SELECT name
        FROM sqlite_master
        WHERE name IN (
            'check_results',
            'check_results_run_computed_idx',
            'check_results_prevent_update',
            'check_results_prevent_delete',
            'runs_prevent_audit_binding_update'
        )
        ORDER BY name
        "#,
    )
    .fetch_all(&app.pool)
    .await
    .unwrap();

    assert_eq!(
        objects,
        [
            "check_results",
            "check_results_prevent_delete",
            "check_results_prevent_update",
            "check_results_run_computed_idx",
            "runs_prevent_audit_binding_update"
        ]
    );

    let run_id = create_run(&app.router).await;
    let binding: (String, String, String, String) = sqlx::query_as(
        "SELECT standard_id, standard_version, profile_id, profile_version FROM runs WHERE run_id = ?",
    )
    .bind(&run_id)
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert_eq!(
        binding,
        (
            "alethos-toolpassport".into(),
            "0.3.0".into(),
            "generic".into(),
            "0.3.0".into()
        )
    );

    let binding_update = sqlx::query("UPDATE runs SET profile_version = '9.9.9' WHERE run_id = ?")
        .bind(&run_id)
        .execute(&app.pool)
        .await
        .unwrap_err();
    assert!(
        binding_update
            .to_string()
            .contains("audit binding is immutable")
    );
}

#[tokio::test]
async fn api_scores_persists_and_events_results_atomically() {
    let app = test_app().await;
    let run_id = create_run(&app.router).await;

    let response = post_results(&app.router, &run_id, all_findings("pass", None)).await;

    assert_eq!(response.0, StatusCode::CREATED);
    assert_eq!(response.1["run_id"], run_id);
    assert_eq!(response.1["standard_version"], "0.3.0");
    assert_eq!(response.1["profile_version"], "0.3.0");
    assert_eq!(response.1["total_score"], 100);
    assert_eq!(response.1["rating"], "core_candidate");

    let stored: String =
        sqlx::query_scalar("SELECT result_json FROM check_results WHERE run_id = ?")
            .bind(&run_id)
            .fetch_one(&app.pool)
            .await
            .unwrap();
    let stored: Value = serde_json::from_str(&stored).unwrap();
    assert_eq!(stored, response.1);

    let event_payload: String = sqlx::query_scalar(
        "SELECT payload FROM run_events WHERE run_id = ? AND event_type = 'score_changed'",
    )
    .bind(&run_id)
    .fetch_one(&app.pool)
    .await
    .unwrap();
    let event_payload: Value = serde_json::from_str(&event_payload).unwrap();
    assert_eq!(
        event_payload["check_results_id"],
        response.1["check_results_id"]
    );
    assert_eq!(event_payload["total_score"], 100);
}

#[tokio::test]
async fn api_requires_frozen_board_before_scoring() {
    let app = test_app().await;
    let run_id = create_run(&app.router).await;
    let submission = all_findings("pass", None);

    let before_freeze = send_json(
        &app.router,
        Method::POST,
        &format!("/api/runs/{run_id}/check-results"),
        submission.clone(),
    )
    .await;
    assert_error(&before_freeze, StatusCode::BAD_REQUEST, "invalid_request");
    assert!(
        before_freeze.1["message"]
            .as_str()
            .unwrap()
            .contains("is not frozen")
    );

    assert_eq!(
        freeze_board(&app.router, &run_id, Vec::new()).await.0,
        StatusCode::CREATED
    );
    let after_freeze = send_json(
        &app.router,
        Method::POST,
        &format!("/api/runs/{run_id}/check-results"),
        submission,
    )
    .await;
    assert_eq!(after_freeze.0, StatusCode::CREATED);
}

#[tokio::test]
async fn duplicate_board_version_is_rejected_without_second_event() {
    let app = test_app().await;
    let run_id = create_run(&app.router).await;
    assert_eq!(
        post_results(&app.router, &run_id, all_findings("pass", None))
            .await
            .0,
        StatusCode::CREATED
    );

    let duplicate = post_results(&app.router, &run_id, all_findings("fail", None)).await;

    assert_error(
        &duplicate,
        StatusCode::CONFLICT,
        "check_results_already_exist",
    );
    let event_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM run_events WHERE run_id = ? AND event_type = 'score_changed'",
    )
    .bind(&run_id)
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert_eq!(event_count, 1);
}

#[tokio::test]
async fn event_failure_rolls_back_check_results_insert() {
    let app = test_app().await;
    let run_id = create_run(&app.router).await;
    sqlx::query(
        r#"
        CREATE TRIGGER reject_score_changed_event
        BEFORE INSERT ON run_events
        WHEN NEW.event_type = 'score_changed'
        BEGIN
            SELECT RAISE(ABORT, 'forced score event failure');
        END
        "#,
    )
    .execute(&app.pool)
    .await
    .unwrap();

    let response = post_results(&app.router, &run_id, all_findings("pass", None)).await;

    assert_error(
        &response,
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal_error",
    );
    let result_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM check_results WHERE run_id = ?")
            .bind(&run_id)
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert_eq!(result_count, 0);
}

#[tokio::test]
async fn persisted_check_results_cannot_be_updated_or_deleted() {
    let app = test_app().await;
    let run_id = create_run(&app.router).await;
    let created = post_results(&app.router, &run_id, all_findings("pass", None)).await;
    let check_results_id = created.1["check_results_id"].as_str().unwrap();

    let update = sqlx::query("UPDATE check_results SET total_score = 0 WHERE check_results_id = ?")
        .bind(check_results_id)
        .execute(&app.pool)
        .await
        .unwrap_err();
    assert!(update.to_string().contains("immutable"));

    let delete = sqlx::query("DELETE FROM check_results WHERE check_results_id = ?")
        .bind(check_results_id)
        .execute(&app.pool)
        .await
        .unwrap_err();
    assert!(delete.to_string().contains("immutable"));
}

#[tokio::test]
async fn api_rejects_missing_unknown_and_approval_required_not_applicable() {
    let app = test_app().await;
    let run_id = create_run(&app.router).await;

    let mut missing = all_findings("pass", None);
    missing["findings"].as_array_mut().unwrap().pop();
    assert_error(
        &post_results(&app.router, &run_id, missing).await,
        StatusCode::BAD_REQUEST,
        "invalid_request",
    );

    let run_id = create_run(&app.router).await;
    let mut unknown = all_findings("pass", None);
    unknown["findings"][0]["check_id"] = json!("generic.unversioned");
    assert_error(
        &post_results(&app.router, &run_id, unknown).await,
        StatusCode::BAD_REQUEST,
        "invalid_request",
    );

    let run_id = create_run(&app.router).await;
    let mut not_applicable = all_findings("pass", None);
    not_applicable["findings"][4]["finding"] = json!("not_applicable");
    not_applicable["findings"][4]["not_applicable_reason"] = json!("Scope exclusion");
    let response = post_results(&app.router, &run_id, not_applicable).await;
    assert_error(&response, StatusCode::BAD_REQUEST, "invalid_request");
    assert!(
        response.1["message"]
            .as_str()
            .unwrap()
            .contains("requires trusted approval")
    );

    let run_id = create_run(&app.router).await;
    let mut caller_owned_total = all_findings("pass", None);
    caller_owned_total["total_score"] = json!(100);
    assert_error(
        &post_results(&app.router, &run_id, caller_owned_total).await,
        StatusCode::BAD_REQUEST,
        "invalid_json",
    );
}

#[tokio::test]
async fn api_only_accepts_evidence_from_the_same_run() {
    let app = test_app().await;
    let first_run = create_run(&app.router).await;
    let second_run = create_run(&app.router).await;
    let evidence_id = create_evidence(&app.router, &first_run).await;

    let accepted = post_results(
        &app.router,
        &first_run,
        all_findings("pass", Some(&evidence_id)),
    )
    .await;
    assert_eq!(accepted.0, StatusCode::CREATED);

    let rejected = post_results(
        &app.router,
        &second_run,
        all_findings("pass", Some(&evidence_id)),
    )
    .await;
    assert_error(&rejected, StatusCode::BAD_REQUEST, "invalid_request");
}

#[tokio::test]
async fn api_get_latest_check_results_returns_persisted_results() {
    let app = test_app().await;
    let run_id = create_run(&app.router).await;
    let evidence_id = create_evidence(&app.router, &run_id).await;
    freeze_board(&app.router, &run_id, vec![json!(evidence_id)]).await;

    // No check results yet — GET returns 404.
    let (get_status, _get_body) = send_json(
        &app.router,
        Method::GET,
        &format!("/api/runs/{run_id}/check-results"),
        json!({}),
    )
    .await;
    assert_eq!(get_status, StatusCode::NOT_FOUND);

    // Create check results.
    let (create_status, _) = post_results(
        &app.router,
        &run_id,
        all_findings("pass", Some(&evidence_id)),
    )
    .await;
    assert_eq!(create_status, StatusCode::CREATED);

    // Now GET returns the check results.
    let (get_status, get_body) = send_json(
        &app.router,
        Method::GET,
        &format!("/api/runs/{run_id}/check-results"),
        json!({}),
    )
    .await;
    assert_eq!(get_status, StatusCode::OK);
    assert_eq!(get_body["check_results_id"].as_str().unwrap().len(), 36); // UUID
    assert!(get_body["total_score"].as_u64().is_some());
    assert!(get_body["rating"].as_str().is_some_and(|r| !r.is_empty()));
    assert_eq!(
        get_body["results"].as_array().unwrap().len(),
        7 // generic@0.3.0 has 7 checks
    );
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
    let tool_id = format!("github:example/tool-{}", Uuid::new_v4());
    let repo = tool_id.strip_prefix("github:").unwrap();
    let tool = send_json(
        router,
        Method::POST,
        "/api/tools",
        json!({
            "tool_id": tool_id,
            "name": "Example Tool",
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
        json!({"goal": "Audit the tool", "tool_id": tool_id}),
    )
    .await;
    assert_eq!(run.0, StatusCode::CREATED);
    assert_eq!(run.1["audit_binding"]["profile_version"], "0.3.0");
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

fn all_findings(finding: &str, evidence_id: Option<&str>) -> Value {
    let evidence_ids = evidence_id.map_or_else(Vec::new, |id| vec![json!(id)]);
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

async fn post_results(router: &Router, run_id: &str, payload: Value) -> (StatusCode, Value) {
    let mut evidence_ids: Vec<Value> = payload["findings"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|finding| finding["evidence_ids"].as_array().into_iter().flatten())
        .cloned()
        .collect();
    evidence_ids.sort_by_key(Value::to_string);
    evidence_ids.dedup();
    let _ = freeze_board(router, run_id, evidence_ids).await;

    send_json(
        router,
        Method::POST,
        &format!("/api/runs/{run_id}/check-results"),
        payload,
    )
    .await
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
            "freeze_reason": "Freeze scoring test evidence."
        }),
    )
    .await
}

async fn send_json(
    router: &Router,
    method: Method,
    uri: &str,
    payload: Value,
) -> (StatusCode, Value) {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_vec(&payload).unwrap()))
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
