mod error;

use axum::{
    Json, Router,
    extract::DefaultBodyLimit,
    extract::{Multipart, Path, Query, State, rejection::JsonRejection},
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
    routing::{get, post},
};
use futures::Stream;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::{convert::Infallible, sync::Arc};
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

use crate::{
    domain::{
        AddIdentifierRequest, AppendRunEventRequest, Approval, Artifact, AttestationPreflight,
        AttestationReceipt, CheckResults, CheckResultsSubmission, CreateApprovalRequest,
        CreateArtifactRequest, CreateEvidenceRequest, CreateRunRequest, CreateToolRequest,
        Evidence, EvidenceFreezeResult, FreezeEvidenceBoardRequest, FreezePassportRequest,
        PassportFreezeResult, ResolveToolRequest, Run, RunDetails, RunEvent, Tool,
    },
    repository::Repository,
    services::{
        AlloyAttestationSubmitter, AttestationSubmitter, DEFAULT_MAX_STORED_BYTES,
        EventBroadcaster, ServiceError, StorageService, TrustCoreService,
    },
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

#[derive(Debug, Serialize)]
struct EventListResponse {
    events: Vec<RunEvent>,
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
    app_with_storage_and_submitter(pool, storage, Arc::new(AlloyAttestationSubmitter))
}

pub fn app_with_storage_and_submitter(
    pool: SqlitePool,
    storage: StorageService,
    attestation_submitter: Arc<dyn AttestationSubmitter>,
) -> Router {
    let body_limit = storage.max_bytes() + 64 * 1024;
    let broadcaster = EventBroadcaster::new();
    let state = AppState {
        service: TrustCoreService::with_attestation_submitter(
            Repository::new(pool),
            storage,
            broadcaster,
            attestation_submitter,
        ),
    };

    Router::new()
        .route("/health", get(health))
        .route("/api/attestation/preflight", get(attestation_preflight))
        .route("/api/runs", post(create_run).get(list_runs))
        .route("/api/runs/{run_id}", get(get_run))
        .route(
            "/api/runs/{run_id}/events",
            post(append_event).get(list_run_events),
        )
        .route("/api/runs/{run_id}/events/stream", get(stream_run_events))
        .route("/api/runs/{run_id}/investigate", post(launch_investigation))
        .route(
            "/api/runs/{run_id}/check-results",
            post(create_check_results).get(get_latest_check_results),
        )
        .route(
            "/api/runs/{run_id}/evidence-board/freeze",
            post(freeze_evidence_board),
        )
        .route(
            "/api/runs/{run_id}/evidence-board/{version}",
            get(get_evidence_freeze),
        )
        .route("/api/runs/{run_id}/passport/freeze", post(freeze_passport))
        .route(
            "/api/runs/{run_id}/approval",
            post(create_approval).get(get_approval),
        )
        .route(
            "/api/runs/{run_id}/attestation",
            post(submit_attestation).get(get_attestation),
        )
        .route(
            "/api/runs/{run_id}/passport/{sequence}",
            get(get_passport_freeze),
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

async fn attestation_preflight(
    State(state): State<AppState>,
) -> ApiResult<Json<AttestationPreflight>> {
    Ok(Json(state.service.attestation_preflight().await?))
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

async fn freeze_evidence_board(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
    payload: Result<Json<FreezeEvidenceBoardRequest>, JsonRejection>,
) -> ApiResult<(StatusCode, Json<EvidenceFreezeResult>)> {
    let Json(request) = payload.map_err(ApiError::invalid_json)?;
    let freeze = state
        .service
        .freeze_evidence_board(&run_id, request)
        .await?;
    Ok((StatusCode::CREATED, Json(freeze)))
}

async fn get_evidence_freeze(
    State(state): State<AppState>,
    Path((run_id, version)): Path<(String, u64)>,
) -> ApiResult<Json<EvidenceFreezeResult>> {
    let freeze = state.service.get_evidence_freeze(&run_id, version).await?;
    Ok(Json(freeze))
}

async fn freeze_passport(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
    payload: Result<Json<FreezePassportRequest>, JsonRejection>,
) -> ApiResult<(StatusCode, Json<PassportFreezeResult>)> {
    let Json(request) = payload.map_err(ApiError::invalid_json)?;
    let freeze = state.service.freeze_passport(&run_id, request).await?;
    Ok((StatusCode::CREATED, Json(freeze)))
}

async fn get_passport_freeze(
    State(state): State<AppState>,
    Path((run_id, sequence)): Path<(String, u64)>,
) -> ApiResult<Json<PassportFreezeResult>> {
    let freeze = state.service.get_passport_freeze(&run_id, sequence).await?;
    Ok(Json(freeze))
}

async fn create_approval(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
    payload: Result<Json<CreateApprovalRequest>, JsonRejection>,
) -> ApiResult<(StatusCode, Json<Approval>)> {
    let Json(request) = payload.map_err(ApiError::invalid_json)?;
    let approval = state.service.create_approval(&run_id, request).await?;
    Ok((StatusCode::CREATED, Json(approval)))
}

async fn get_approval(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> ApiResult<Json<Approval>> {
    Ok(Json(state.service.get_approval(&run_id).await?))
}

async fn submit_attestation(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> ApiResult<(StatusCode, Json<AttestationReceipt>)> {
    let receipt = state.service.submit_attestation(&run_id).await?;
    Ok((StatusCode::CREATED, Json(receipt)))
}

async fn get_attestation(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> ApiResult<Json<AttestationReceipt>> {
    Ok(Json(state.service.get_attestation(&run_id).await?))
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

/// GET /api/runs/{run_id}/events
/// Returns all events for a run as a JSON array.
async fn list_run_events(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> ApiResult<Json<EventListResponse>> {
    let events = state.service.list_events_for_run(&run_id).await?;
    Ok(Json(EventListResponse { events }))
}

/// GET /api/runs/{run_id}/events/stream
/// Server-Sent Events stream that pushes new events as they are appended.
async fn stream_run_events(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> ApiResult<Sse<impl Stream<Item = Result<Event, Infallible>>>> {
    let run_id = uuid::Uuid::parse_str(&run_id).map_err(|_| ServiceError::InvalidRunId)?;

    // Ensure the run exists before opening the stream.
    let _ = state.service.get_run_details(&run_id.to_string()).await?;

    let rx = state.service.broadcaster().subscribe(run_id);
    let stream = BroadcastStream::new(rx).filter_map(|result| match result {
        Ok(event_json) => Some(Ok(Event::default().data(event_json).event("run_event"))),
        Err(_lagged) => {
            // If the receiver lagged behind, signal to the client.
            Some(Ok(Event::default().event("lagged").data(
                "{\"message\":\"event stream lagged; some events may have been missed\"}",
            )))
        }
    });

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("keep-alive"),
    ))
}

/// GET /api/runs/{run_id}/check-results
/// Returns the latest check results for a run (the one with the highest
/// evidence_board_version) or 404 if none have been computed yet.
async fn get_latest_check_results(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> ApiResult<Json<CheckResults>> {
    match state.service.get_latest_check_results(&run_id).await? {
        Some(results) => Ok(Json(results)),
        None => Err(ApiError::new(
            StatusCode::NOT_FOUND,
            "check_results_not_found",
            "no check results have been computed for this run",
            serde_json::json!({}),
        )),
    }
}

/// POST /api/runs/{run_id}/investigate
/// Launches the orchestrator subprocess for the given run.
/// The orchestrator reads BACKEND_URL and RUN_ID from the environment.
async fn launch_investigation(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let run = state.service.get_run_details(&run_id).await?;

    let backend_url =
        std::env::var("BACKEND_URL").unwrap_or_else(|_| "http://127.0.0.1:8080".to_owned());
    let python_cmd = std::env::var("ORCHESTRATOR_PYTHON").unwrap_or_else(|_| "python3".to_owned());
    let orch_dir =
        std::env::var("ORCHESTRATOR_DIR").unwrap_or_else(|_| "../orchestrator".to_owned());
    let live_research = std::env::var("ORCHESTRATOR_LIVE_RESEARCH").unwrap_or_default();
    let checkpoint_db = std::env::var("ORCHESTRATOR_CHECKPOINT_DB")
        .unwrap_or_else(|_| "../data/orchestrator-checkpoints.sqlite".to_owned());

    let mut cmd = tokio::process::Command::new(&python_cmd);
    cmd.arg("scripts/live_audit.py")
        .arg(&run.run.canonical_url)
        .env("BACKEND_URL", &backend_url)
        .env("RUN_ID", run.run.run_id.to_string())
        .env("PYTHONPATH", "src")
        .env("ORCHESTRATOR_LIVE_RESEARCH", &live_research)
        .env("CHECKPOINT_DB", &checkpoint_db)
        .env("LANGGRAPH_STRICT_MSGPACK", "true")
        .current_dir(&orch_dir)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());

    let mut child = cmd.spawn().map_err(|error| {
        ServiceError::InvalidRequest(format!("failed to launch orchestrator: {error}"))
    })?;
    let pid = child.id();
    tokio::spawn(async move {
        if let Err(error) = child.wait().await {
            tracing::warn!(%error, "failed to reap orchestrator process");
        }
    });

    Ok(Json(serde_json::json!({
        "status": "launched",
        "mode": "start_or_resume",
        "run_id": run.run.run_id.to_string(),
        "pid": pid,
    })))
}
