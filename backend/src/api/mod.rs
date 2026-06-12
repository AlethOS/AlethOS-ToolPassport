mod error;

use axum::{
    Json, Router,
    extract::{Path, State, rejection::JsonRejection},
    http::StatusCode,
    routing::{get, post},
};
use serde::Serialize;
use sqlx::SqlitePool;

use crate::{
    domain::{AppendRunEventRequest, CreateRunRequest, Run, RunDetails, RunEvent},
    repository::Repository,
    services::TrustCoreService,
};

use self::error::{ApiError, ApiResult};

#[derive(Clone)]
struct AppState {
    service: TrustCoreService,
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
    service: &'static str,
}

#[derive(Debug, Serialize)]
struct RunListResponse {
    runs: Vec<Run>,
}

pub fn app(pool: SqlitePool) -> Router {
    let state = AppState {
        service: TrustCoreService::new(Repository::new(pool)),
    };

    Router::new()
        .route("/health", get(health))
        .route("/api/runs", post(create_run).get(list_runs))
        .route("/api/runs/{run_id}", get(get_run))
        .route("/api/runs/{run_id}/events", post(append_event))
        .fallback(error::not_found)
        .method_not_allowed_fallback(error::method_not_allowed)
        .with_state(state)
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        service: "toolpassport-backend",
    })
}

async fn create_run(
    State(state): State<AppState>,
    payload: Result<Json<CreateRunRequest>, JsonRejection>,
) -> ApiResult<(StatusCode, Json<Run>)> {
    let Json(request) = payload.map_err(ApiError::invalid_json)?;
    let run = state.service.create_run(request).await?;
    Ok((StatusCode::CREATED, Json(run)))
}

async fn list_runs(State(state): State<AppState>) -> ApiResult<Json<RunListResponse>> {
    let runs = state.service.list_runs().await?;
    Ok(Json(RunListResponse { runs }))
}

async fn get_run(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> ApiResult<Json<RunDetails>> {
    let details = state.service.get_run_details(&run_id).await?;
    Ok(Json(details))
}

async fn append_event(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
    payload: Result<Json<AppendRunEventRequest>, JsonRejection>,
) -> ApiResult<(StatusCode, Json<RunEvent>)> {
    let Json(request) = payload.map_err(ApiError::invalid_json)?;
    let event = state.service.append_event(&run_id, request).await?;
    Ok((StatusCode::CREATED, Json(event)))
}
