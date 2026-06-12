use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolType {
    Generic,
    AgentFramework,
    McpServer,
    CliApiTool,
}

impl ToolType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Generic => "generic",
            Self::AgentFramework => "agent_framework",
            Self::McpServer => "mcp_server",
            Self::CliApiTool => "cli_api_tool",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "generic" => Some(Self::Generic),
            "agent_framework" => Some(Self::AgentFramework),
            "mcp_server" => Some(Self::McpServer),
            "cli_api_tool" => Some(Self::CliApiTool),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalIdentifier {
    pub namespace: String,
    pub value: String,
    pub canonical_url: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Tool {
    pub tool_schema_version: &'static str,
    pub tool_id: String,
    pub name: String,
    pub tool_type: ToolType,
    pub canonical_url: String,
    pub external_identifiers: Vec<ExternalIdentifier>,
    pub aliases: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateToolRequest {
    pub tool_id: String,
    pub name: String,
    pub tool_type: ToolType,
    pub canonical_url: String,
    pub external_identifiers: Vec<ExternalIdentifier>,
    #[serde(default)]
    pub aliases: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddIdentifierRequest {
    pub identifier: ExternalIdentifier,
}
