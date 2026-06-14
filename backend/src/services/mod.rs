use chrono::Utc;
use serde_json::{Map, Value, json};
use thiserror::Error;
use uuid::Uuid;

use crate::{
    domain::{
        AddIdentifierRequest, AppendRunEventRequest, Artifact, CreateArtifactRequest,
        CreateEvidenceRequest, CreateRunRequest, CreateToolRequest, Evidence, ExternalIdentifier,
        ReasonCode, ResolutionResponse, ResolutionStatus, ResolveToolRequest, Run, RunDetails,
        RunEvent, RunEventType, RunStatus, Tool, ToolInput, ZERO_HASH,
    },
    repository::{Repository, RepositoryError},
};

mod normalizer;
mod scoring;
mod storage;

pub use scoring::{ScoringError, score_check_results};
pub use storage::{DEFAULT_MAX_STORED_BYTES, StorageError, StorageService};

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error("run ID must be a UUID")]
    InvalidRunId,
    #[error("{0}")]
    InvalidRequest(String),
    #[error("run not found")]
    RunNotFound,
    #[error("{0}")]
    Conflict(String),
    #[error("tool not found")]
    ToolNotFound,
    #[error("invalid tool ID format")]
    InvalidToolIdFormat,
    #[error("tool already exists")]
    ToolAlreadyExists,
    #[error("external identifier already claimed by {0}")]
    IdentifierAlreadyClaimed(String),
    #[error("invalid URL: {0}")]
    InvalidUrl(String),
    #[error("invalid intake version")]
    InvalidIntakeVersion,
    #[error(transparent)]
    Repository(#[from] RepositoryError),
    #[error(transparent)]
    Storage(#[from] StorageError),
}

#[derive(Clone)]
pub struct TrustCoreService {
    repository: Repository,
    storage: StorageService,
}

impl TrustCoreService {
    pub fn new(repository: Repository, storage: StorageService) -> Self {
        Self {
            repository,
            storage,
        }
    }

    pub async fn create_run(&self, request: CreateRunRequest) -> Result<Run, ServiceError> {
        validate_create_run(&request)?;

        let tool_id = request.tool_id.trim().to_owned();

        // Load and freeze a snapshot from the Tool Registry.
        let tool = self
            .repository
            .get_tool(&tool_id)
            .await?
            .ok_or(ServiceError::ToolNotFound)?;

        let now = Utc::now();
        let run = Run {
            run_id: Uuid::new_v4(),
            goal: request.goal.trim().to_owned(),
            tool_id: tool.tool_id.clone(),
            canonical_url: tool.canonical_url.clone(),
            tool: ToolInput {
                name: tool.name.clone(),
                tool_type: tool.tool_type.as_str().to_owned(),
                urls: vec![tool.canonical_url.clone()],
            },
            status: RunStatus::Pending,
            current_node: None,
            created_at: now,
            updated_at: now,
        };
        let created_event = RunEvent {
            event_id: Uuid::new_v4(),
            run_id: run.run_id,
            sequence: 0,
            node_id: "run".to_owned(),
            event_type: RunEventType::RunCreated,
            payload: Map::from_iter([(
                "status".to_owned(),
                Value::String(RunStatus::Pending.as_str().to_owned()),
            )]),
            created_at: now,
            event_hash: String::new(), // computed by repository
            prev_event_hash: ZERO_HASH.to_owned(),
        };

        self.repository
            .create_run(&run, &created_event)
            .await
            .map_err(Into::into)
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

        let run = self
            .repository
            .get_run(run_id)
            .await?
            .ok_or(ServiceError::RunNotFound)?;
        let projection = event_projection(&run, &request)?;

        let event = RunEvent {
            event_id: Uuid::new_v4(),
            run_id,
            sequence: 0,
            node_id: request.node_id.trim().to_owned(),
            event_type: request.event_type,
            payload: request.payload,
            created_at: Utc::now(),
            event_hash: String::new(),      // computed by repository
            prev_event_hash: String::new(), // computed by repository
        };

        self.repository
            .append_event(
                &event,
                run.status,
                projection.status,
                projection.current_node.as_deref(),
            )
            .await
            .map_err(|error| match error {
                RepositoryError::RunStateChanged => ServiceError::Conflict(
                    "run state changed while appending the event; reload and retry".to_owned(),
                ),
                other => ServiceError::Repository(other),
            })
    }

    // ── Tool Registry ─────────────────────────────────────────────

    pub async fn create_tool(&self, request: CreateToolRequest) -> Result<Tool, ServiceError> {
        validate_create_tool(&request)?;

        let now = Utc::now();
        let tool = Tool {
            tool_schema_version: "0.1.0",
            tool_id: request.tool_id.trim().to_owned(),
            name: request.name.trim().to_owned(),
            tool_type: request.tool_type,
            canonical_url: request.canonical_url.trim().to_owned(),
            external_identifiers: request.external_identifiers,
            aliases: request.aliases,
            created_at: now,
            updated_at: now,
        };

        self.repository
            .create_tool(&tool)
            .await
            .map_err(|error| match error {
                RepositoryError::UniqueViolation => ServiceError::ToolAlreadyExists,
                other => ServiceError::Repository(other),
            })
    }

    pub async fn get_tool(&self, tool_id: &str) -> Result<Tool, ServiceError> {
        self.repository
            .get_tool(tool_id)
            .await?
            .ok_or(ServiceError::ToolNotFound)
    }

    pub async fn list_tools(&self) -> Result<Vec<Tool>, ServiceError> {
        self.repository.list_tools().await.map_err(Into::into)
    }

    pub async fn resolve_tool(
        &self,
        request: ResolveToolRequest,
    ) -> Result<ResolutionResponse, ServiceError> {
        if request.intake_version != "0.1.0" {
            return Err(ServiceError::InvalidIntakeVersion);
        }
        validate_required("name", &request.name, 200)?;

        // Normalize all URLs, tracking invalid ones.
        let mut normalized_by_key: Vec<(String, normalizer::NormalizedIdentifier)> = Vec::new();
        let mut invalid_url = false;

        for raw_url in &request.urls {
            match normalizer::normalize_url(raw_url) {
                Some(normalized) => {
                    let key = normalized.key();
                    if !normalized_by_key.iter().any(|(k, _)| k == &key) {
                        normalized_by_key.push((key, normalized));
                    }
                }
                None => invalid_url = true,
            }
        }

        // Sort by key for deterministic output.
        normalized_by_key.sort_by(|a, b| a.0.cmp(&b.0));
        let normalized_identifiers: Vec<ExternalIdentifier> = normalized_by_key
            .iter()
            .map(|(_, n)| n.to_external_identifier())
            .collect();

        // Find which existing tools own any of the normalized identifier keys.
        let keys: Vec<String> = normalized_by_key.iter().map(|(k, _)| k.clone()).collect();
        let matched_tools = self.repository.find_tools_by_identifiers(&keys).await?;
        let matched_tool_ids: std::collections::HashSet<String> =
            matched_tools.iter().map(|t| t.tool_id.clone()).collect();

        // Find tools whose name or aliases match.
        let name_candidates = self.repository.find_tools_by_name(&request.name).await?;
        let name_candidate_ids: std::collections::HashSet<String> =
            name_candidates.iter().map(|t| t.tool_id.clone()).collect();

        let candidate_tool_ids: Vec<String> = {
            let mut all: Vec<String> = matched_tool_ids.iter().cloned().collect();
            all.extend(name_candidate_ids.iter().cloned());
            all.sort();
            all.dedup();
            all
        };

        // Decision tree — direct port of Python resolve_identity().
        if invalid_url {
            return Ok(build_resolution(
                ResolutionStatus::NeedsReview,
                None,
                normalized_identifiers,
                candidate_tool_ids,
                &[ReasonCode::InvalidOrAmbiguousUrl],
            ));
        }
        if matched_tool_ids.len() > 1 {
            return Ok(build_resolution(
                ResolutionStatus::NeedsReview,
                None,
                normalized_identifiers,
                candidate_tool_ids,
                &[ReasonCode::ConflictingExistingIdentifiers],
            ));
        }
        if matched_tool_ids.len() == 1 {
            let matched_id = matched_tool_ids.iter().next().unwrap();
            // Check for unmatched keys.
            let owned_keys: std::collections::HashSet<String> = matched_tools
                .iter()
                .flat_map(|t| t.external_identifiers.iter())
                .map(|id| format!("{}:{}", id.namespace, id.value))
                .collect();
            let unmatched: bool = keys.iter().any(|k| !owned_keys.contains(k));
            if unmatched {
                return Ok(build_resolution(
                    ResolutionStatus::NeedsReview,
                    None,
                    normalized_identifiers,
                    candidate_tool_ids,
                    &[ReasonCode::AdditionalIdentifierRequiresReview],
                ));
            }
            return Ok(build_resolution(
                ResolutionStatus::Resolved,
                Some(matched_id.clone()),
                normalized_identifiers,
                candidate_tool_ids,
                &[ReasonCode::ExistingIdentifierMatch],
            ));
        }
        if normalized_identifiers.is_empty() {
            let reason = if !name_candidate_ids.is_empty() {
                ReasonCode::NameMatchOnly
            } else {
                ReasonCode::NameOnly
            };
            return Ok(build_resolution(
                ResolutionStatus::NeedsReview,
                None,
                normalized_identifiers,
                candidate_tool_ids,
                &[reason],
            ));
        }
        if normalized_identifiers.len() > 1 {
            return Ok(build_resolution(
                ResolutionStatus::NeedsReview,
                None,
                normalized_identifiers,
                candidate_tool_ids,
                &[ReasonCode::MultipleStrongIdentifiers],
            ));
        }
        if !name_candidate_ids.is_empty() {
            return Ok(build_resolution(
                ResolutionStatus::NeedsReview,
                None,
                normalized_identifiers,
                candidate_tool_ids,
                &[ReasonCode::PossibleForkOrSourceMigration],
            ));
        }

        let proposed_tool_id = normalized_by_key[0].0.clone();
        Ok(build_resolution(
            ResolutionStatus::CreateCandidate,
            Some(proposed_tool_id),
            normalized_identifiers,
            candidate_tool_ids,
            &[ReasonCode::NewStrongIdentifier],
        ))
    }

    pub async fn add_identifier(
        &self,
        tool_id: &str,
        request: AddIdentifierRequest,
    ) -> Result<Tool, ServiceError> {
        // Verify the tool exists.
        let existing = self.get_tool(tool_id).await?;

        // Validate the new identifier is in canonical form.
        let normalized =
            normalizer::normalize_url(&request.identifier.canonical_url).ok_or_else(|| {
                ServiceError::InvalidUrl(format!(
                    "identifier canonical_url is not a valid strong URL: {}",
                    request.identifier.canonical_url
                ))
            })?;
        let expected_key = format!(
            "{}:{}",
            request.identifier.namespace, request.identifier.value
        );
        if normalized.key() != expected_key {
            return Err(ServiceError::InvalidRequest(
                "identifier namespace:value does not match its canonical_url normalization"
                    .to_owned(),
            ));
        }

        // Check it's not already owned by another tool.
        let existing_owners = self
            .repository
            .find_tools_by_identifiers(&[expected_key])
            .await?;
        if existing_owners
            .iter()
            .any(|t| t.tool_id != existing.tool_id)
        {
            let claimed_by = existing_owners
                .iter()
                .find(|t| t.tool_id != existing.tool_id)
                .map(|t| t.tool_id.clone())
                .unwrap_or_default();
            return Err(ServiceError::IdentifierAlreadyClaimed(claimed_by));
        }

        self.repository
            .add_external_id(tool_id, &request.identifier, Utc::now())
            .await
            .map_err(|error| match error {
                RepositoryError::UniqueViolation => {
                    ServiceError::IdentifierAlreadyClaimed(existing.tool_id.clone())
                }
                other => ServiceError::Repository(other),
            })
    }

    pub async fn create_artifact(
        &self,
        run_id: &str,
        request: CreateArtifactRequest,
        content: &[u8],
    ) -> Result<Artifact, ServiceError> {
        let run_id = parse_run_id(run_id)?;
        validate_artifact_request(&request, content)?;

        let _ = self
            .repository
            .get_run(run_id)
            .await?
            .ok_or(ServiceError::RunNotFound)?;

        let artifact_id = Uuid::new_v4();
        let stored = self
            .storage
            .save_artifact(run_id, artifact_id, content)
            .await?;
        let artifact = Artifact {
            artifact_schema_version: "0.1.0",
            artifact_id,
            run_id,
            filename: request.filename,
            content_type: request.content_type,
            size_bytes: stored.size_bytes,
            sha256_hash: stored.sha256_hash,
            storage_key: stored.storage_key,
            created_at: Utc::now(),
        };
        let event = generated_event(
            run_id,
            RunEventType::ArtifactCreated,
            json!({
                "artifact_id": artifact.artifact_id,
                "sha256_hash": artifact.sha256_hash,
                "size_bytes": artifact.size_bytes,
            }),
        );

        match self.repository.create_artifact(&artifact, &event).await {
            Ok(artifact) => Ok(artifact),
            Err(error) => {
                let _ = self.storage.remove_file(&artifact.storage_key).await;
                Err(error.into())
            }
        }
    }

    pub async fn list_artifacts(&self, run_id: &str) -> Result<Vec<Artifact>, ServiceError> {
        let run_id = parse_run_id(run_id)?;
        ensure_run_exists(&self.repository, run_id).await?;
        self.repository
            .list_artifacts(run_id)
            .await
            .map_err(Into::into)
    }

    pub async fn create_evidence(
        &self,
        run_id: &str,
        request: CreateEvidenceRequest,
    ) -> Result<Evidence, ServiceError> {
        let run_id = parse_run_id(run_id)?;
        validate_evidence_request(&request)?;

        ensure_run_exists(&self.repository, run_id).await?;
        if let Some(artifact_id) = request.snapshot_artifact_id {
            let artifact = self
                .repository
                .get_artifact(artifact_id)
                .await?
                .ok_or_else(|| {
                    ServiceError::InvalidRequest(
                        "snapshot_artifact_id must reference an artifact in this run".to_owned(),
                    )
                })?;
            if artifact.run_id != run_id {
                return Err(ServiceError::InvalidRequest(
                    "snapshot_artifact_id must reference an artifact in this run".to_owned(),
                ));
            }
        }

        let evidence_id = Uuid::new_v4();
        let content = serde_json::to_vec(&request)
            .map_err(|error| ServiceError::InvalidRequest(error.to_string()))?;
        let stored = self
            .storage
            .save_evidence(run_id, evidence_id, &content)
            .await?;
        let evidence = Evidence {
            evidence_schema_version: "0.2.0",
            evidence_id,
            run_id,
            source_type: request.source_type,
            source_url: request.source_url,
            source_revision: request.source_revision,
            title: request.title,
            excerpt: request.excerpt,
            retrieved_at: request.retrieved_at,
            snapshot_artifact_id: request.snapshot_artifact_id,
            supports: request.supports,
            contradicts: request.contradicts,
            metadata: request.metadata,
            size_bytes: stored.size_bytes,
            content_hash: stored.sha256_hash,
            storage_key: stored.storage_key,
            created_at: Utc::now(),
        };
        let event = generated_event(
            run_id,
            RunEventType::EvidenceCreated,
            json!({
                "content_hash": evidence.content_hash,
                "evidence_id": evidence.evidence_id,
                "snapshot_artifact_id": evidence.snapshot_artifact_id,
            }),
        );

        match self.repository.create_evidence(&evidence, &event).await {
            Ok(evidence) => Ok(evidence),
            Err(error) => {
                let _ = self.storage.remove_file(&evidence.storage_key).await;
                Err(error.into())
            }
        }
    }

    pub async fn list_evidence(&self, run_id: &str) -> Result<Vec<Evidence>, ServiceError> {
        let run_id = parse_run_id(run_id)?;
        ensure_run_exists(&self.repository, run_id).await?;
        self.repository
            .list_evidence(run_id)
            .await
            .map_err(Into::into)
    }
}

struct EventProjection {
    status: Option<RunStatus>,
    current_node: Option<String>,
}

fn event_projection(
    run: &Run,
    request: &AppendRunEventRequest,
) -> Result<EventProjection, ServiceError> {
    let current_node = || Some(request.node_id.trim().to_owned());

    match request.event_type {
        RunEventType::RunCreated => Err(ServiceError::InvalidRequest(
            "run_created is generated by the Rust trust core".to_owned(),
        )),
        RunEventType::NodeStarted => {
            ensure_status(run.status, &[RunStatus::Pending, RunStatus::Running])?;
            Ok(EventProjection {
                status: (run.status == RunStatus::Pending).then_some(RunStatus::Running),
                current_node: current_node(),
            })
        }
        RunEventType::NodeFinished => {
            ensure_status(run.status, &[RunStatus::Running])?;
            Ok(EventProjection {
                status: None,
                current_node: current_node(),
            })
        }
        RunEventType::ApprovalRequired => {
            ensure_status(run.status, &[RunStatus::Running])?;
            Ok(EventProjection {
                status: Some(RunStatus::WaitingApproval),
                current_node: current_node(),
            })
        }
        RunEventType::ApprovalResolved => {
            ensure_status(run.status, &[RunStatus::WaitingApproval])?;
            Ok(EventProjection {
                status: Some(RunStatus::Running),
                current_node: current_node(),
            })
        }
        RunEventType::RunStatusChanged => {
            let status = requested_terminal_status(&request.payload)?;
            ensure_terminal_transition(run.status, status)?;
            Ok(EventProjection {
                status: Some(status),
                current_node: current_node(),
            })
        }
        _ => Ok(EventProjection {
            status: None,
            current_node: None,
        }),
    }
}

fn requested_terminal_status(payload: &Map<String, Value>) -> Result<RunStatus, ServiceError> {
    let status = payload
        .get("status")
        .and_then(Value::as_str)
        .and_then(RunStatus::parse)
        .ok_or_else(|| {
            ServiceError::InvalidRequest(
                "run_status_changed payload.status must be success or failed".to_owned(),
            )
        })?;

    match status {
        RunStatus::Success | RunStatus::Failed => Ok(status),
        _ => Err(ServiceError::InvalidRequest(
            "run_status_changed payload.status must be success or failed".to_owned(),
        )),
    }
}

fn ensure_terminal_transition(from: RunStatus, to: RunStatus) -> Result<(), ServiceError> {
    let allowed = matches!(
        (from, to),
        (RunStatus::Pending, RunStatus::Failed)
            | (RunStatus::Running, RunStatus::Success | RunStatus::Failed)
            | (RunStatus::WaitingApproval, RunStatus::Failed)
    );

    if allowed {
        Ok(())
    } else {
        Err(ServiceError::InvalidRequest(format!(
            "run status cannot transition from {} to {}",
            from.as_str(),
            to.as_str()
        )))
    }
}

fn ensure_status(current: RunStatus, allowed: &[RunStatus]) -> Result<(), ServiceError> {
    if allowed.contains(&current) {
        Ok(())
    } else {
        Err(ServiceError::InvalidRequest(format!(
            "event is not allowed while run status is {}",
            current.as_str()
        )))
    }
}

fn parse_run_id(run_id: &str) -> Result<Uuid, ServiceError> {
    Uuid::parse_str(run_id).map_err(|_| ServiceError::InvalidRunId)
}

async fn ensure_run_exists(repository: &Repository, run_id: Uuid) -> Result<(), ServiceError> {
    repository
        .get_run(run_id)
        .await?
        .ok_or(ServiceError::RunNotFound)?;
    Ok(())
}

fn generated_event(run_id: Uuid, event_type: RunEventType, payload: Value) -> RunEvent {
    RunEvent {
        event_id: Uuid::new_v4(),
        run_id,
        sequence: 0,
        node_id: "trust_core".to_owned(),
        event_type,
        payload: payload.as_object().cloned().unwrap_or_default(),
        created_at: Utc::now(),
        event_hash: String::new(),      // computed by repository
        prev_event_hash: String::new(), // computed by repository
    }
}

fn validate_artifact_request(
    request: &CreateArtifactRequest,
    content: &[u8],
) -> Result<(), ServiceError> {
    validate_required("filename", &request.filename, 255)?;
    validate_required("content_type", &request.content_type, 255)?;
    if request.filename.contains('/')
        || request.filename.contains('\\')
        || request.filename.contains("..")
        || request.filename.chars().any(char::is_control)
    {
        return Err(ServiceError::InvalidRequest(
            "filename must be a plain display name without path components".to_owned(),
        ));
    }
    if content.is_empty() {
        return Err(ServiceError::InvalidRequest(
            "artifact content must not be empty".to_owned(),
        ));
    }
    Ok(())
}

fn validate_evidence_request(request: &CreateEvidenceRequest) -> Result<(), ServiceError> {
    if request.evidence_schema_version != "0.2.0" {
        return Err(ServiceError::InvalidRequest(
            "evidence_schema_version must be 0.2.0".to_owned(),
        ));
    }
    validate_required("source_url", &request.source_url, 2_048)?;
    if request
        .source_url
        .strip_prefix("https://")
        .is_none_or(|remainder| remainder.trim().is_empty())
    {
        return Err(ServiceError::InvalidRequest(
            "source_url must use HTTPS".to_owned(),
        ));
    }
    validate_required("title", &request.title, 500)?;
    if request.excerpt.chars().count() > 20_000 {
        return Err(ServiceError::InvalidRequest(
            "excerpt must contain at most 20000 characters".to_owned(),
        ));
    }
    if let Some(revision) = &request.source_revision {
        validate_required("source_revision", revision, 500)?;
    }
    validate_reference_list("supports", &request.supports)?;
    validate_reference_list("contradicts", &request.contradicts)?;
    Ok(())
}

fn validate_reference_list(field: &str, values: &[String]) -> Result<(), ServiceError> {
    if values.len() > 200 {
        return Err(ServiceError::InvalidRequest(format!(
            "{field} must contain at most 200 entries"
        )));
    }
    let mut unique = std::collections::HashSet::new();
    for value in values {
        validate_required(field, value, 500)?;
        if !unique.insert(value) {
            return Err(ServiceError::InvalidRequest(format!(
                "{field} entries must be unique"
            )));
        }
    }
    Ok(())
}

fn validate_create_run(request: &CreateRunRequest) -> Result<(), ServiceError> {
    validate_required("goal", &request.goal, 4_000)?;
    validate_required("tool_id", &request.tool_id, 200)?;

    if !normalizer::validate_tool_id_format(request.tool_id.trim()) {
        return Err(ServiceError::InvalidToolIdFormat);
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

fn validate_create_tool(request: &CreateToolRequest) -> Result<(), ServiceError> {
    if !normalizer::validate_tool_id_format(&request.tool_id) {
        return Err(ServiceError::InvalidToolIdFormat);
    }

    validate_required("name", &request.name, 200)?;
    validate_required("canonical_url", &request.canonical_url, 2_048)?;

    if request.external_identifiers.is_empty() {
        return Err(ServiceError::InvalidRequest(
            "external_identifiers must contain at least one entry".to_owned(),
        ));
    }
    if request.external_identifiers.len() > 20 {
        return Err(ServiceError::InvalidRequest(
            "external_identifiers must contain at most 20 entries".to_owned(),
        ));
    }

    // Each identifier must be in canonical form.
    for (index, identifier) in request.external_identifiers.iter().enumerate() {
        let normalized = normalizer::normalize_url(&identifier.canonical_url).ok_or_else(|| {
            ServiceError::InvalidUrl(format!(
                "external_identifiers[{index}].canonical_url is not a valid strong URL"
            ))
        })?;
        let expected_key = format!("{}:{}", identifier.namespace, identifier.value);
        if normalized.key() != expected_key {
            return Err(ServiceError::InvalidRequest(format!(
                "external_identifiers[{index}]: namespace:value does not match canonical_url normalization"
            )));
        }
    }

    // tool_id must match exactly one external identifier key.
    let id_keys: Vec<String> = request
        .external_identifiers
        .iter()
        .map(|id| format!("{}:{}", id.namespace, id.value))
        .collect();
    let tool_id_matches = id_keys.iter().filter(|k| *k == &request.tool_id).count();
    if tool_id_matches != 1 {
        return Err(ServiceError::InvalidRequest(
            "tool_id must match exactly one external identifier namespace:value".to_owned(),
        ));
    }

    // canonical_url must match one external identifier's canonical_url.
    let url_matches = request
        .external_identifiers
        .iter()
        .filter(|id| id.canonical_url.trim() == request.canonical_url.trim())
        .count();
    if url_matches == 0 {
        return Err(ServiceError::InvalidRequest(
            "canonical_url must match one external identifier's canonical_url".to_owned(),
        ));
    }

    // Aliases must be unique ignoring case.
    let alias_lower: Vec<String> = request
        .aliases
        .iter()
        .map(|a| a.to_ascii_lowercase())
        .collect();
    if alias_lower.len()
        != alias_lower
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len()
    {
        return Err(ServiceError::InvalidRequest(
            "aliases must be unique ignoring case".to_owned(),
        ));
    }

    for alias in &request.aliases {
        validate_required("alias", alias, 200)?;
    }

    Ok(())
}

fn build_resolution(
    status: ResolutionStatus,
    tool_id: Option<String>,
    normalized_identifiers: Vec<ExternalIdentifier>,
    candidate_tool_ids: Vec<String>,
    reason_codes: &[ReasonCode],
) -> ResolutionResponse {
    let mut codes: Vec<ReasonCode> = reason_codes.to_vec();
    codes.sort_by_key(|c| {
        // Stable sort order matching Python's sorted(set(...)) on string representation.
        serde_json::to_string(c).unwrap_or_default()
    });
    codes.dedup();
    ResolutionResponse {
        resolution_version: "0.1.0",
        status,
        normalized_identifiers,
        tool_id,
        candidate_tool_ids,
        reason_codes: codes,
    }
}
