mod error;

use axum::{
    Json, Router,
    extract::{Path, Query, State, rejection::JsonRejection},
    http::StatusCode,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::{
    domain::{
        AddIdentifierRequest, AppendRunEventRequest, CreateRunRequest, CreateToolRequest,
        ResolveToolRequest, Run, RunDetails, RunEvent, Tool,
    },
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

#[derive(Debug, Serialize)]
struct ToolListResponse {
    tools: Vec<Tool>,
}

#[derive(Debug, Deserialize)]
struct ToolQueryParams {
    tool_id: String,
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
        .route("/api/tools", post(create_tool).get(list_tools))
        .route("/api/tools/by-id", get(get_tool_by_query))
        .route("/api/tools/resolve", post(resolve_tool))
        .route("/api/tools/identifiers", post(add_identifier))
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

async fn create_tool(
    State(state): State<AppState>,
    payload: Result<Json<CreateToolRequest>, JsonRejection>,
) -> ApiResult<(StatusCode, Json<Tool>)> {
    let Json(request) = payload.map_err(ApiError::invalid_json)?;
    let tool = state.service.create_tool(request).await?;
    Ok((StatusCode::CREATED, Json(tool)))
}

async fn list_tools(State(state): State<AppState>) -> ApiResult<Json<ToolListResponse>> {
    let tools = state.service.list_tools().await?;
    Ok(Json(ToolListResponse { tools }))
}

async fn get_tool_by_query(
    State(state): State<AppState>,
    Query(params): Query<ToolQueryParams>,
) -> ApiResult<Json<Tool>> {
    let tool = state.service.get_tool(&params.tool_id).await?;
    Ok(Json(tool))
}

async fn resolve_tool(
    State(state): State<AppState>,
    payload: Result<Json<ResolveToolRequest>, JsonRejection>,
) -> ApiResult<Json<crate::domain::ResolutionResponse>> {
    let Json(request) = payload.map_err(ApiError::invalid_json)?;
    let resolution = state.service.resolve_tool(request).await?;
    Ok(Json(resolution))
}

async fn add_identifier(
    State(state): State<AppState>,
    Query(params): Query<ToolQueryParams>,
    payload: Result<Json<AddIdentifierRequest>, JsonRejection>,
) -> ApiResult<Json<Tool>> {
    let Json(request) = payload.map_err(ApiError::invalid_json)?;
    let tool = state
        .service
        .add_identifier(&params.tool_id, request)
        .await?;
    Ok(Json(tool))
}
