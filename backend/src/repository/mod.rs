use std::{str::FromStr, time::Duration};

use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use sqlx::{
    FromRow, SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};
use std::ops::DerefMut;
use thiserror::Error;
use uuid::Uuid;

use crate::domain::{
    Approval, Artifact, AuditBinding, CheckResults, Evidence, EvidenceFreezeResult,
    EvidenceSourceType, ExternalIdentifier, FrozenEvidenceBoard, FrozenEvidenceManifest, Passport,
    PassportFreezeResult, Provenance, Run, RunEvent, RunEventType, RunStatus, Tool, ToolInput,
    ToolType, ZERO_HASH,
};

#[derive(Debug, Error)]
pub enum RepositoryError {
    #[error("database operation failed")]
    Database(#[from] sqlx::Error),
    #[error("stored data is invalid: {0}")]
    InvalidStoredData(String),
    #[error("run state changed")]
    RunStateChanged,
    #[error("database migration failed")]
    Migration(#[from] sqlx::migrate::MigrateError),
    #[error("unique constraint violation")]
    UniqueViolation,
}

#[derive(Clone)]
pub struct Repository {
    pool: SqlitePool,
}

impl Repository {
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create_run(
        &self,
        run: &Run,
        created_event: &RunEvent,
    ) -> Result<Run, RepositoryError> {
        let tool_urls = serde_json::to_string(&run.tool.urls)
            .map_err(|error| RepositoryError::InvalidStoredData(error.to_string()))?;
        let event_payload = serde_json::to_string(&created_event.payload)
            .map_err(|error| RepositoryError::InvalidStoredData(error.to_string()))?;
        let mut transaction = self.pool.begin().await?;

        sqlx::query(
            r#"
            INSERT INTO runs (
                run_id, goal, tool_id, canonical_url, tool_name, tool_type,
                tool_urls, standard_id, standard_version, profile_id, profile_version,
                status, current_node, created_at, updated_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(run.run_id.to_string())
        .bind(&run.goal)
        .bind(&run.tool_id)
        .bind(&run.canonical_url)
        .bind(&run.tool.name)
        .bind(&run.tool.tool_type)
        .bind(tool_urls)
        .bind(&run.audit_binding.standard_id)
        .bind(&run.audit_binding.standard_version)
        .bind(&run.audit_binding.profile_id)
        .bind(&run.audit_binding.profile_version)
        .bind(run.status.as_str())
        .bind(&run.current_node)
        .bind(format_timestamp(run.created_at))
        .bind(format_timestamp(run.updated_at))
        .execute(&mut *transaction)
        .await?;

        // First event: sequence = 1, prev_hash = zero.
        let run_id_str = run.run_id.to_string();
        let created_at_str = format_timestamp(created_event.created_at);
        let event_hash = compute_event_hash(
            &run_id_str,
            1,
            &created_event.node_id,
            created_event.event_type.as_str(),
            &created_event.payload,
            &created_at_str,
            ZERO_HASH,
        );

        sqlx::query(
            r#"
            INSERT INTO run_events (
                event_id, run_id, sequence, node_id, event_type, payload,
                created_at, event_hash, prev_event_hash
            )
            VALUES (?, ?, 1, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(created_event.event_id.to_string())
        .bind(&run_id_str)
        .bind(&created_event.node_id)
        .bind(created_event.event_type.as_str())
        .bind(event_payload)
        .bind(&created_at_str)
        .bind(&event_hash)
        .bind(ZERO_HASH)
        .execute(&mut *transaction)
        .await?;

        transaction.commit().await?;
        Ok(run.clone())
    }

    pub async fn list_runs(&self) -> Result<Vec<Run>, RepositoryError> {
        let rows = sqlx::query_as::<_, RunRow>(
            r#"
            SELECT run_id, goal, tool_id, canonical_url, tool_name, tool_type,
                   tool_urls, standard_id, standard_version, profile_id, profile_version,
                   status, current_node, created_at, updated_at
            FROM runs
            ORDER BY created_at DESC, run_id DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(TryInto::try_into).collect()
    }

    pub async fn get_run(&self, run_id: Uuid) -> Result<Option<Run>, RepositoryError> {
        let row = sqlx::query_as::<_, RunRow>(
            r#"
            SELECT run_id, goal, tool_id, canonical_url, tool_name, tool_type,
                   tool_urls, standard_id, standard_version, profile_id, profile_version,
                   status, current_node, created_at, updated_at
            FROM runs
            WHERE run_id = ?
            "#,
        )
        .bind(run_id.to_string())
        .fetch_optional(&self.pool)
        .await?;

        row.map(TryInto::try_into).transpose()
    }

    pub async fn append_event(
        &self,
        event: &RunEvent,
        expected_status: RunStatus,
        next_status: Option<RunStatus>,
        current_node: Option<&str>,
    ) -> Result<RunEvent, RepositoryError> {
        let payload = serde_json::to_string(&event.payload)
            .map_err(|error| RepositoryError::InvalidStoredData(error.to_string()))?;
        let mut transaction = self.pool.begin().await?;
        let next_status = next_status.map(RunStatus::as_str);
        let run_id_str = event.run_id.to_string();

        let update = sqlx::query(
            r#"
            UPDATE runs
            SET status = COALESCE(?, status),
                current_node = COALESCE(?, current_node),
                updated_at = ?
            WHERE run_id = ? AND status = ?
            "#,
        )
        .bind(next_status)
        .bind(current_node)
        .bind(format_timestamp(event.created_at))
        .bind(&run_id_str)
        .bind(expected_status.as_str())
        .execute(&mut *transaction)
        .await?;

        if update.rows_affected() != 1 {
            return Err(RepositoryError::RunStateChanged);
        }

        // Determine next sequence and previous hash before inserting.
        let (next_seq, prev_event_hash) =
            next_sequence_and_prev_hash(&mut transaction, &run_id_str).await;
        let created_at_str = format_timestamp(event.created_at);
        let event_hash = compute_event_hash(
            &run_id_str,
            next_seq,
            &event.node_id,
            event.event_type.as_str(),
            &event.payload,
            &created_at_str,
            &prev_event_hash,
        );

        let row = sqlx::query_as::<_, RunEventRow>(
            r#"
            INSERT INTO run_events (
                event_id, run_id, sequence, node_id, event_type, payload,
                created_at, event_hash, prev_event_hash
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            RETURNING event_id, run_id, sequence, node_id, event_type, payload,
                      created_at, event_hash, prev_event_hash
            "#,
        )
        .bind(event.event_id.to_string())
        .bind(&run_id_str)
        .bind(next_seq)
        .bind(&event.node_id)
        .bind(event.event_type.as_str())
        .bind(&payload)
        .bind(&created_at_str)
        .bind(&event_hash)
        .bind(&prev_event_hash)
        .fetch_one(&mut *transaction)
        .await?;

        transaction.commit().await?;
        row.try_into()
    }

    pub async fn list_events(&self, run_id: Uuid) -> Result<Vec<RunEvent>, RepositoryError> {
        let rows = sqlx::query_as::<_, RunEventRow>(
            r#"
            SELECT event_id, run_id, sequence, node_id, event_type, payload,
                   created_at, event_hash, prev_event_hash
            FROM run_events
            WHERE run_id = ?
            ORDER BY sequence ASC
            "#,
        )
        .bind(run_id.to_string())
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(TryInto::try_into).collect()
    }

    // ── Tool Registry ─────────────────────────────────────────────

    pub async fn create_tool(&self, tool: &Tool) -> Result<Tool, RepositoryError> {
        let identifiers_json = serde_json::to_string(&tool.external_identifiers)
            .map_err(|error| RepositoryError::InvalidStoredData(error.to_string()))?;
        let mut transaction = self.pool.begin().await?;

        let result = sqlx::query(
            r#"
            INSERT INTO tools (tool_id, name, tool_type, canonical_url, external_identifiers, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&tool.tool_id)
        .bind(&tool.name)
        .bind(tool.tool_type.as_str())
        .bind(&tool.canonical_url)
        .bind(&identifiers_json)
        .bind(format_timestamp(tool.created_at))
        .bind(format_timestamp(tool.updated_at))
        .execute(&mut *transaction)
        .await;

        if let Err(error) = result {
            if is_unique_violation(&error) {
                return Err(RepositoryError::UniqueViolation);
            }
            return Err(RepositoryError::Database(error));
        }

        for identifier in &tool.external_identifiers {
            let result = sqlx::query(
                r#"
                INSERT INTO tool_external_ids (namespace, value, tool_id, canonical_url)
                VALUES (?, ?, ?, ?)
                "#,
            )
            .bind(&identifier.namespace)
            .bind(&identifier.value)
            .bind(&tool.tool_id)
            .bind(&identifier.canonical_url)
            .execute(&mut *transaction)
            .await;

            if let Err(error) = result {
                if is_unique_violation(&error) {
                    return Err(RepositoryError::UniqueViolation);
                }
                return Err(RepositoryError::Database(error));
            }
        }

        for alias in &tool.aliases {
            sqlx::query(
                r#"
                INSERT INTO tool_aliases (tool_id, alias)
                VALUES (?, ?)
                "#,
            )
            .bind(&tool.tool_id)
            .bind(alias)
            .execute(&mut *transaction)
            .await?;
        }

        transaction.commit().await?;
        Ok(tool.clone())
    }

    pub async fn get_tool(&self, tool_id: &str) -> Result<Option<Tool>, RepositoryError> {
        let row = sqlx::query_as::<_, ToolRow>(
            r#"
            SELECT tool_id, name, tool_type, canonical_url, external_identifiers, created_at, updated_at
            FROM tools
            WHERE tool_id = ?
            "#,
        )
        .bind(tool_id)
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(row) => Ok(Some(self.load_full_tool(row).await?)),
            None => Ok(None),
        }
    }

    pub async fn list_tools(&self) -> Result<Vec<Tool>, RepositoryError> {
        let rows = sqlx::query_as::<_, ToolRow>(
            r#"
            SELECT tool_id, name, tool_type, canonical_url, external_identifiers, created_at, updated_at
            FROM tools
            ORDER BY created_at DESC, tool_id DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        let mut tools = Vec::with_capacity(rows.len());
        for row in rows {
            tools.push(self.load_full_tool(row).await?);
        }
        Ok(tools)
    }

    pub async fn find_tools_by_identifiers(
        &self,
        keys: &[String],
    ) -> Result<Vec<Tool>, RepositoryError> {
        if keys.is_empty() {
            return Ok(Vec::new());
        }

        let placeholders = keys.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "SELECT DISTINCT tool_id FROM tool_external_ids WHERE (namespace || ':' || value) IN ({placeholders})"
        );

        let mut query = sqlx::query_scalar::<_, String>(&sql);
        for key in keys {
            query = query.bind(key);
        }

        let tool_ids: Vec<String> = query.fetch_all(&self.pool).await?;
        let mut tools = Vec::with_capacity(tool_ids.len());
        for tool_id in tool_ids {
            if let Some(tool) = self.get_tool(&tool_id).await? {
                tools.push(tool);
            }
        }
        Ok(tools)
    }

    pub async fn find_tools_by_name(&self, name: &str) -> Result<Vec<Tool>, RepositoryError> {
        let name_lower = name.to_ascii_lowercase();
        let tool_ids: Vec<String> = sqlx::query_scalar(
            r#"
            SELECT DISTINCT tool_id
            FROM (
                SELECT tool_id FROM tools WHERE LOWER(name) = ?
                UNION
                SELECT tool_id FROM tool_aliases WHERE LOWER(alias) = ?
            )
            "#,
        )
        .bind(&name_lower)
        .bind(&name_lower)
        .fetch_all(&self.pool)
        .await?;

        let mut tools = Vec::with_capacity(tool_ids.len());
        for tool_id in tool_ids {
            if let Some(tool) = self.get_tool(&tool_id).await? {
                tools.push(tool);
            }
        }
        Ok(tools)
    }

    pub async fn add_external_id(
        &self,
        tool_id: &str,
        identifier: &ExternalIdentifier,
        updated_at: DateTime<Utc>,
    ) -> Result<Tool, RepositoryError> {
        let mut transaction = self.pool.begin().await?;

        let result = sqlx::query(
            r#"
            INSERT INTO tool_external_ids (namespace, value, tool_id, canonical_url)
            VALUES (?, ?, ?, ?)
            "#,
        )
        .bind(&identifier.namespace)
        .bind(&identifier.value)
        .bind(tool_id)
        .bind(&identifier.canonical_url)
        .execute(&mut *transaction)
        .await;

        if let Err(error) = result {
            if is_unique_violation(&error) {
                return Err(RepositoryError::UniqueViolation);
            }
            return Err(RepositoryError::Database(error));
        }

        // Append the new identifier to the JSON array.
        let row = sqlx::query_as::<_, ToolRow>(
            r#"
            SELECT tool_id, name, tool_type, canonical_url, external_identifiers, created_at, updated_at
            FROM tools
            WHERE tool_id = ?
            "#,
        )
        .bind(tool_id)
        .fetch_one(&mut *transaction)
        .await?;

        let mut identifiers: Vec<ExternalIdentifier> =
            serde_json::from_str(&row.external_identifiers).map_err(invalid_stored_data)?;
        identifiers.push(identifier.clone());
        let updated_json = serde_json::to_string(&identifiers).map_err(invalid_stored_data)?;

        sqlx::query(
            r#"
            UPDATE tools
            SET external_identifiers = ?, updated_at = ?
            WHERE tool_id = ?
            "#,
        )
        .bind(&updated_json)
        .bind(format_timestamp(updated_at))
        .bind(tool_id)
        .execute(&mut *transaction)
        .await?;

        transaction.commit().await?;

        self.get_tool(tool_id).await?.ok_or_else(|| {
            RepositoryError::InvalidStoredData(format!("tool {tool_id} disappeared after update"))
        })
    }

    pub async fn create_artifact(
        &self,
        artifact: &Artifact,
        event: &RunEvent,
    ) -> Result<Artifact, RepositoryError> {
        let payload = serde_json::to_string(&event.payload).map_err(invalid_stored_data)?;
        let mut transaction = self.pool.begin().await?;

        sqlx::query(
            r#"
            INSERT INTO artifacts (
                artifact_id, run_id, filename, content_type, size_bytes, sha256_hash,
                storage_key, created_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(artifact.artifact_id.to_string())
        .bind(artifact.run_id.to_string())
        .bind(&artifact.filename)
        .bind(&artifact.content_type)
        .bind(artifact.size_bytes)
        .bind(&artifact.sha256_hash)
        .bind(&artifact.storage_key)
        .bind(format_timestamp(artifact.created_at))
        .execute(&mut *transaction)
        .await?;

        insert_generated_event(&mut transaction, event, &payload).await?;
        transaction.commit().await?;
        Ok(artifact.clone())
    }

    pub async fn get_artifact(
        &self,
        artifact_id: Uuid,
    ) -> Result<Option<Artifact>, RepositoryError> {
        let row = sqlx::query_as::<_, ArtifactRow>(
            r#"
            SELECT artifact_id, run_id, filename, content_type, size_bytes, sha256_hash,
                   storage_key, created_at
            FROM artifacts
            WHERE artifact_id = ?
            "#,
        )
        .bind(artifact_id.to_string())
        .fetch_optional(&self.pool)
        .await?;

        row.map(TryInto::try_into).transpose()
    }

    pub async fn list_artifacts(&self, run_id: Uuid) -> Result<Vec<Artifact>, RepositoryError> {
        let rows = sqlx::query_as::<_, ArtifactRow>(
            r#"
            SELECT artifact_id, run_id, filename, content_type, size_bytes, sha256_hash,
                   storage_key, created_at
            FROM artifacts
            WHERE run_id = ?
            ORDER BY created_at ASC, artifact_id ASC
            "#,
        )
        .bind(run_id.to_string())
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(TryInto::try_into).collect()
    }

    pub async fn create_evidence(
        &self,
        evidence: &Evidence,
        event: &RunEvent,
    ) -> Result<Evidence, RepositoryError> {
        let supports = serde_json::to_string(&evidence.supports).map_err(invalid_stored_data)?;
        let contradicts =
            serde_json::to_string(&evidence.contradicts).map_err(invalid_stored_data)?;
        let metadata = serde_json::to_string(&evidence.metadata).map_err(invalid_stored_data)?;
        let payload = serde_json::to_string(&event.payload).map_err(invalid_stored_data)?;
        let mut transaction = self.pool.begin().await?;

        sqlx::query(
            r#"
            INSERT INTO evidence (
                evidence_id, run_id, source_type, source_url, source_revision, title, excerpt,
                retrieved_at, snapshot_artifact_id, supports, contradicts, metadata, size_bytes,
                content_hash, storage_key, created_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(evidence.evidence_id.to_string())
        .bind(evidence.run_id.to_string())
        .bind(evidence.source_type.as_str())
        .bind(&evidence.source_url)
        .bind(&evidence.source_revision)
        .bind(&evidence.title)
        .bind(&evidence.excerpt)
        .bind(format_timestamp(evidence.retrieved_at))
        .bind(evidence.snapshot_artifact_id.map(|id| id.to_string()))
        .bind(supports)
        .bind(contradicts)
        .bind(metadata)
        .bind(evidence.size_bytes)
        .bind(&evidence.content_hash)
        .bind(&evidence.storage_key)
        .bind(format_timestamp(evidence.created_at))
        .execute(&mut *transaction)
        .await?;

        insert_generated_event(&mut transaction, event, &payload).await?;
        transaction.commit().await?;
        Ok(evidence.clone())
    }

    pub async fn list_evidence(&self, run_id: Uuid) -> Result<Vec<Evidence>, RepositoryError> {
        let rows = sqlx::query_as::<_, EvidenceRow>(
            r#"
            SELECT evidence_id, run_id, source_type, source_url, source_revision, title, excerpt,
                   retrieved_at, snapshot_artifact_id, supports, contradicts, metadata, size_bytes,
                   content_hash, storage_key, created_at
            FROM evidence
            WHERE run_id = ?
            ORDER BY created_at ASC, evidence_id ASC
            "#,
        )
        .bind(run_id.to_string())
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(TryInto::try_into).collect()
    }

    pub async fn create_check_results(
        &self,
        check_results: &CheckResults,
        event: &RunEvent,
    ) -> Result<CheckResults, RepositoryError> {
        let result_json = serde_json::to_string(check_results).map_err(invalid_stored_data)?;
        let payload = serde_json::to_string(&event.payload).map_err(invalid_stored_data)?;
        let mut transaction = self.pool.begin().await?;

        let result = sqlx::query(
            r#"
            INSERT INTO check_results (
                check_results_id, run_id, evidence_board_version, standard_id, standard_version,
                profile_id, profile_version, result_json, total_score, rating, computed_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(check_results.check_results_id.to_string())
        .bind(check_results.run_id.to_string())
        .bind(check_results.evidence_board_version as i64)
        .bind(&check_results.standard_id)
        .bind(&check_results.standard_version)
        .bind(&check_results.profile_id)
        .bind(&check_results.profile_version)
        .bind(result_json)
        .bind(i64::from(check_results.total_score))
        .bind(check_results.rating.as_str())
        .bind(format_timestamp(check_results.computed_at))
        .execute(&mut *transaction)
        .await;

        if let Err(error) = result {
            if is_unique_violation(&error) {
                return Err(RepositoryError::UniqueViolation);
            }
            return Err(RepositoryError::Database(error));
        }

        insert_generated_event(&mut transaction, event, &payload).await?;
        transaction.commit().await?;
        Ok(check_results.clone())
    }

    pub async fn create_evidence_freeze(
        &self,
        freeze: &EvidenceFreezeResult,
        event: &RunEvent,
    ) -> Result<EvidenceFreezeResult, RepositoryError> {
        let board_json =
            serde_json::to_string(&freeze.evidence_board).map_err(invalid_stored_data)?;
        let manifest_json =
            serde_json::to_string(&freeze.evidence_manifest).map_err(invalid_stored_data)?;
        let payload = serde_json::to_string(&event.payload).map_err(invalid_stored_data)?;
        let mut transaction = self.pool.begin().await?;
        let run_id = freeze.evidence_board.run_id.to_string();
        let version = freeze.evidence_board.version as i64;

        let board_insert = sqlx::query(
            r#"
            INSERT INTO evidence_boards (run_id, version, board_json, frozen_at)
            VALUES (?, ?, ?, ?)
            "#,
        )
        .bind(&run_id)
        .bind(version)
        .bind(board_json)
        .bind(format_timestamp(freeze.evidence_board.frozen_at))
        .execute(&mut *transaction)
        .await;

        if let Err(error) = board_insert {
            if is_unique_violation(&error) {
                return Err(RepositoryError::UniqueViolation);
            }
            return Err(RepositoryError::Database(error));
        }

        sqlx::query(
            r#"
            INSERT INTO evidence_manifests (run_id, evidence_board_version, manifest_json)
            VALUES (?, ?, ?)
            "#,
        )
        .bind(&run_id)
        .bind(version)
        .bind(manifest_json)
        .execute(&mut *transaction)
        .await?;

        insert_generated_event(&mut transaction, event, &payload).await?;
        transaction.commit().await?;
        Ok(freeze.clone())
    }

    pub async fn get_evidence_freeze(
        &self,
        run_id: Uuid,
        version: u64,
    ) -> Result<Option<EvidenceFreezeResult>, RepositoryError> {
        let row: Option<(String, String)> = sqlx::query_as(
            r#"
            SELECT board_json, manifest_json
            FROM evidence_boards
            JOIN evidence_manifests
              ON evidence_manifests.run_id = evidence_boards.run_id
             AND evidence_manifests.evidence_board_version = evidence_boards.version
            WHERE evidence_boards.run_id = ? AND evidence_boards.version = ?
            "#,
        )
        .bind(run_id.to_string())
        .bind(version as i64)
        .fetch_optional(&self.pool)
        .await?;

        row.map(|(board_json, manifest_json)| {
            Ok(EvidenceFreezeResult {
                evidence_board: serde_json::from_str::<FrozenEvidenceBoard>(&board_json)
                    .map_err(invalid_stored_data)?,
                evidence_manifest: serde_json::from_str::<FrozenEvidenceManifest>(&manifest_json)
                    .map_err(invalid_stored_data)?,
            })
        })
        .transpose()
    }

    /// Load the stored deterministic Check Results for a frozen Evidence Board
    /// version. Used to source Rust-owned scores and `check_results_id` when
    /// building a Passport.
    pub async fn get_check_results(
        &self,
        run_id: Uuid,
        evidence_board_version: u64,
    ) -> Result<Option<CheckResults>, RepositoryError> {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT result_json FROM check_results WHERE run_id = ? AND evidence_board_version = ?",
        )
        .bind(run_id.to_string())
        .bind(evidence_board_version as i64)
        .fetch_optional(&self.pool)
        .await?;

        row.map(|(result_json,)| {
            serde_json::from_str::<CheckResults>(&result_json).map_err(invalid_stored_data)
        })
        .transpose()
    }

    /// Load the latest stored deterministic Check Results for a Run (the one
    /// with the highest evidence_board_version). Returns None when no check
    /// results have been stored for the run yet.
    pub async fn get_latest_check_results(
        &self,
        run_id: Uuid,
    ) -> Result<Option<CheckResults>, RepositoryError> {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT result_json FROM check_results WHERE run_id = ? ORDER BY evidence_board_version DESC LIMIT 1",
        )
        .bind(run_id.to_string())
        .fetch_optional(&self.pool)
        .await?;

        row.map(|(result_json,)| {
            serde_json::from_str::<CheckResults>(&result_json).map_err(invalid_stored_data)
        })
        .transpose()
    }

    /// Next Passport sequence for a Run (1 if none exists yet). The unique
    /// constraint on `(run_id, sequence)` is the backstop against concurrent
    /// freezes selecting the same value.
    pub async fn next_passport_sequence(&self, run_id: Uuid) -> Result<u64, RepositoryError> {
        let row: Option<(Option<i64>,)> =
            sqlx::query_as("SELECT MAX(sequence) FROM passports WHERE run_id = ?")
                .bind(run_id.to_string())
                .fetch_optional(&self.pool)
                .await?;
        Ok(row
            .and_then(|(max_sequence,)| max_sequence)
            .map(|max_sequence| max_sequence as u64 + 1)
            .unwrap_or(1))
    }

    /// Atomically persist an immutable Passport, its Provenance record, and the
    /// Trust-Core-owned `provenance_frozen` event. The event is appended first so
    /// its computed `event_hash` can be stamped as `audit_log_hash` into the
    /// Provenance before both rows are inserted. Any failure rolls back the
    /// event, the Passport and the Provenance together.
    pub async fn create_passport_freeze(
        &self,
        passport: &Passport,
        mut provenance: Provenance,
        event: &RunEvent,
    ) -> Result<PassportFreezeResult, RepositoryError> {
        let passport_json = serde_json::to_string(passport).map_err(invalid_stored_data)?;
        let payload = serde_json::to_string(&event.payload).map_err(invalid_stored_data)?;
        let mut transaction = self.pool.begin().await?;
        let run_id_str = passport.run_id.to_string();
        let sequence = passport.passport_sequence as i64;

        provenance.audit_log_hash =
            insert_generated_event(&mut transaction, event, &payload).await?;
        let provenance_json = serde_json::to_string(&provenance).map_err(invalid_stored_data)?;

        let passport_insert = sqlx::query(
            r#"
            INSERT INTO passports (run_id, sequence, passport_json, frozen_at)
            VALUES (?, ?, ?, ?)
            "#,
        )
        .bind(&run_id_str)
        .bind(sequence)
        .bind(&passport_json)
        .bind(format_timestamp(provenance.frozen_at))
        .execute(&mut *transaction)
        .await;

        if let Err(error) = passport_insert {
            if is_unique_violation(&error) {
                return Err(RepositoryError::UniqueViolation);
            }
            return Err(RepositoryError::Database(error));
        }

        let provenance_insert = sqlx::query(
            r#"
            INSERT INTO provenances (run_id, freeze_version, provenance_json)
            VALUES (?, ?, ?)
            "#,
        )
        .bind(&run_id_str)
        .bind(sequence)
        .bind(&provenance_json)
        .execute(&mut *transaction)
        .await;

        if let Err(error) = provenance_insert {
            if is_unique_violation(&error) {
                return Err(RepositoryError::UniqueViolation);
            }
            return Err(RepositoryError::Database(error));
        }

        transaction.commit().await?;
        Ok(PassportFreezeResult {
            passport: passport.clone(),
            provenance,
        })
    }

    pub async fn get_passport_freeze(
        &self,
        run_id: Uuid,
        sequence: u64,
    ) -> Result<Option<PassportFreezeResult>, RepositoryError> {
        let row: Option<(String, String)> = sqlx::query_as(
            r#"
            SELECT passport_json, provenance_json
            FROM passports
            JOIN provenances
              ON provenances.run_id = passports.run_id
             AND provenances.freeze_version = passports.sequence
            WHERE passports.run_id = ? AND passports.sequence = ?
            "#,
        )
        .bind(run_id.to_string())
        .bind(sequence as i64)
        .fetch_optional(&self.pool)
        .await?;

        row.map(|(passport_json, provenance_json)| {
            Ok(PassportFreezeResult {
                passport: serde_json::from_str::<Passport>(&passport_json)
                    .map_err(invalid_stored_data)?,
                provenance: serde_json::from_str::<Provenance>(&provenance_json)
                    .map_err(invalid_stored_data)?,
            })
        })
        .transpose()
    }

    pub async fn create_approval(
        &self,
        approval: &Approval,
        event: &RunEvent,
        expected_status: RunStatus,
        next_status: RunStatus,
    ) -> Result<Approval, RepositoryError> {
        let approval_json = serde_json::to_string(approval).map_err(invalid_stored_data)?;
        let event_payload = serde_json::to_string(&event.payload).map_err(invalid_stored_data)?;
        let run_id = approval.run_id.to_string();
        let decided_at = format_timestamp(approval.decided_at);
        let mut transaction = self.pool.begin().await?;

        let update = sqlx::query(
            "UPDATE runs SET status = ?, current_node = ?, updated_at = ? WHERE run_id = ? AND status = ?",
        )
        .bind(next_status.as_str())
        .bind("human_review_gate")
        .bind(&decided_at)
        .bind(&run_id)
        .bind(expected_status.as_str())
        .execute(&mut *transaction)
        .await?;
        if update.rows_affected() != 1 {
            return Err(RepositoryError::RunStateChanged);
        }

        let insert = sqlx::query(
            "INSERT INTO approvals (approval_id, run_id, approval_json, decided_at) VALUES (?, ?, ?, ?)",
        )
        .bind(approval.approval_id.to_string())
        .bind(&run_id)
        .bind(approval_json)
        .bind(&decided_at)
        .execute(&mut *transaction)
        .await;
        if let Err(error) = insert {
            if is_unique_violation(&error) {
                return Err(RepositoryError::UniqueViolation);
            }
            return Err(RepositoryError::Database(error));
        }

        insert_generated_event(&mut transaction, event, &event_payload).await?;
        transaction.commit().await?;
        Ok(approval.clone())
    }

    pub async fn get_approval(&self, run_id: Uuid) -> Result<Option<Approval>, RepositoryError> {
        let approval_json: Option<String> =
            sqlx::query_scalar("SELECT approval_json FROM approvals WHERE run_id = ?")
                .bind(run_id.to_string())
                .fetch_optional(&self.pool)
                .await?;
        approval_json
            .map(|value| serde_json::from_str(&value).map_err(invalid_stored_data))
            .transpose()
    }

    async fn load_full_tool(&self, row: ToolRow) -> Result<Tool, RepositoryError> {
        let external_ids = sqlx::query_as::<_, ToolExternalIdRow>(
            r#"
            SELECT namespace, value, tool_id, canonical_url
            FROM tool_external_ids
            WHERE tool_id = ?
            ORDER BY namespace, value
            "#,
        )
        .bind(&row.tool_id)
        .fetch_all(&self.pool)
        .await?;

        let aliases: Vec<String> =
            sqlx::query_scalar("SELECT alias FROM tool_aliases WHERE tool_id = ? ORDER BY alias")
                .bind(&row.tool_id)
                .fetch_all(&self.pool)
                .await?;

        let tool: Tool = row.try_into()?;
        Ok(Tool {
            external_identifiers: external_ids
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<Vec<_>, _>>()?,
            aliases,
            ..tool
        })
    }
}

pub async fn connect_and_migrate(database_url: &str) -> Result<SqlitePool, RepositoryError> {
    let options = SqliteConnectOptions::from_str(database_url)?
        .create_if_missing(true)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5));
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;
    migrate(&pool).await?;
    Ok(pool)
}

pub async fn migrate(pool: &SqlitePool) -> Result<(), RepositoryError> {
    sqlx::migrate!("./migrations").run(pool).await?;
    Ok(())
}

#[derive(Debug, FromRow)]
struct RunRow {
    run_id: String,
    goal: String,
    tool_id: String,
    canonical_url: String,
    tool_name: String,
    tool_type: String,
    tool_urls: String,
    standard_id: String,
    standard_version: String,
    profile_id: String,
    profile_version: String,
    status: String,
    current_node: Option<String>,
    created_at: String,
    updated_at: String,
}

impl TryFrom<RunRow> for Run {
    type Error = RepositoryError;

    fn try_from(row: RunRow) -> Result<Self, Self::Error> {
        Ok(Self {
            run_id: parse_uuid(&row.run_id)?,
            goal: row.goal,
            tool_id: row.tool_id,
            canonical_url: row.canonical_url,
            tool: ToolInput {
                name: row.tool_name,
                tool_type: row.tool_type,
                urls: serde_json::from_str(&row.tool_urls).map_err(invalid_stored_data)?,
            },
            audit_binding: AuditBinding {
                standard_id: row.standard_id,
                standard_version: row.standard_version,
                profile_id: row.profile_id,
                profile_version: row.profile_version,
            },
            status: RunStatus::parse(&row.status).ok_or_else(|| {
                RepositoryError::InvalidStoredData(format!("unknown run status: {}", row.status))
            })?,
            current_node: row.current_node,
            created_at: parse_timestamp(&row.created_at)?,
            updated_at: parse_timestamp(&row.updated_at)?,
        })
    }
}

#[derive(Debug, FromRow)]
struct RunEventRow {
    event_id: String,
    run_id: String,
    sequence: i64,
    node_id: String,
    event_type: String,
    payload: String,
    created_at: String,
    event_hash: String,
    prev_event_hash: String,
}

impl TryFrom<RunEventRow> for RunEvent {
    type Error = RepositoryError;

    fn try_from(row: RunEventRow) -> Result<Self, Self::Error> {
        Ok(Self {
            event_id: parse_uuid(&row.event_id)?,
            run_id: parse_uuid(&row.run_id)?,
            sequence: row.sequence,
            node_id: row.node_id,
            event_type: RunEventType::parse(&row.event_type).ok_or_else(|| {
                RepositoryError::InvalidStoredData(format!(
                    "unknown run event type: {}",
                    row.event_type
                ))
            })?,
            payload: serde_json::from_str::<Map<String, Value>>(&row.payload)
                .map_err(invalid_stored_data)?,
            created_at: parse_timestamp(&row.created_at)?,
            event_hash: row.event_hash,
            prev_event_hash: row.prev_event_hash,
        })
    }
}

fn format_timestamp(timestamp: DateTime<Utc>) -> String {
    timestamp.to_rfc3339_opts(SecondsFormat::Micros, true)
}

/// Compute the deterministic event hash: SHA-256(JCS(canonical_input)).
///
/// The canonical input includes all fields except `event_hash` itself:
/// `run_id`, `sequence`, `node_id`, `event_type`, `payload`, `created_at`, `prev_event_hash`.
fn compute_event_hash(
    run_id: &str,
    sequence: i64,
    node_id: &str,
    event_type: &str,
    payload: &Map<String, Value>,
    created_at: &str,
    prev_event_hash: &str,
) -> String {
    let canonical_input = json!({
        "run_id": run_id,
        "sequence": sequence,
        "node_id": node_id,
        "event_type": event_type,
        "payload": payload,
        "created_at": created_at,
        "prev_event_hash": prev_event_hash,
    });
    canonical_sha256(&canonical_input)
}

/// SHA-256 over the RFC 8785 JCS canonicalization of a JSON value, returned as
/// a `0x`-prefixed lowercase hex string. Used for every commitment hash that is
/// taken over a JSON document (`event_hash`, `passportHash`, `evidenceManifestHash`).
pub fn canonical_sha256(value: &Value) -> String {
    let canonical_bytes = serde_json_canonicalizer::to_string(value)
        .expect("JCS canonicalization must succeed for deterministic hash");
    sha256_hex(canonical_bytes.as_bytes())
}

/// Raw SHA-256 over arbitrary bytes, returned as `0x`-prefixed lowercase hex.
/// Used for the onchain `runId` commitment: SHA-256 of the lowercase Run UUID
/// string.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("0x{}", hex::encode(hasher.finalize()))
}

/// Determine the next sequence number and previous hash for a run within a transaction.
async fn next_sequence_and_prev_hash(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    run_id: &str,
) -> (i64, String) {
    let conn = transaction.deref_mut();
    let row: Option<(Option<i64>, Option<String>)> = sqlx::query_as(
        "SELECT MAX(sequence), (SELECT event_hash FROM run_events WHERE run_id = ? ORDER BY sequence DESC LIMIT 1) FROM run_events WHERE run_id = ?"
    )
    .bind(run_id)
    .bind(run_id)
    .fetch_optional(conn)
    .await
    .ok()
    .flatten();

    match row {
        Some((Some(max_seq), Some(prev_hash))) => (max_seq + 1, prev_hash),
        _ => (1, ZERO_HASH.to_owned()),
    }
}

fn parse_uuid(value: &str) -> Result<Uuid, RepositoryError> {
    Uuid::parse_str(value).map_err(invalid_stored_data)
}

fn parse_timestamp(value: &str) -> Result<DateTime<Utc>, RepositoryError> {
    DateTime::parse_from_rfc3339(value)
        .map(|timestamp| timestamp.with_timezone(&Utc))
        .map_err(invalid_stored_data)
}

fn invalid_stored_data(error: impl std::fmt::Display) -> RepositoryError {
    RepositoryError::InvalidStoredData(error.to_string())
}

fn is_unique_violation(error: &sqlx::Error) -> bool {
    if let sqlx::Error::Database(db_error) = error {
        db_error.message().contains("UNIQUE constraint failed")
    } else {
        false
    }
}

// ── Tool Row Types ────────────────────────────────────────────────

#[derive(Debug, FromRow)]
struct ToolRow {
    tool_id: String,
    name: String,
    tool_type: String,
    canonical_url: String,
    external_identifiers: String,
    created_at: String,
    updated_at: String,
}

impl TryFrom<ToolRow> for Tool {
    type Error = RepositoryError;

    fn try_from(row: ToolRow) -> Result<Self, Self::Error> {
        let external_identifiers: Vec<ExternalIdentifier> =
            serde_json::from_str(&row.external_identifiers).map_err(invalid_stored_data)?;
        Ok(Self {
            tool_schema_version: "0.1.0",
            tool_id: row.tool_id,
            name: row.name,
            tool_type: ToolType::parse(&row.tool_type).ok_or_else(|| {
                RepositoryError::InvalidStoredData(format!("unknown tool_type: {}", row.tool_type))
            })?,
            canonical_url: row.canonical_url,
            external_identifiers,
            aliases: Vec::new(), // populated by load_full_tool
            created_at: parse_timestamp(&row.created_at)?,
            updated_at: parse_timestamp(&row.updated_at)?,
        })
    }
}

#[derive(Debug, FromRow)]
struct ToolExternalIdRow {
    namespace: String,
    value: String,
    #[allow(dead_code)]
    tool_id: String,
    canonical_url: String,
}

impl TryFrom<ToolExternalIdRow> for ExternalIdentifier {
    type Error = RepositoryError;

    fn try_from(row: ToolExternalIdRow) -> Result<Self, Self::Error> {
        Ok(Self {
            namespace: row.namespace,
            value: row.value,
            canonical_url: row.canonical_url,
        })
    }
}

#[derive(Debug, FromRow)]
struct ArtifactRow {
    artifact_id: String,
    run_id: String,
    filename: String,
    content_type: String,
    size_bytes: i64,
    sha256_hash: String,
    storage_key: String,
    created_at: String,
}

impl TryFrom<ArtifactRow> for Artifact {
    type Error = RepositoryError;

    fn try_from(row: ArtifactRow) -> Result<Self, Self::Error> {
        Ok(Self {
            artifact_schema_version: "0.1.0",
            artifact_id: parse_uuid(&row.artifact_id)?,
            run_id: parse_uuid(&row.run_id)?,
            filename: row.filename,
            content_type: row.content_type,
            size_bytes: row.size_bytes,
            sha256_hash: row.sha256_hash,
            storage_key: row.storage_key,
            created_at: parse_timestamp(&row.created_at)?,
        })
    }
}

#[derive(Debug, FromRow)]
struct EvidenceRow {
    evidence_id: String,
    run_id: String,
    source_type: String,
    source_url: String,
    source_revision: Option<String>,
    title: String,
    excerpt: String,
    retrieved_at: String,
    snapshot_artifact_id: Option<String>,
    supports: String,
    contradicts: String,
    metadata: String,
    size_bytes: i64,
    content_hash: String,
    storage_key: String,
    created_at: String,
}

impl TryFrom<EvidenceRow> for Evidence {
    type Error = RepositoryError;

    fn try_from(row: EvidenceRow) -> Result<Self, Self::Error> {
        Ok(Self {
            evidence_schema_version: "0.2.0",
            evidence_id: parse_uuid(&row.evidence_id)?,
            run_id: parse_uuid(&row.run_id)?,
            source_type: EvidenceSourceType::parse(&row.source_type).ok_or_else(|| {
                RepositoryError::InvalidStoredData(format!(
                    "unknown evidence source type: {}",
                    row.source_type
                ))
            })?,
            source_url: row.source_url,
            source_revision: row.source_revision,
            title: row.title,
            excerpt: row.excerpt,
            retrieved_at: parse_timestamp(&row.retrieved_at)?,
            snapshot_artifact_id: row
                .snapshot_artifact_id
                .map(|value| parse_uuid(&value))
                .transpose()?,
            supports: serde_json::from_str(&row.supports).map_err(invalid_stored_data)?,
            contradicts: serde_json::from_str(&row.contradicts).map_err(invalid_stored_data)?,
            metadata: serde_json::from_str(&row.metadata).map_err(invalid_stored_data)?,
            size_bytes: row.size_bytes,
            content_hash: row.content_hash,
            storage_key: row.storage_key,
            created_at: parse_timestamp(&row.created_at)?,
        })
    }
}

async fn insert_generated_event(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    event: &RunEvent,
    payload: &str,
) -> Result<String, RepositoryError> {
    let run_id_str = event.run_id.to_string();
    let (next_seq, prev_event_hash) = next_sequence_and_prev_hash(transaction, &run_id_str).await;
    let created_at_str = format_timestamp(event.created_at);
    let event_hash = compute_event_hash(
        &run_id_str,
        next_seq,
        &event.node_id,
        event.event_type.as_str(),
        &event.payload,
        &created_at_str,
        &prev_event_hash,
    );

    sqlx::query(
        r#"
        INSERT INTO run_events (
            event_id, run_id, sequence, node_id, event_type, payload,
            created_at, event_hash, prev_event_hash
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(event.event_id.to_string())
    .bind(&run_id_str)
    .bind(next_seq)
    .bind(&event.node_id)
    .bind(event.event_type.as_str())
    .bind(payload)
    .bind(&created_at_str)
    .bind(&event_hash)
    .bind(&prev_event_hash)
    .execute(&mut **transaction)
    .await?;

    sqlx::query("UPDATE runs SET updated_at = ? WHERE run_id = ?")
        .bind(&created_at_str)
        .bind(&run_id_str)
        .execute(&mut **transaction)
        .await?;

    Ok(event_hash)
}
