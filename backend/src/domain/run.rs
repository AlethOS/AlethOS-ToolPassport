use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::RunEvent;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ToolInput {
    pub name: String,
    pub tool_type: String,
    #[serde(default)]
    pub urls: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateRunRequest {
    pub goal: String,
    pub tool_id: String,
}

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Pending,
    Running,
    Success,
    Failed,
    WaitingApproval,
    Cancelled,
}

impl RunStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Success => "success",
            Self::Failed => "failed",
            Self::WaitingApproval => "waiting_approval",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(Self::Pending),
            "running" => Some(Self::Running),
            "success" => Some(Self::Success),
            "failed" => Some(Self::Failed),
            "waiting_approval" => Some(Self::WaitingApproval),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Run {
    pub run_id: Uuid,
    pub goal: String,
    pub tool_id: String,
    pub canonical_url: String,
    pub tool: ToolInput,
    pub status: RunStatus,
    pub current_node: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunDetails {
    pub run: Run,
    pub events: Vec<RunEvent>,
}
