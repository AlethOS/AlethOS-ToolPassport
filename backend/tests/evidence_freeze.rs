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
        let path = std::env::temp_dir().join(format!("toolpassport-freeze-{}", Uuid::new_v4()));
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
async fn migration_creates_immutable_frozen_evidence_tables() {
    let app = test_app().await;
    let objects: Vec<String> = sqlx::query_scalar(
        r#"
        SELECT name FROM sqlite_master
        WHERE name IN (
            'evidence_boards',
            'evidence_manifests',
            'evidence_boards_prevent_update',
            'evidence_boards_prevent_delete',
            'evidence_manifests_prevent_update',
            'evidence_manifests_prevent_delete'
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
async fn api_freezes_normalized_board_manifest_and_event() {
    let app = test_app().await;
    let run_id = create_run(&app.router).await;
    let artifact_id = Uuid::new_v4();
    let artifact_hash = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    sqlx::query(
        r#"
        INSERT INTO artifacts (
            artifact_id, run_id, filename, content_type, size_bytes, sha256_hash,
            storage_key, created_at
        ) VALUES (?, ?, 'snapshot.html', 'text/html', 1, ?, 'test/snapshot', '2026-06-13T12:00:00Z')
        "#,
    )
    .bind(artifact_id.to_string())
    .bind(&run_id)
    .bind(artifact_hash)
    .execute(&app.pool)
    .await
    .unwrap();
    let first =
        create_evidence_with_snapshot(&app.router, &run_id, "First", Some(artifact_id)).await;
    let second = create_evidence(&app.router, &run_id, "Second").await;

    let response = freeze(
        &app.router,
        &run_id,
        json!({
            "evidence_board_schema_version": "0.1.0",
            "version": 1,
            "evidence_ids": [second["evidence_id"], first["evidence_id"]],
            "claims": [{
                "claim_id": "claim.interface",
                "check_id": "generic.interface_contract",
                "statement": "  Interface is documented.  ",
                "status": "supported",
                "confidence": 0.9,
                "supports": [second["evidence_id"], first["evidence_id"]],
                "contradicts": []
            }],
            "gaps": [{
                "gap_id": "gap.permissions",
                "check_id": "generic.permission_boundary",
                "description": "  Permission details need review.  ",
                "priority": "high",
                "status": "open",
                "resolution": null
            }],
            "freeze_reason": "  Ready for deterministic scoring.  "
        }),
    )
    .await;

    assert_eq!(response.0, StatusCode::CREATED);
    assert_eq!(response.1["evidence_board"]["run_id"], run_id);
    assert_eq!(response.1["evidence_board"]["standard_version"], "0.3.0");
    assert_eq!(
        response.1["evidence_board"]["claims"][0]["statement"],
        "Interface is documented."
    );
    assert_eq!(
        response.1["evidence_board"]["freeze_reason"],
        "Ready for deterministic scoring."
    );
    let ids = response.1["evidence_board"]["evidence_ids"]
        .as_array()
        .unwrap();
    assert!(ids[0].as_str().unwrap() < ids[1].as_str().unwrap());
    assert_eq!(
        response.1["evidence_manifest"]["entries"][0]["evidence_id"],
        ids[0]
    );
    let first_manifest_entry = response.1["evidence_manifest"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["evidence_id"] == first["evidence_id"])
        .unwrap();
    assert_eq!(
        first_manifest_entry["snapshot_artifact_id"],
        artifact_id.to_string()
    );
    assert_eq!(first_manifest_entry["snapshot_hash"], artifact_hash);

    let fetched = send(
        &app.router,
        Method::GET,
        &format!("/api/runs/{run_id}/evidence-board/1"),
        None,
    )
    .await;
    assert_eq!(fetched, (StatusCode::OK, response.1.clone()));

    let event_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM run_events WHERE run_id = ? AND event_type = 'evidence_board_frozen'",
    )
    .bind(&run_id)
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert_eq!(event_count, 1);

    for statement in [
        "UPDATE evidence_boards SET version = 2 WHERE run_id = ?",
        "DELETE FROM evidence_boards WHERE run_id = ?",
        "UPDATE evidence_manifests SET evidence_board_version = 2 WHERE run_id = ?",
        "DELETE FROM evidence_manifests WHERE run_id = ?",
    ] {
        let error = sqlx::query(statement)
            .bind(&run_id)
            .execute(&app.pool)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("immutable"));
    }
}

#[tokio::test]
async fn api_rejects_invalid_freezes_and_duplicate_version() {
    let app = test_app().await;
    let run_id = create_run(&app.router).await;
    let evidence = create_evidence(&app.router, &run_id, "Evidence").await;
    let valid = freeze_request(vec![evidence["evidence_id"].clone()]);
    assert_eq!(
        freeze(&app.router, &run_id, valid.clone()).await.0,
        StatusCode::CREATED
    );

    assert_error(
        &freeze(&app.router, &run_id, valid).await,
        StatusCode::CONFLICT,
        "evidence_board_already_frozen",
    );

    let second_run = create_run(&app.router).await;
    let cross_run = freeze_request(vec![evidence["evidence_id"].clone()]);
    assert_error(
        &freeze(&app.router, &second_run, cross_run).await,
        StatusCode::BAD_REQUEST,
        "invalid_request",
    );

    let mut outside_claim = freeze_request(Vec::new());
    outside_claim["claims"] = json!([{
        "claim_id": "claim.outside",
        "check_id": "generic.interface_contract",
        "statement": "Outside evidence.",
        "status": "supported",
        "confidence": 1.0,
        "supports": [evidence["evidence_id"]],
        "contradicts": []
    }]);
    assert_error(
        &freeze(&app.router, &second_run, outside_claim).await,
        StatusCode::BAD_REQUEST,
        "invalid_request",
    );

    let mut caller_owned = freeze_request(Vec::new());
    caller_owned["run_id"] = json!(second_run);
    assert_error(
        &freeze(&app.router, &second_run, caller_owned).await,
        StatusCode::BAD_REQUEST,
        "invalid_json",
    );

    assert_error(
        &send(
            &app.router,
            Method::GET,
            &format!("/api/runs/{second_run}/evidence-board/2"),
            None,
        )
        .await,
        StatusCode::NOT_FOUND,
        "evidence_board_not_found",
    );
}

#[tokio::test]
async fn event_failure_rolls_back_board_and_manifest() {
    let app = test_app().await;
    let run_id = create_run(&app.router).await;
    sqlx::query(
        r#"
        CREATE TRIGGER reject_freeze_event
        BEFORE INSERT ON run_events
        WHEN NEW.event_type = 'evidence_board_frozen'
        BEGIN
            SELECT RAISE(ABORT, 'forced freeze event failure');
        END
        "#,
    )
    .execute(&app.pool)
    .await
    .unwrap();

    assert_error(
        &freeze(&app.router, &run_id, freeze_request(Vec::new())).await,
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal_error",
    );
    let board_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM evidence_boards")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    let manifest_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM evidence_manifests")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!((board_count, manifest_count), (0, 0));
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
    let tool_id = format!("github:example/freeze-{}", Uuid::new_v4());
    let repo = tool_id.strip_prefix("github:").unwrap();
    assert_eq!(
        send_json(
            router,
            Method::POST,
            "/api/tools",
            json!({
                "tool_id": tool_id,
                "name": "Freeze Tool",
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
        .await
        .0,
        StatusCode::CREATED
    );
    let run = send_json(
        router,
        Method::POST,
        "/api/runs",
        json!({"goal": "Freeze evidence", "tool_id": tool_id}),
    )
    .await;
    assert_eq!(run.0, StatusCode::CREATED);
    run.1["run_id"].as_str().unwrap().to_owned()
}

async fn create_evidence(router: &Router, run_id: &str, title: &str) -> Value {
    create_evidence_with_snapshot(router, run_id, title, None).await
}

async fn create_evidence_with_snapshot(
    router: &Router,
    run_id: &str,
    title: &str,
    snapshot_artifact_id: Option<Uuid>,
) -> Value {
    let response = send_json(
        router,
        Method::POST,
        &format!("/api/runs/{run_id}/evidence"),
        json!({
            "evidence_schema_version": "0.2.0",
            "source_type": "official_docs",
            "source_url": format!("https://example.com/{title}"),
            "source_revision": null,
            "title": title,
            "excerpt": "Evidence",
            "retrieved_at": "2026-06-13T12:00:00Z",
            "snapshot_artifact_id": snapshot_artifact_id,
            "supports": [],
            "contradicts": [],
            "metadata": {}
        }),
    )
    .await;
    assert_eq!(response.0, StatusCode::CREATED);
    response.1
}

fn freeze_request(evidence_ids: Vec<Value>) -> Value {
    json!({
        "evidence_board_schema_version": "0.1.0",
        "version": 1,
        "evidence_ids": evidence_ids,
        "claims": [],
        "gaps": [],
        "freeze_reason": "Ready for scoring."
    })
}

async fn freeze(router: &Router, run_id: &str, payload: Value) -> (StatusCode, Value) {
    send_json(
        router,
        Method::POST,
        &format!("/api/runs/{run_id}/evidence-board/freeze"),
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
    send(router, method, uri, Some(payload)).await
}

async fn send(
    router: &Router,
    method: Method,
    uri: &str,
    payload: Option<Value>,
) -> (StatusCode, Value) {
    let body = payload.map_or_else(Body::empty, |value| {
        Body::from(serde_json::to_vec(&value).unwrap())
    });
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
