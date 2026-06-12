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
    let created = send_json(
        &router,
        Method::POST,
        "/api/runs",
        json!({
            "goal": "Audit the tool",
            "tool": {
                "name": "Example Tool",
                "tool_type": "agent_framework",
                "urls": ["https://example.com"]
            }
        }),
    )
    .await;

    assert_eq!(created.0, StatusCode::CREATED);
    assert_eq!(created.1["goal"], "Audit the tool");
    assert_eq!(created.1["tool"]["name"], "Example Tool");
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
    assert_eq!(details.1["events"], json!([]));
}

#[tokio::test]
async fn appends_events_in_order_and_rejects_mutation() {
    let (router, pool) = test_app().await;
    let run_id = create_run(&router).await;

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
    assert_eq!(events[0]["event_type"], "node_started");
    assert_eq!(events[1]["event_type"], "node_finished");

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

    let response = send_json(
        &router,
        Method::POST,
        "/api/runs",
        json!({
            "goal": " ",
            "tool": {
                "name": "Example Tool",
                "tool_type": "agent_framework"
            }
        }),
    )
    .await;
    assert_error(&response, StatusCode::BAD_REQUEST, "invalid_request");

    let response = send(&router, Method::POST, "/api/runs", Body::from("{invalid")).await;
    assert_error(&response, StatusCode::BAD_REQUEST, "invalid_json");

    let response = send(&router, Method::GET, "/api/runs/not-a-uuid", Body::empty()).await;
    assert_error(&response, StatusCode::BAD_REQUEST, "invalid_run_id");
}

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

async fn create_run(router: &Router) -> String {
    let response = send_json(
        router,
        Method::POST,
        "/api/runs",
        json!({
            "goal": "Audit the tool",
            "tool": {
                "name": "Example Tool",
                "tool_type": "agent_framework"
            }
        }),
    )
    .await;
    assert_eq!(response.0, StatusCode::CREATED);
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
    send_json(
        router,
        Method::POST,
        &format!("/api/runs/{run_id}/events"),
        json!({
            "node_id": node_id,
            "event_type": event_type,
            "payload": {}
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
