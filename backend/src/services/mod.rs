use chrono::Utc;
use thiserror::Error;
use uuid::Uuid;

use crate::{
    domain::{AppendRunEventRequest, CreateRunRequest, Run, RunDetails, RunEvent, RunStatus},
    repository::{Repository, RepositoryError},
};

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error("run ID must be a UUID")]
    InvalidRunId,
    #[error("{0}")]
    InvalidRequest(String),
    #[error("run not found")]
    RunNotFound,
    #[error(transparent)]
    Repository(#[from] RepositoryError),
}

#[derive(Clone)]
pub struct TrustCoreService {
    repository: Repository,
}

impl TrustCoreService {
    pub const fn new(repository: Repository) -> Self {
        Self { repository }
    }

    pub async fn create_run(&self, request: CreateRunRequest) -> Result<Run, ServiceError> {
        validate_create_run(&request)?;

        let now = Utc::now();
        let run = Run {
            run_id: Uuid::new_v4(),
            goal: request.goal.trim().to_owned(),
            tool: request.tool,
            status: RunStatus::Pending,
            current_node: None,
            created_at: now,
            updated_at: now,
        };

        self.repository.create_run(&run).await.map_err(Into::into)
    }

    pub async fn list_runs(&self) -> Result<Vec<Run>, ServiceError> {
        self.repository.list_runs().await.map_err(Into::into)
    }

    pub async fn get_run_details(&self, run_id: &str) -> Result<RunDetails, ServiceError> {
        let run_id = parse_run_id(run_id)?;
        let run = self
            .repository
            .get_run(run_id)
            .await?
            .ok_or(ServiceError::RunNotFound)?;
        let events = self.repository.list_events(run_id).await?;

        Ok(RunDetails { run, events })
    }

    pub async fn append_event(
        &self,
        run_id: &str,
        request: AppendRunEventRequest,
    ) -> Result<RunEvent, ServiceError> {
        let run_id = parse_run_id(run_id)?;
        validate_append_event(&request)?;

        if self.repository.get_run(run_id).await?.is_none() {
            return Err(ServiceError::RunNotFound);
        }

        let event = RunEvent {
            event_id: Uuid::new_v4(),
            run_id,
            sequence: 0,
            node_id: request.node_id.trim().to_owned(),
            event_type: request.event_type,
            payload: request.payload,
            created_at: Utc::now(),
        };

        self.repository
            .append_event(&event)
            .await
            .map_err(Into::into)
    }
}

fn parse_run_id(run_id: &str) -> Result<Uuid, ServiceError> {
    Uuid::parse_str(run_id).map_err(|_| ServiceError::InvalidRunId)
}

fn validate_create_run(request: &CreateRunRequest) -> Result<(), ServiceError> {
    validate_required("goal", &request.goal, 4_000)?;
    validate_required("tool.name", &request.tool.name, 200)?;
    validate_required("tool.tool_type", &request.tool.tool_type, 100)?;

    if request.tool.urls.len() > 20 {
        return Err(ServiceError::InvalidRequest(
            "tool.urls must contain at most 20 entries".to_owned(),
        ));
    }

    for url in &request.tool.urls {
        validate_required("tool.urls[]", url, 2_048)?;
    }

    Ok(())
}

fn validate_append_event(request: &AppendRunEventRequest) -> Result<(), ServiceError> {
    validate_required("node_id", &request.node_id, 200)
}

fn validate_required(field: &str, value: &str, max_length: usize) -> Result<(), ServiceError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(ServiceError::InvalidRequest(format!(
            "{field} must not be empty"
        )));
    }
    if value.chars().count() > max_length {
        return Err(ServiceError::InvalidRequest(format!(
            "{field} must contain at most {max_length} characters"
        )));
    }
    Ok(())
}
