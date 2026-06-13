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
use sha2::{Digest, Sha256};
use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};
use toolpassport_backend::{StorageService, app_with_storage, migrate};
use tower::ServiceExt;
use uuid::Uuid;

const TEST_MAX_BYTES: usize = 1024;

struct TestApp {
    router: Router,
    pool: SqlitePool,
    root: TestDirectory,
}

struct TestDirectory {
    path: PathBuf,
}

impl TestDirectory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("toolpassport-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&path).expect("temporary artifact root must be created");
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
async fn migrations_create_stage_four_tables_and_constraints() {
    let app = test_app().await;
    let objects: Vec<String> = sqlx::query_scalar(
        r#"
        SELECT name
        FROM sqlite_master
        WHERE name IN (
            'artifacts',
            'artifacts_run_created_idx',
            'evidence',
            'evidence_run_created_idx'
        )
        ORDER BY name
        "#,
    )
    .fetch_all(&app.pool)
    .await
    .expect("Stage 4 migration objects must be queryable");

    assert_eq!(
        objects,
        [
            "artifacts",
            "artifacts_run_created_idx",
            "evidence",
            "evidence_run_created_idx"
        ]
    );
}

#[tokio::test]
async fn artifact_upload_persists_exact_bytes_hash_metadata_and_event() {
    let app = test_app().await;
    let run_id = create_run(&app.router).await;

    let created = upload_artifact(
        &app.router,
        &run_id,
        "report.txt",
        "text/plain",
        b"hello world",
    )
    .await;

    assert_eq!(created.0, StatusCode::CREATED);
    assert_eq!(created.1["artifact_schema_version"], "0.1.0");
    assert_eq!(created.1["filename"], "report.txt");
    assert_eq!(created.1["size_bytes"], 11);
    assert_eq!(
        created.1["sha256_hash"],
        "0xb94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
    );
    assert!(created.1.get("storage_key").is_none());

    let artifact_id = created.1["artifact_id"].as_str().unwrap();
    let stored = app
        .root
        .path()
        .join(&run_id)
        .join("artifacts")
        .join(artifact_id);
    assert_eq!(tokio::fs::read(stored).await.unwrap(), b"hello world");

    let listed = send(
        &app.router,
        Method::GET,
        &format!("/api/runs/{run_id}/artifacts"),
        Body::empty(),
        "application/json",
    )
    .await;
    assert_eq!(listed.0, StatusCode::OK);
    assert_eq!(listed.1["artifacts"].as_array().unwrap().len(), 1);

    let details = send(
        &app.router,
        Method::GET,
        &format!("/api/runs/{run_id}"),
        Body::empty(),
        "application/json",
    )
    .await;
    let events = details.1["events"].as_array().unwrap();
    assert_eq!(events.last().unwrap()["event_type"], "artifact_created");
    assert_eq!(
        events.last().unwrap()["payload"]["artifact_id"],
        artifact_id
    );
}

#[tokio::test]
async fn duplicate_display_filenames_do_not_overwrite_stored_artifacts() {
    let app = test_app().await;
    let run_id = create_run(&app.router).await;

    let first = upload_artifact(&app.router, &run_id, "same.txt", "text/plain", b"first").await;
    let second = upload_artifact(&app.router, &run_id, "same.txt", "text/plain", b"second").await;

    assert_eq!(first.0, StatusCode::CREATED);
    assert_eq!(second.0, StatusCode::CREATED);
    assert_ne!(first.1["artifact_id"], second.1["artifact_id"]);
    assert_ne!(first.1["sha256_hash"], second.1["sha256_hash"]);

    let first_path = app
        .root
        .path()
        .join(&run_id)
        .join("artifacts")
        .join(first.1["artifact_id"].as_str().unwrap());
    let second_path = app
        .root
        .path()
        .join(&run_id)
        .join("artifacts")
        .join(second.1["artifact_id"].as_str().unwrap());
    assert_eq!(tokio::fs::read(first_path).await.unwrap(), b"first");
    assert_eq!(tokio::fs::read(second_path).await.unwrap(), b"second");
}

#[tokio::test]
async fn artifact_upload_rejects_path_names_empty_content_large_content_and_missing_runs() {
    let app = test_app().await;
    let run_id = create_run(&app.router).await;

    for filename in ["../../etc/passwd", r"..\secret", "nested/file"] {
        let response = upload_artifact(&app.router, &run_id, filename, "text/plain", b"x").await;
        assert_error(&response, StatusCode::BAD_REQUEST, "invalid_request");
    }

    let empty = upload_artifact(&app.router, &run_id, "empty.txt", "text/plain", b"").await;
    assert_error(&empty, StatusCode::BAD_REQUEST, "invalid_request");

    let too_large = upload_artifact(
        &app.router,
        &run_id,
        "large.bin",
        "application/octet-stream",
        &vec![b'x'; TEST_MAX_BYTES + 1],
    )
    .await;
    assert_error(
        &too_large,
        StatusCode::PAYLOAD_TOO_LARGE,
        "stored_content_too_large",
    );

    let missing = upload_artifact(
        &app.router,
        &Uuid::new_v4().to_string(),
        "file.txt",
        "text/plain",
        b"x",
    )
    .await;
    assert_error(&missing, StatusCode::NOT_FOUND, "run_not_found");
}

#[tokio::test]
async fn structured_evidence_is_stably_hashed_persisted_listed_and_evented() {
    let app = test_app().await;
    let run_id = create_run(&app.router).await;
    let snapshot = upload_artifact(
        &app.router,
        &run_id,
        "snapshot.html",
        "text/html",
        b"<p>source</p>",
    )
    .await;
    let request = evidence_request(Some(snapshot.1["artifact_id"].as_str().unwrap()));

    let first = send_json(
        &app.router,
        Method::POST,
        &format!("/api/runs/{run_id}/evidence"),
        request.clone(),
    )
    .await;
    let second = send_json(
        &app.router,
        Method::POST,
        &format!("/api/runs/{run_id}/evidence"),
        request,
    )
    .await;

    assert_eq!(first.0, StatusCode::CREATED);
    assert_eq!(second.0, StatusCode::CREATED);
    assert_eq!(first.1["evidence_schema_version"], "0.2.0");
    assert_eq!(first.1["content_hash"], second.1["content_hash"]);
    assert_ne!(first.1["evidence_id"], second.1["evidence_id"]);
    assert!(first.1["content_hash"].as_str().unwrap().starts_with("0x"));
    assert!(first.1.get("storage_key").is_none());

    let evidence_id = first.1["evidence_id"].as_str().unwrap();
    let stored_path = app
        .root
        .path()
        .join(&run_id)
        .join("evidence")
        .join(format!("{evidence_id}.json"));
    let stored_bytes = tokio::fs::read(stored_path).await.unwrap();
    let stored: Value = serde_json::from_slice(&stored_bytes).unwrap();
    assert_eq!(
        first.1["content_hash"],
        format!("0x{}", hex::encode(Sha256::digest(&stored_bytes)))
    );
    assert_eq!(stored["title"], "Interface documentation");
    assert!(stored.get("evidence_id").is_none());
    assert!(stored.get("content_hash").is_none());

    let listed = send(
        &app.router,
        Method::GET,
        &format!("/api/runs/{run_id}/evidence"),
        Body::empty(),
        "application/json",
    )
    .await;
    assert_eq!(listed.0, StatusCode::OK);
    assert_eq!(listed.1["evidence"].as_array().unwrap().len(), 2);

    let details = send(
        &app.router,
        Method::GET,
        &format!("/api/runs/{run_id}"),
        Body::empty(),
        "application/json",
    )
    .await;
    let evidence_events = details.1["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|event| event["event_type"] == "evidence_created")
        .count();
    assert_eq!(evidence_events, 2);
}

#[tokio::test]
async fn database_event_failure_rolls_back_metadata_and_removes_stored_file() {
    let app = test_app().await;
    let run_id = create_run(&app.router).await;
    sqlx::query(
        r#"
        CREATE TRIGGER reject_artifact_event
        BEFORE INSERT ON run_events
        WHEN NEW.event_type = 'artifact_created'
        BEGIN
            SELECT RAISE(ABORT, 'reject artifact event');
        END
        "#,
    )
    .execute(&app.pool)
    .await
    .unwrap();

    let response =
        upload_artifact(&app.router, &run_id, "report.txt", "text/plain", b"content").await;
    assert_error(
        &response,
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal_error",
    );

    let artifact_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM artifacts WHERE run_id = ?")
        .bind(&run_id)
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(artifact_count, 0);

    let artifact_dir = app.root.path().join(&run_id).join("artifacts");
    assert_eq!(std::fs::read_dir(artifact_dir).unwrap().count(), 0);
}

#[tokio::test]
async fn evidence_rejects_invalid_contract_cross_run_snapshot_and_missing_run() {
    let app = test_app().await;
    let first_run = create_run(&app.router).await;
    let second_run = create_run(&app.router).await;
    let snapshot = upload_artifact(
        &app.router,
        &first_run,
        "snapshot.txt",
        "text/plain",
        b"source",
    )
    .await;

    let cross_run = send_json(
        &app.router,
        Method::POST,
        &format!("/api/runs/{second_run}/evidence"),
        evidence_request(Some(snapshot.1["artifact_id"].as_str().unwrap())),
    )
    .await;
    assert_error(&cross_run, StatusCode::BAD_REQUEST, "invalid_request");

    let mut invalid_version = evidence_request(None);
    invalid_version["evidence_schema_version"] = json!("0.1.0");
    let invalid_version = send_json(
        &app.router,
        Method::POST,
        &format!("/api/runs/{first_run}/evidence"),
        invalid_version,
    )
    .await;
    assert_error(&invalid_version, StatusCode::BAD_REQUEST, "invalid_request");

    let mut duplicate_refs = evidence_request(None);
    duplicate_refs["supports"] = json!(["check.a", "check.a"]);
    let duplicate_refs = send_json(
        &app.router,
        Method::POST,
        &format!("/api/runs/{first_run}/evidence"),
        duplicate_refs,
    )
    .await;
    assert_error(&duplicate_refs, StatusCode::BAD_REQUEST, "invalid_request");

    let mut invalid_url = evidence_request(None);
    invalid_url["source_url"] = json!("https://");
    let invalid_url = send_json(
        &app.router,
        Method::POST,
        &format!("/api/runs/{first_run}/evidence"),
        invalid_url,
    )
    .await;
    assert_error(&invalid_url, StatusCode::BAD_REQUEST, "invalid_request");

    let mut unknown_field = evidence_request(None);
    unknown_field["unexpected"] = json!(true);
    let unknown_field = send_json(
        &app.router,
        Method::POST,
        &format!("/api/runs/{first_run}/evidence"),
        unknown_field,
    )
    .await;
    assert_error(&unknown_field, StatusCode::BAD_REQUEST, "invalid_json");

    let missing_run = Uuid::new_v4().to_string();
    let missing = send_json(
        &app.router,
        Method::POST,
        &format!("/api/runs/{missing_run}/evidence"),
        evidence_request(None),
    )
    .await;
    assert_error(&missing, StatusCode::NOT_FOUND, "run_not_found");

    for collection in ["artifacts", "evidence"] {
        let response = send(
            &app.router,
            Method::GET,
            &format!("/api/runs/{missing_run}/{collection}"),
            Body::empty(),
            "application/json",
        )
        .await;
        assert_error(&response, StatusCode::NOT_FOUND, "run_not_found");
    }
}

#[cfg(unix)]
#[tokio::test]
async fn storage_rejects_symlink_escape() {
    use std::os::unix::fs::symlink;

    let root = TestDirectory::new();
    let outside = TestDirectory::new();
    let run_id = Uuid::new_v4();
    let run_dir = root.path().join(run_id.to_string());
    std::fs::create_dir_all(&run_dir).unwrap();
    symlink(outside.path(), run_dir.join("artifacts")).unwrap();

    let storage = StorageService::new(root.path(), TEST_MAX_BYTES);
    let error = storage
        .save_artifact(run_id, Uuid::new_v4(), b"escape")
        .await
        .unwrap_err();
    assert!(error.to_string().contains("escaped"));
}

async fn test_app() -> TestApp {
    let pool = test_pool().await;
    let root = TestDirectory::new();
    let router = app_with_storage(
        pool.clone(),
        StorageService::new(root.path(), TEST_MAX_BYTES),
    );
    TestApp { router, pool, root }
}

async fn test_pool() -> SqlitePool {
    let options = SqliteConnectOptions::from_str("sqlite::memory:")
        .expect("in-memory SQLite URL must parse")
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .expect("test database must connect");
    migrate(&pool).await.expect("migrations must run");
    pool
}

async fn create_run(router: &Router) -> String {
    let tool_id = format!("github:example-org/example-{}", Uuid::new_v4());
    let repository = tool_id.trim_start_matches("github:");
    let tool = send_json(
        router,
        Method::POST,
        "/api/tools",
        json!({
            "tool_id": tool_id,
            "name": "example",
            "tool_type": "generic",
            "canonical_url": format!("https://github.com/{repository}"),
            "external_identifiers": [{
                "namespace": "github",
                "value": repository,
                "canonical_url": format!("https://github.com/{repository}")
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
    run.1["run_id"].as_str().unwrap().to_owned()
}

fn evidence_request(snapshot_artifact_id: Option<&str>) -> Value {
    json!({
        "evidence_schema_version": "0.2.0",
        "source_type": "official_docs",
        "source_url": "https://example.com/docs",
        "source_revision": "v1.0.0",
        "title": "Interface documentation",
        "excerpt": "The interface accepts structured JSON.",
        "retrieved_at": "2026-06-12T20:00:00Z",
        "snapshot_artifact_id": snapshot_artifact_id,
        "supports": ["check.structured_io"],
        "contradicts": [],
        "metadata": {"section": "API"}
    })
}

async fn upload_artifact(
    router: &Router,
    run_id: &str,
    filename: &str,
    content_type: &str,
    content: &[u8],
) -> (StatusCode, Value) {
    let boundary = "stage-four-boundary";
    let mut body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\nContent-Type: {content_type}\r\n\r\n"
    )
    .into_bytes();
    body.extend_from_slice(content);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

    send(
        router,
        Method::POST,
        &format!("/api/runs/{run_id}/artifacts"),
        Body::from(body),
        &format!("multipart/form-data; boundary={boundary}"),
    )
    .await
}

async fn send_json(
    router: &Router,
    method: Method,
    uri: &str,
    payload: Value,
) -> (StatusCode, Value) {
    send(
        router,
        method,
        uri,
        Body::from(serde_json::to_vec(&payload).unwrap()),
        "application/json",
    )
    .await
}

async fn send(
    router: &Router,
    method: Method,
    uri: &str,
    body: Body,
    content_type: &str,
) -> (StatusCode, Value) {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .header(CONTENT_TYPE, content_type)
                .body(body)
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let payload = if body.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&body).unwrap()
    };
    (status, payload)
}

fn assert_error(response: &(StatusCode, Value), status: StatusCode, code: &str) {
    assert_eq!(response.0, status);
    assert_eq!(response.1["code"], code);
    assert!(
        response.1["message"]
            .as_str()
            .is_some_and(|value| !value.is_empty())
    );
    assert!(response.1["details"].is_object());
}
