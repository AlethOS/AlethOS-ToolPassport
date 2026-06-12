use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::Value;
use sqlx::sqlite::SqlitePoolOptions;
use toolpassport_backend::{app, migrate};
use tower::ServiceExt;

#[tokio::test]
async fn health_returns_json_status() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("test database must connect");
    migrate(&pool).await.expect("migrations must run");

    let response = app(pool)
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .expect("request must build"),
        )
        .await
        .expect("health request must complete");

    assert_eq!(response.status(), StatusCode::OK);

    let body = response
        .into_body()
        .collect()
        .await
        .expect("response body must collect")
        .to_bytes();
    let payload: Value = serde_json::from_slice(&body).expect("health response must be JSON");

    assert_eq!(payload["status"], "ok");
    assert_eq!(payload["service"], "toolpassport-backend");
}
