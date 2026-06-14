mod error;

use axum::{
    Json, Router,
    extract::DefaultBodyLimit,
    extract::{Multipart, Path, Query, State, rejection::JsonRejection},
    http::StatusCode,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::{
    domain::{
        AddIdentifierRequest, AppendRunEventRequest, Artifact, CheckResults,
        CheckResultsSubmission, CreateArtifactRequest, CreateEvidenceRequest, CreateRunRequest,
        CreateToolRequest, Evidence, ResolveToolRequest, Run, RunDetails, RunEvent, Tool,
    },
    repository::Repository,
    services::{DEFAULT_MAX_STORED_BYTES, ServiceError, StorageService, TrustCoreService},
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

#[derive(Debug, Serialize)]
struct ArtifactListResponse {
    artifacts: Vec<Artifact>,
}

#[derive(Debug, Serialize)]
struct EvidenceListResponse {
    evidence: Vec<Evidence>,
}

#[derive(Debug, Deserialize)]
struct ToolQueryParams {
    tool_id: String,
}

pub fn app(pool: SqlitePool) -> Router {
    app_with_storage(
        pool,
        StorageService::new("../runs", DEFAULT_MAX_STORED_BYTES),
    )
}

pub fn app_with_storage(pool: SqlitePool, storage: StorageService) -> Router {
    let body_limit = storage.max_bytes() + 64 * 1024;
    let state = AppState {
        service: TrustCoreService::new(Repository::new(pool), storage),
    };

    Router::new()
        .route("/health", get(health))
        .route("/api/runs", post(create_run).get(list_runs))
        .route("/api/runs/{run_id}", get(get_run))
        .route("/api/runs/{run_id}/events", post(append_event))
        .route(
            "/api/runs/{run_id}/check-results",
            post(create_check_results),
        )
        .route(
            "/api/runs/{run_id}/artifacts",
            post(upload_artifact).get(list_artifacts),
        )
        .route(
            "/api/runs/{run_id}/evidence",
            post(upload_evidence).get(list_evidence),
        )
        .route("/api/tools", post(create_tool).get(list_tools))
        .route("/api/tools/by-id", get(get_tool_by_query))
        .route("/api/tools/resolve", post(resolve_tool))
        .route("/api/tools/identifiers", post(add_identifier))
        .fallback(error::not_found)
        .method_not_allowed_fallback(error::method_not_allowed)
        .layer(DefaultBodyLimit::max(body_limit))
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

async fn create_check_results(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
    payload: Result<Json<CheckResultsSubmission>, JsonRejection>,
) -> ApiResult<(StatusCode, Json<CheckResults>)> {
    let Json(submission) = payload.map_err(ApiError::invalid_json)?;
    let check_results = state
        .service
        .create_check_results(&run_id, submission)
        .await?;
    Ok((StatusCode::CREATED, Json(check_results)))
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

async fn upload_artifact(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
    mut multipart: Multipart,
) -> ApiResult<(StatusCode, Json<Artifact>)> {
    let mut filename = None;
    let mut content_type = None;
    let mut data = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(ApiError::invalid_multipart)?
    {
        let name = field.name().unwrap_or_default().to_string();
        if name == "file" {
            filename = field.file_name().map(|s| s.to_string());
            content_type = field.content_type().map(|s| s.to_string());
            data = Some(field.bytes().await.map_err(ApiError::invalid_multipart)?);
        }
    }

    let filename =
        filename.ok_or_else(|| ServiceError::InvalidRequest("file is required".into()))?;
    let content_type = content_type.unwrap_or_else(|| "application/octet-stream".into());
    let data =
        data.ok_or_else(|| ServiceError::InvalidRequest("file content is required".into()))?;

    let artifact = state
        .service
        .create_artifact(
            &run_id,
            CreateArtifactRequest {
                filename,
                content_type,
            },
            &data,
        )
        .await?;

    Ok((StatusCode::CREATED, Json(artifact)))
}

async fn list_artifacts(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> ApiResult<Json<ArtifactListResponse>> {
    let artifacts = state.service.list_artifacts(&run_id).await?;
    Ok(Json(ArtifactListResponse { artifacts }))
}

async fn upload_evidence(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
    payload: Result<Json<CreateEvidenceRequest>, JsonRejection>,
) -> ApiResult<(StatusCode, Json<Evidence>)> {
    let Json(request) = payload.map_err(ApiError::invalid_json)?;
    let evidence = state.service.create_evidence(&run_id, request).await?;

    Ok((StatusCode::CREATED, Json(evidence)))
}

async fn list_evidence(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> ApiResult<Json<EvidenceListResponse>> {
    let evidence = state.service.list_evidence(&run_id).await?;
    Ok(Json(EvidenceListResponse { evidence }))
}
