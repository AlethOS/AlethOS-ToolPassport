use std::str::FromStr;

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
use toolpassport_backend::{app, migrate};
use tower::ServiceExt;
use uuid::Uuid;

// ═══════════════════════════════════════════════════════════════════
// Helpers (same pattern as tools.rs, vendored here for test isolation)
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn migrations_create_run_tables_and_append_only_triggers() {
    let pool = test_pool().await;

    let object_names: Vec<String> = sqlx::query_scalar(
        r#"
        SELECT name
        FROM sqlite_master
        WHERE name IN (
            'runs',
            'run_events',
            'run_events_prevent_update',
            'run_events_prevent_delete'
        )
        ORDER BY name
        "#,
    )
    .fetch_all(&pool)
    .await
    .expect("migration objects must be queryable");

    assert_eq!(
        object_names,
        [
            "run_events",
            "run_events_prevent_delete",
            "run_events_prevent_update",
            "runs"
        ]
    );
}

#[tokio::test]
async fn creates_and_queries_runs() {
    let (router, _) = test_app().await;

    // Create a tool first, then bind the run to it.
    let tool_id = create_github_tool(&router).await;

    let created = send_json(
        &router,
        Method::POST,
        "/api/runs",
        json!({
            "goal": "Audit the tool",
            "tool_id": tool_id
        }),
    )
    .await;

    assert_eq!(created.0, StatusCode::CREATED);
    assert_eq!(created.1["goal"], "Audit the tool");
    assert_eq!(created.1["tool_id"], tool_id);
    assert_eq!(created.1["tool"]["name"], "example-lib");
    assert_eq!(created.1["tool"]["tool_type"], "generic");
    assert_eq!(created.1["status"], "pending");
    let run_id = created.1["run_id"]
        .as_str()
        .expect("created run must have an ID");
    Uuid::parse_str(run_id).expect("run ID must be a UUID");

    let list = send(&router, Method::GET, "/api/runs", Body::empty()).await;
    assert_eq!(list.0, StatusCode::OK);
    assert_eq!(
        list.1["runs"]
            .as_array()
            .expect("runs must be an array")
            .len(),
        1
    );
    assert_eq!(list.1["runs"][0]["run_id"], run_id);

    let details = send(
        &router,
        Method::GET,
        &format!("/api/runs/{run_id}"),
        Body::empty(),
    )
    .await;
    assert_eq!(details.0, StatusCode::OK);
    assert_eq!(details.1["run"]["run_id"], run_id);
    assert_eq!(details.1["run"]["tool_id"], tool_id);
    assert_eq!(details.1["events"][0]["event_type"], "run_created");
    assert_eq!(details.1["events"][0]["node_id"], "run");
    assert_eq!(details.1["events"][0]["payload"]["status"], "pending");
}

#[tokio::test]
async fn appends_events_in_order_and_rejects_mutation() {
    let (router, pool) = test_app().await;
    let tool_id = create_github_tool(&router).await;
    let run_id = create_run(&router, &tool_id).await;

    let first = append_event(&router, &run_id, "plan_audit", "node_started").await;
    let second = append_event(&router, &run_id, "plan_audit", "node_finished").await;

    assert_eq!(first.0, StatusCode::CREATED);
    assert_eq!(second.0, StatusCode::CREATED);
    assert!(first.1.get("sequence").is_none());
    assert!(second.1.get("sequence").is_none());

    let details = send(
        &router,
        Method::GET,
        &format!("/api/runs/{run_id}"),
        Body::empty(),
    )
    .await;
    let events = details.1["events"]
        .as_array()
        .expect("events must be an array");
    assert_eq!(events[0]["event_type"], "run_created");
    assert_eq!(events[1]["event_type"], "node_started");
    assert_eq!(events[2]["event_type"], "node_finished");
    assert_eq!(details.1["run"]["status"], "running");
    assert_eq!(details.1["run"]["current_node"], "plan_audit");

    let event_id = first.1["event_id"]
        .as_str()
        .expect("event ID must be present");
    let update_error = sqlx::query("UPDATE run_events SET node_id = 'changed' WHERE event_id = ?")
        .bind(event_id)
        .execute(&pool)
        .await
        .expect_err("event update must be rejected");
    assert!(update_error.to_string().contains("append-only"));

    let delete_error = sqlx::query("DELETE FROM run_events WHERE event_id = ?")
        .bind(event_id)
        .execute(&pool)
        .await
        .expect_err("event delete must be rejected");
    assert!(delete_error.to_string().contains("append-only"));
}

#[tokio::test]
async fn projects_approval_and_terminal_status_events() {
    let (router, _) = test_app().await;
    let tool_id = create_github_tool(&router).await;
    let run_id = create_run(&router, &tool_id).await;

    let started = append_event(&router, &run_id, "plan_audit", "node_started").await;
    assert_eq!(started.0, StatusCode::CREATED);

    let approval_required =
        append_event(&router, &run_id, "human_review_gate", "approval_required").await;
    assert_eq!(approval_required.0, StatusCode::CREATED);
    let details = get_run(&router, &run_id).await;
    assert_eq!(details.1["run"]["status"], "waiting_approval");
    assert_eq!(details.1["run"]["current_node"], "human_review_gate");

    let blocked_node = append_event(&router, &run_id, "next_node", "node_started").await;
    assert_error(&blocked_node, StatusCode::BAD_REQUEST, "invalid_request");

    let approval_resolved =
        append_event(&router, &run_id, "human_review_gate", "approval_resolved").await;
    assert_eq!(approval_resolved.0, StatusCode::CREATED);

    let finished = append_event_with_payload(
        &router,
        &run_id,
        "finish",
        "run_status_changed",
        json!({"status": "success"}),
    )
    .await;
    assert_eq!(finished.0, StatusCode::CREATED);

    let details = get_run(&router, &run_id).await;
    assert_eq!(details.1["run"]["status"], "success");
    assert_eq!(details.1["run"]["current_node"], "finish");
}

#[tokio::test]
async fn rejects_trust_core_owned_and_invalid_status_events() {
    let (router, _) = test_app().await;
    let tool_id = create_github_tool(&router).await;
    let run_id = create_run(&router, &tool_id).await;

    let duplicate_created = append_event(&router, &run_id, "run", "run_created").await;
    assert_error(
        &duplicate_created,
        StatusCode::BAD_REQUEST,
        "invalid_request",
    );

    let invalid_status = append_event_with_payload(
        &router,
        &run_id,
        "finish",
        "run_status_changed",
        json!({"status": "cancelled"}),
    )
    .await;
    assert_error(&invalid_status, StatusCode::BAD_REQUEST, "invalid_request");

    let success_from_pending = append_event_with_payload(
        &router,
        &run_id,
        "finish",
        "run_status_changed",
        json!({"status": "success"}),
    )
    .await;
    assert_error(
        &success_from_pending,
        StatusCode::BAD_REQUEST,
        "invalid_request",
    );

    let details = get_run(&router, &run_id).await;
    assert_eq!(details.1["run"]["status"], "pending");
    assert_eq!(
        details.1["events"]
            .as_array()
            .expect("events must be an array")
            .len(),
        1
    );
}

#[tokio::test]
async fn returns_json_errors_for_missing_runs() {
    let (router, _) = test_app().await;
    let missing_id = Uuid::new_v4();

    let response = send(
        &router,
        Method::GET,
        &format!("/api/runs/{missing_id}"),
        Body::empty(),
    )
    .await;
    assert_error(&response, StatusCode::NOT_FOUND, "run_not_found");

    let response = append_event(&router, &missing_id.to_string(), "node", "node_started").await;
    assert_error(&response, StatusCode::NOT_FOUND, "run_not_found");
}

#[tokio::test]
async fn returns_json_errors_for_invalid_requests() {
    let (router, _) = test_app().await;

    // Missing tool_id.
    let response = send_json(
        &router,
        Method::POST,
        "/api/runs",
        json!({
            "goal": "Audit the tool"
        }),
    )
    .await;
    assert_error(&response, StatusCode::BAD_REQUEST, "invalid_json");

    // Empty goal.
    let response = send_json(
        &router,
        Method::POST,
        "/api/runs",
        json!({
            "goal": " ",
            "tool_id": "github:owner/repo"
        }),
    )
    .await;
    assert_error(&response, StatusCode::BAD_REQUEST, "invalid_request");

    // Invalid JSON body.
    let response = send(&router, Method::POST, "/api/runs", Body::from("{invalid")).await;
    assert_error(&response, StatusCode::BAD_REQUEST, "invalid_json");

    // Invalid run_id (not a UUID).
    let response = send(&router, Method::GET, "/api/runs/not-a-uuid", Body::empty()).await;
    assert_error(&response, StatusCode::BAD_REQUEST, "invalid_run_id");
}

#[tokio::test]
async fn run_binding_requires_existing_tool() {
    let (router, _) = test_app().await;

    let response = send_json(
        &router,
        Method::POST,
        "/api/runs",
        json!({
            "goal": "Audit a nonexistent tool",
            "tool_id": "github:nonexistent/repo"
        }),
    )
    .await;
    assert_error(&response, StatusCode::NOT_FOUND, "tool_not_found");
}

#[tokio::test]
async fn run_binding_freezes_tool_snapshot() {
    let (router, _) = test_app().await;

    // Create a tool.
    let tool_id = create_github_tool(&router).await;

    // Create a run bound to it — snapshot is frozen at creation time.
    let run = send_json(
        &router,
        Method::POST,
        "/api/runs",
        json!({
            "goal": "First audit",
            "tool_id": &tool_id
        }),
    )
    .await;
    assert_eq!(run.0, StatusCode::CREATED);
    let run_id = run.1["run_id"]
        .as_str()
        .expect("run_id must be present")
        .to_owned();

    // Fetch the run and verify the frozen snapshot fields.
    let details = get_run(&router, &run_id).await;
    assert_eq!(details.1["run"]["tool_id"], tool_id);
    assert_eq!(
        details.1["run"]["canonical_url"],
        "https://github.com/example-org/example-lib"
    );
    assert_eq!(details.1["run"]["tool"]["name"], "example-lib");
    assert_eq!(details.1["run"]["tool"]["tool_type"], "generic");
    assert_eq!(
        details.1["run"]["tool"]["urls"][0],
        "https://github.com/example-org/example-lib"
    );

    // Update the Tool (add an alias) — the Run snapshot must NOT change.
    let _alias_result = send_json(
        &router,
        Method::POST,
        &format!("/api/tools/{tool_id}/identifiers"),
        json!({
            "identifier": {
                "namespace": "url",
                "value": "other.example.com/tool",
                "canonical_url": "https://other.example.com/tool"
            }
        }),
    )
    .await;

    // Re-fetch the run — snapshot unchanged.
    let details_after = get_run(&router, &run_id).await;
    assert_eq!(details_after.1["run"]["tool_id"], tool_id);
    assert_eq!(
        details_after.1["run"]["canonical_url"],
        "https://github.com/example-org/example-lib"
    );
}

#[tokio::test]
async fn run_binding_rejects_invalid_tool_id_format() {
    let (router, _) = test_app().await;

    let response = send_json(
        &router,
        Method::POST,
        "/api/runs",
        json!({
            "goal": "Invalid tool",
            "tool_id": "not-a-namespaced-id"
        }),
    )
    .await;
    assert_error(&response, StatusCode::BAD_REQUEST, "invalid_tool_id_format");
}

// ═══════════════════════════════════════════════════════════════════
// Test infrastructure
// ═══════════════════════════════════════════════════════════════════

async fn test_app() -> (Router, SqlitePool) {
    let pool = test_pool().await;
    (app(pool.clone()), pool)
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

async fn create_github_tool(router: &Router) -> String {
    let response = send_json(
        router,
        Method::POST,
        "/api/tools",
        json!({
            "tool_id": "github:example-org/example-lib",
            "name": "example-lib",
            "tool_type": "generic",
            "canonical_url": "https://github.com/example-org/example-lib",
            "external_identifiers": [
                {
                    "namespace": "github",
                    "value": "example-org/example-lib",
                    "canonical_url": "https://github.com/example-org/example-lib"
                }
            ],
            "aliases": ["Example Library"]
        }),
    )
    .await;
    assert_eq!(
        response.0,
        StatusCode::CREATED,
        "tool creation must succeed: {response:?}"
    );
    response.1["tool_id"]
        .as_str()
        .expect("created tool must have an ID")
        .to_owned()
}

async fn create_run(router: &Router, tool_id: &str) -> String {
    let response = send_json(
        router,
        Method::POST,
        "/api/runs",
        json!({
            "goal": "Audit the tool",
            "tool_id": tool_id
        }),
    )
    .await;
    assert_eq!(
        response.0,
        StatusCode::CREATED,
        "run creation must succeed: {response:?}"
    );
    response.1["run_id"]
        .as_str()
        .expect("created run must have an ID")
        .to_owned()
}

async fn append_event(
    router: &Router,
    run_id: &str,
    node_id: &str,
    event_type: &str,
) -> (StatusCode, Value) {
    append_event_with_payload(router, run_id, node_id, event_type, json!({})).await
}

async fn append_event_with_payload(
    router: &Router,
    run_id: &str,
    node_id: &str,
    event_type: &str,
    payload: Value,
) -> (StatusCode, Value) {
    send_json(
        router,
        Method::POST,
        &format!("/api/runs/{run_id}/events"),
        json!({
            "node_id": node_id,
            "event_type": event_type,
            "payload": payload
        }),
    )
    .await
}

async fn get_run(router: &Router, run_id: &str) -> (StatusCode, Value) {
    send(
        router,
        Method::GET,
        &format!("/api/runs/{run_id}"),
        Body::empty(),
    )
    .await
}

async fn send_json(
    router: &Router,
    method: Method,
    uri: &str,
    payload: Value,
) -> (StatusCode, Value) {
    let body = Body::from(serde_json::to_vec(&payload).expect("JSON body must serialize"));
    send(router, method, uri, body).await
}

async fn send(router: &Router, method: Method, uri: &str, body: Body) -> (StatusCode, Value) {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .header(CONTENT_TYPE, "application/json")
                .body(body)
                .expect("request must build"),
        )
        .await
        .expect("request must complete");
    let status = response.status();
    let body = response
        .into_body()
        .collect()
        .await
        .expect("response body must collect")
        .to_bytes();
    let payload = serde_json::from_slice(&body).expect("response body must be JSON");
    (status, payload)
}

fn assert_error(response: &(StatusCode, Value), status: StatusCode, code: &str) {
    assert_eq!(response.0, status);
    assert_eq!(response.1["code"], code);
    assert!(
        response.1["message"]
            .as_str()
            .is_some_and(|message| !message.is_empty())
    );
    assert!(response.1["details"].is_object());
}
