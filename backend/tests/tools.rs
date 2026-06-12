use std::str::FromStr;

use axum::{
    Router,
    body::Body,
    http::{Method, StatusCode, header::CONTENT_TYPE},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};
use toolpassport_backend::{app, migrate};
use tower::ServiceExt;

// ── Helpers ──────────────────────────────────────────────────────

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

async fn test_app() -> (Router, SqlitePool) {
    let pool = test_pool().await;
    (app(pool.clone()), pool)
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

use axum::http::Request;

fn github_tool_request(owner: &str, repo: &str, name: &str, tool_type: &str) -> Value {
    let tool_id = format!("github:{owner}/{repo}");
    let canonical_url = format!("https://github.com/{owner}/{repo}");
    json!({
        "tool_id": tool_id,
        "name": name,
        "tool_type": tool_type,
        "canonical_url": canonical_url,
        "external_identifiers": [{
            "namespace": "github",
            "value": format!("{owner}/{repo}"),
            "canonical_url": canonical_url
        }]
    })
}

async fn create_tool_via_api(router: &Router, payload: Value) -> String {
    let response = send_json(router, Method::POST, "/api/tools", payload).await;
    assert_eq!(response.0, StatusCode::CREATED);
    response.1["tool_id"]
        .as_str()
        .expect("created tool must have an ID")
        .to_owned()
}

// ── Migration Tests ──────────────────────────────────────────────

#[tokio::test]
async fn migrations_create_tool_tables_and_constraints() {
    let pool = test_pool().await;

    let object_names: Vec<String> = sqlx::query_scalar(
        r#"
        SELECT name
        FROM sqlite_master
        WHERE name IN (
            'tools',
            'tool_external_ids',
            'tool_aliases',
            'tool_aliases_alias_idx'
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
            "tool_aliases",
            "tool_aliases_alias_idx",
            "tool_external_ids",
            "tools"
        ]
    );
}

// ── CRUD Tests ────────────────────────────────────────────────────

#[tokio::test]
async fn creates_and_queries_tool() {
    let (router, _) = test_app().await;

    let payload = github_tool_request("langchain-ai", "langgraph", "LangGraph", "agent_framework");
    let created = send_json(&router, Method::POST, "/api/tools", payload).await;

    assert_eq!(created.0, StatusCode::CREATED);
    assert_eq!(created.1["tool_schema_version"], "0.1.0");
    assert_eq!(created.1["tool_id"], "github:langchain-ai/langgraph");
    assert_eq!(created.1["name"], "LangGraph");
    assert_eq!(created.1["tool_type"], "agent_framework");
    assert_eq!(
        created.1["canonical_url"],
        "https://github.com/langchain-ai/langgraph"
    );
    assert_eq!(
        created.1["external_identifiers"].as_array().unwrap().len(),
        1
    );
    assert!(created.1["aliases"].as_array().unwrap().is_empty());
    assert!(created.1["created_at"].is_string());
    assert!(created.1["updated_at"].is_string());

    let list = send(&router, Method::GET, "/api/tools", Body::empty()).await;
    assert_eq!(list.0, StatusCode::OK);
    assert_eq!(list.1["tools"].as_array().unwrap().len(), 1);

    let detail = send(
        &router,
        Method::GET,
        "/api/tools/by-id?tool_id=github:langchain-ai/langgraph",
        Body::empty(),
    )
    .await;
    assert_eq!(detail.0, StatusCode::OK);
    assert_eq!(detail.1["tool_id"], "github:langchain-ai/langgraph");
}

#[tokio::test]
async fn creates_tool_with_aliases() {
    let (router, _) = test_app().await;

    let payload = json!({
        "tool_id": "github:owner/repo",
        "name": "My Tool",
        "tool_type": "generic",
        "canonical_url": "https://github.com/owner/repo",
        "external_identifiers": [{
            "namespace": "github",
            "value": "owner/repo",
            "canonical_url": "https://github.com/owner/repo"
        }],
        "aliases": ["Old Name", "Legacy Alias"]
    });

    let created = send_json(&router, Method::POST, "/api/tools", payload).await;
    assert_eq!(created.0, StatusCode::CREATED);
    assert_eq!(
        created.1["aliases"].as_array().unwrap(),
        &["Old Name", "Legacy Alias"]
    );
}

// ── Conflict Tests ───────────────────────────────────────────────

#[tokio::test]
async fn rejects_duplicate_tool_id() {
    let (router, _) = test_app().await;

    let payload = github_tool_request("owner", "repo", "Tool", "generic");
    create_tool_via_api(&router, payload.clone()).await;

    let duplicate = send_json(&router, Method::POST, "/api/tools", payload).await;
    assert_error(&duplicate, StatusCode::CONFLICT, "tool_already_exists");
}

#[tokio::test]
async fn rejects_duplicate_external_identifier() {
    let (router, _) = test_app().await;

    let payload = github_tool_request("owner", "repo", "Tool A", "generic");
    create_tool_via_api(&router, payload).await;

    let second = github_tool_request("owner", "repo", "Tool B", "generic");
    let response = send_json(&router, Method::POST, "/api/tools", second).await;
    assert_error(&response, StatusCode::CONFLICT, "tool_already_exists");
}

#[tokio::test]
async fn rejects_invalid_tool_id_format() {
    let (router, _) = test_app().await;

    let payload = json!({
        "tool_id": "invalid:id with spaces",
        "name": "Bad Tool",
        "tool_type": "generic",
        "canonical_url": "https://example.com",
        "external_identifiers": []
    });

    let response = send_json(&router, Method::POST, "/api/tools", payload).await;
    assert_error(&response, StatusCode::BAD_REQUEST, "invalid_tool_id_format");
}

#[tokio::test]
async fn rejects_missing_identifiers() {
    let (router, _) = test_app().await;

    let payload = json!({
        "tool_id": "github:owner/repo",
        "name": "No IDs",
        "tool_type": "generic",
        "canonical_url": "https://github.com/owner/repo",
        "external_identifiers": []
    });

    let response = send_json(&router, Method::POST, "/api/tools", payload).await;
    assert_error(&response, StatusCode::BAD_REQUEST, "invalid_request");
}

// ── Resolution Tests ───────────────────────────────────────────────

#[tokio::test]
async fn resolve_create_candidate_for_github_url() {
    let (router, _) = test_app().await;

    let response = send_json(
        &router,
        Method::POST,
        "/api/tools/resolve",
        json!({
            "intake_version": "0.1.0",
            "name": "LangGraph",
            "tool_type": "agent_framework",
            "urls": ["https://GitHub.com/Langchain-AI/LangGraph.git/"]
        }),
    )
    .await;

    assert_eq!(response.0, StatusCode::OK);
    assert_eq!(response.1["status"], "create_candidate");
    assert_eq!(response.1["tool_id"], "github:langchain-ai/langgraph");
    assert_eq!(
        response.1["normalized_identifiers"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let ident = &response.1["normalized_identifiers"][0];
    assert_eq!(ident["namespace"], "github");
    assert_eq!(ident["value"], "langchain-ai/langgraph");
    assert_eq!(response.1["reason_codes"], json!(["new_strong_identifier"]));
}

#[tokio::test]
async fn resolve_create_candidate_for_https_url() {
    let (router, _) = test_app().await;

    let response = send_json(
        &router,
        Method::POST,
        "/api/tools/resolve",
        json!({
            "intake_version": "0.1.0",
            "name": "Example Tool",
            "tool_type": "generic",
            "urls": ["https://Example.com:443/Tool/"]
        }),
    )
    .await;

    assert_eq!(response.0, StatusCode::OK);
    assert_eq!(response.1["status"], "create_candidate");
    assert_eq!(response.1["tool_id"], "url:example.com/Tool");
    let ident = &response.1["normalized_identifiers"][0];
    assert_eq!(ident["namespace"], "url");
    assert_eq!(ident["value"], "example.com/Tool");
}

#[tokio::test]
async fn resolve_resolved_after_tool_creation() {
    let (router, _) = test_app().await;

    let payload = github_tool_request("langchain-ai", "langgraph", "LangGraph", "agent_framework");
    create_tool_via_api(&router, payload).await;

    let response = send_json(
        &router,
        Method::POST,
        "/api/tools/resolve",
        json!({
            "intake_version": "0.1.0",
            "name": "LangGraph",
            "tool_type": "agent_framework",
            "urls": ["https://github.com/langchain-ai/langgraph"]
        }),
    )
    .await;

    assert_eq!(response.0, StatusCode::OK);
    assert_eq!(response.1["status"], "resolved");
    assert_eq!(response.1["tool_id"], "github:langchain-ai/langgraph");
    assert_eq!(
        response.1["candidate_tool_ids"],
        json!(["github:langchain-ai/langgraph"])
    );
    assert_eq!(
        response.1["reason_codes"],
        json!(["existing_identifier_match"])
    );
}

#[tokio::test]
async fn resolve_needs_review_invalid_url() {
    let (router, _) = test_app().await;

    let response = send_json(
        &router,
        Method::POST,
        "/api/tools/resolve",
        json!({
            "intake_version": "0.1.0",
            "name": "Bad Tool",
            "tool_type": "generic",
            "urls": ["http://example.com/tool", "https://example.com/tool?query=1"]
        }),
    )
    .await;

    assert_eq!(response.0, StatusCode::OK);
    assert_eq!(response.1["status"], "needs_review");
    assert!(response.1["tool_id"].is_null());
    assert_eq!(
        response.1["reason_codes"],
        json!(["invalid_or_ambiguous_url"])
    );
}

#[tokio::test]
async fn resolve_needs_review_name_only() {
    let (router, _) = test_app().await;

    let response = send_json(
        &router,
        Method::POST,
        "/api/tools/resolve",
        json!({
            "intake_version": "0.1.0",
            "name": "Just A Name",
            "tool_type": "generic",
            "urls": []
        }),
    )
    .await;

    assert_eq!(response.0, StatusCode::OK);
    assert_eq!(response.1["status"], "needs_review");
    assert_eq!(response.1["reason_codes"], json!(["name_only"]));
}

#[tokio::test]
async fn resolve_needs_review_name_match_only() {
    let (router, _) = test_app().await;

    let payload = json!({
        "tool_id": "github:owner/repo",
        "name": "Old Framework",
        "tool_type": "agent_framework",
        "canonical_url": "https://github.com/owner/repo",
        "external_identifiers": [{
            "namespace": "github",
            "value": "owner/repo",
            "canonical_url": "https://github.com/owner/repo"
        }],
        "aliases": ["Legacy Name"]
    });
    create_tool_via_api(&router, payload).await;

    let response = send_json(
        &router,
        Method::POST,
        "/api/tools/resolve",
        json!({
            "intake_version": "0.1.0",
            "name": "Legacy Name",
            "tool_type": "agent_framework",
            "urls": []
        }),
    )
    .await;

    assert_eq!(response.0, StatusCode::OK);
    assert_eq!(response.1["status"], "needs_review");
    assert_eq!(response.1["reason_codes"], json!(["name_match_only"]));
    assert_eq!(
        response.1["candidate_tool_ids"],
        json!(["github:owner/repo"])
    );
}

#[tokio::test]
async fn resolve_needs_review_conflicting_identifiers() {
    let (router, _) = test_app().await;

    create_tool_via_api(
        &router,
        github_tool_request("org-a", "tool", "Tool A", "generic"),
    )
    .await;
    create_tool_via_api(
        &router,
        github_tool_request("org-b", "tool", "Tool B", "generic"),
    )
    .await;

    let response = send_json(
        &router,
        Method::POST,
        "/api/tools/resolve",
        json!({
            "intake_version": "0.1.0",
            "name": "Confused",
            "tool_type": "generic",
            "urls": [
                "https://github.com/org-a/tool",
                "https://github.com/org-b/tool"
            ]
        }),
    )
    .await;

    assert_eq!(response.0, StatusCode::OK);
    assert_eq!(response.1["status"], "needs_review");
    assert_eq!(
        response.1["reason_codes"],
        json!(["conflicting_existing_identifiers"])
    );
}

#[tokio::test]
async fn resolve_needs_review_multiple_new_identifiers() {
    let (router, _) = test_app().await;

    let response = send_json(
        &router,
        Method::POST,
        "/api/tools/resolve",
        json!({
            "intake_version": "0.1.0",
            "name": "Multi",
            "tool_type": "generic",
            "urls": [
                "https://github.com/owner/repo",
                "https://example.com/tool"
            ]
        }),
    )
    .await;

    assert_eq!(response.0, StatusCode::OK);
    assert_eq!(response.1["status"], "needs_review");
    assert_eq!(
        response.1["reason_codes"],
        json!(["multiple_strong_identifiers"])
    );
}

#[tokio::test]
async fn resolve_needs_review_possible_fork() {
    let (router, _) = test_app().await;

    create_tool_via_api(
        &router,
        github_tool_request("owner", "framework", "Example Framework", "agent_framework"),
    )
    .await;

    let response = send_json(
        &router,
        Method::POST,
        "/api/tools/resolve",
        json!({
            "intake_version": "0.1.0",
            "name": "Example Framework",
            "tool_type": "agent_framework",
            "urls": ["https://github.com/community/fork"]
        }),
    )
    .await;

    assert_eq!(response.0, StatusCode::OK);
    assert_eq!(response.1["status"], "needs_review");
    assert_eq!(
        response.1["reason_codes"],
        json!(["possible_fork_or_source_migration"])
    );
    assert_eq!(
        response.1["candidate_tool_ids"],
        json!(["github:owner/framework"])
    );
}

#[tokio::test]
async fn resolve_needs_review_additional_identifier() {
    let (router, _) = test_app().await;

    create_tool_via_api(
        &router,
        github_tool_request("owner", "repo", "My Tool", "generic"),
    )
    .await;

    let response = send_json(
        &router,
        Method::POST,
        "/api/tools/resolve",
        json!({
            "intake_version": "0.1.0",
            "name": "Something Else",
            "tool_type": "generic",
            "urls": [
                "https://github.com/owner/repo",
                "https://example.com/mirror"
            ]
        }),
    )
    .await;

    assert_eq!(response.0, StatusCode::OK);
    assert_eq!(response.1["status"], "needs_review");
    assert_eq!(
        response.1["reason_codes"],
        json!(["additional_identifier_requires_review"])
    );
}

// ── Approved Migration ────────────────────────────────────────────

#[tokio::test]
async fn adds_identifier_to_existing_tool() {
    let (router, _) = test_app().await;

    create_tool_via_api(
        &router,
        github_tool_request("old-org", "tool", "My Tool", "generic"),
    )
    .await;

    let response = send_json(
        &router,
        Method::POST,
        "/api/tools/identifiers?tool_id=github:old-org/tool",
        json!({
            "identifier": {
                "namespace": "github",
                "value": "new-org/tool",
                "canonical_url": "https://github.com/new-org/tool"
            }
        }),
    )
    .await;

    assert_eq!(response.0, StatusCode::OK);
    assert_eq!(response.1["tool_id"], "github:old-org/tool");
    let identifiers = response.1["external_identifiers"].as_array().unwrap();
    assert_eq!(identifiers.len(), 2);

    // Resolving via either identifier should resolve.
    let resolve = send_json(
        &router,
        Method::POST,
        "/api/tools/resolve",
        json!({
            "intake_version": "0.1.0",
            "name": "Something",
            "tool_type": "generic",
            "urls": ["https://github.com/new-org/tool"]
        }),
    )
    .await;
    assert_eq!(resolve.0, StatusCode::OK);
    assert_eq!(resolve.1["status"], "resolved");
    assert_eq!(resolve.1["tool_id"], "github:old-org/tool");
}

// ── Error Tests ───────────────────────────────────────────────────

#[tokio::test]
async fn returns_json_error_for_missing_tool() {
    let (router, _) = test_app().await;

    let response = send(
        &router,
        Method::GET,
        "/api/tools/by-id?tool_id=github:nonexistent/tool",
        Body::empty(),
    )
    .await;
    assert_error(&response, StatusCode::NOT_FOUND, "tool_not_found");
}

#[tokio::test]
async fn rejects_invalid_resolve_request() {
    let (router, _) = test_app().await;

    // Wrong intake version.
    let response = send_json(
        &router,
        Method::POST,
        "/api/tools/resolve",
        json!({
            "intake_version": "0.0.0",
            "name": "Test",
            "tool_type": "generic",
            "urls": ["https://example.com/tool"]
        }),
    )
    .await;
    assert_error(&response, StatusCode::BAD_REQUEST, "invalid_intake_version");

    // Empty name.
    let response = send_json(
        &router,
        Method::POST,
        "/api/tools/resolve",
        json!({
            "intake_version": "0.1.0",
            "name": " ",
            "tool_type": "generic",
            "urls": []
        }),
    )
    .await;
    assert_error(&response, StatusCode::BAD_REQUEST, "invalid_request");

    // Invalid JSON.
    let response = send(
        &router,
        Method::POST,
        "/api/tools/resolve",
        Body::from("{invalid"),
    )
    .await;
    assert_error(&response, StatusCode::BAD_REQUEST, "invalid_json");
}
