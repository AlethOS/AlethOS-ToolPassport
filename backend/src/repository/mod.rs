use std::{str::FromStr, time::Duration};

use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{Map, Value};
use sqlx::{
    FromRow, SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};
use thiserror::Error;
use uuid::Uuid;

use crate::domain::{Run, RunEvent, RunEventType, RunStatus, ToolInput};

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
                run_id, goal, tool_name, tool_type, tool_urls, status,
                current_node, created_at, updated_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(run.run_id.to_string())
        .bind(&run.goal)
        .bind(&run.tool.name)
        .bind(&run.tool.tool_type)
        .bind(tool_urls)
        .bind(run.status.as_str())
        .bind(&run.current_node)
        .bind(format_timestamp(run.created_at))
        .bind(format_timestamp(run.updated_at))
        .execute(&mut *transaction)
        .await?;

        sqlx::query(
            r#"
            INSERT INTO run_events (
                event_id, run_id, sequence, node_id, event_type, payload, created_at
            )
            VALUES (?, ?, 1, ?, ?, ?, ?)
            "#,
        )
        .bind(created_event.event_id.to_string())
        .bind(created_event.run_id.to_string())
        .bind(&created_event.node_id)
        .bind(created_event.event_type.as_str())
        .bind(event_payload)
        .bind(format_timestamp(created_event.created_at))
        .execute(&mut *transaction)
        .await?;

        transaction.commit().await?;
        Ok(run.clone())
    }

    pub async fn list_runs(&self) -> Result<Vec<Run>, RepositoryError> {
        let rows = sqlx::query_as::<_, RunRow>(
            r#"
            SELECT run_id, goal, tool_name, tool_type, tool_urls, status,
                   current_node, created_at, updated_at
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
            SELECT run_id, goal, tool_name, tool_type, tool_urls, status,
                   current_node, created_at, updated_at
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
        .bind(event.run_id.to_string())
        .bind(expected_status.as_str())
        .execute(&mut *transaction)
        .await?;

        if update.rows_affected() != 1 {
            return Err(RepositoryError::RunStateChanged);
        }

        let row = sqlx::query_as::<_, RunEventRow>(
            r#"
            INSERT INTO run_events (
                event_id, run_id, sequence, node_id, event_type, payload, created_at
            )
            SELECT ?, ?, COALESCE(MAX(sequence), 0) + 1, ?, ?, ?, ?
            FROM run_events
            WHERE run_id = ?
            RETURNING event_id, run_id, sequence, node_id, event_type, payload, created_at
            "#,
        )
        .bind(event.event_id.to_string())
        .bind(event.run_id.to_string())
        .bind(&event.node_id)
        .bind(event.event_type.as_str())
        .bind(payload)
        .bind(format_timestamp(event.created_at))
        .bind(event.run_id.to_string())
        .fetch_one(&mut *transaction)
        .await?;

        transaction.commit().await?;
        row.try_into()
    }

    pub async fn list_events(&self, run_id: Uuid) -> Result<Vec<RunEvent>, RepositoryError> {
        let rows = sqlx::query_as::<_, RunEventRow>(
            r#"
            SELECT event_id, run_id, sequence, node_id, event_type, payload, created_at
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
    tool_name: String,
    tool_type: String,
    tool_urls: String,
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
            tool: ToolInput {
                name: row.tool_name,
                tool_type: row.tool_type,
                urls: serde_json::from_str(&row.tool_urls).map_err(invalid_stored_data)?,
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
        })
    }
}

fn format_timestamp(timestamp: DateTime<Utc>) -> String {
    timestamp.to_rfc3339_opts(SecondsFormat::Micros, true)
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
