use axum::{
    Json,
    extract::{multipart::MultipartError, rejection::JsonRejection},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use serde_json::{Value, json};

use crate::services::ServiceError;

pub type ApiResult<T> = Result<T, ApiError>;

#[derive(Debug, Serialize)]
struct ErrorResponse {
    code: &'static str,
    message: String,
    details: Value,
}

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    response: ErrorResponse,
}

impl ApiError {
    pub fn invalid_json(rejection: JsonRejection) -> Self {
        Self::new(
            StatusCode::BAD_REQUEST,
            "invalid_json",
            "request body must be valid JSON matching the expected shape",
            json!({ "reason": rejection.body_text() }),
        )
    }

    pub fn invalid_multipart(error: MultipartError) -> Self {
        Self::new(
            error.status(),
            "invalid_multipart",
            "request body must contain one valid multipart file field",
            json!({}),
        )
    }

    pub fn new(
        status: StatusCode,
        code: &'static str,
        message: impl Into<String>,
        details: Value,
    ) -> Self {
        Self {
            status,
            response: ErrorResponse {
                code,
                message: message.into(),
                details,
            },
        }
    }
}

impl From<ServiceError> for ApiError {
    fn from(error: ServiceError) -> Self {
        match error {
            ServiceError::InvalidRunId => Self::new(
                StatusCode::BAD_REQUEST,
                "invalid_run_id",
                error.to_string(),
                json!({}),
            ),
            ServiceError::InvalidRequest(message) => Self::new(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                message,
                json!({}),
            ),
            ServiceError::RunNotFound => Self::new(
                StatusCode::NOT_FOUND,
                "run_not_found",
                error.to_string(),
                json!({}),
            ),
            ServiceError::Conflict(message) => Self::new(
                StatusCode::CONFLICT,
                "run_state_conflict",
                message,
                json!({}),
            ),
            ServiceError::CheckResultsAlreadyExist => Self::new(
                StatusCode::CONFLICT,
                "check_results_already_exist",
                error.to_string(),
                json!({}),
            ),
            ServiceError::EvidenceBoardAlreadyFrozen => Self::new(
                StatusCode::CONFLICT,
                "evidence_board_already_frozen",
                error.to_string(),
                json!({}),
            ),
            ServiceError::EvidenceBoardNotFound => Self::new(
                StatusCode::NOT_FOUND,
                "evidence_board_not_found",
                error.to_string(),
                json!({}),
            ),
            ServiceError::PassportAlreadyFrozen => Self::new(
                StatusCode::CONFLICT,
                "passport_already_frozen",
                error.to_string(),
                json!({}),
            ),
            ServiceError::PassportNotFound => Self::new(
                StatusCode::NOT_FOUND,
                "passport_not_found",
                error.to_string(),
                json!({}),
            ),
            ServiceError::ToolNotFound => Self::new(
                StatusCode::NOT_FOUND,
                "tool_not_found",
                error.to_string(),
                json!({}),
            ),
            ServiceError::InvalidToolIdFormat => Self::new(
                StatusCode::BAD_REQUEST,
                "invalid_tool_id_format",
                error.to_string(),
                json!({}),
            ),
            ServiceError::ToolAlreadyExists => Self::new(
                StatusCode::CONFLICT,
                "tool_already_exists",
                error.to_string(),
                json!({}),
            ),
            ServiceError::IdentifierAlreadyClaimed(ref owner) => Self::new(
                StatusCode::CONFLICT,
                "identifier_already_claimed",
                error.to_string(),
                json!({ "claimed_by": owner }),
            ),
            ServiceError::InvalidUrl(detail) => {
                Self::new(StatusCode::BAD_REQUEST, "invalid_url", detail, json!({}))
            }
            ServiceError::InvalidIntakeVersion => Self::new(
                StatusCode::BAD_REQUEST,
                "invalid_intake_version",
                error.to_string(),
                json!({}),
            ),
            ServiceError::Repository(_) => Self::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "an internal error occurred",
                json!({}),
            ),
            ServiceError::Storage(err) => match err {
                crate::services::StorageError::TooLarge { .. } => Self::new(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "stored_content_too_large",
                    err.to_string(),
                    json!({}),
                ),
                crate::services::StorageError::InvalidStorageKey
                | crate::services::StorageError::PathTraversal => Self::new(
                    StatusCode::BAD_REQUEST,
                    "storage_error",
                    err.to_string(),
                    json!({}),
                ),
                crate::services::StorageError::Io(_) => Self::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "an internal error occurred",
                    json!({}),
                ),
            },
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(self.response)).into_response()
    }
}

pub async fn not_found() -> ApiError {
    ApiError::new(
        StatusCode::NOT_FOUND,
        "route_not_found",
        "route not found",
        json!({}),
    )
}

pub async fn method_not_allowed() -> ApiError {
    ApiError::new(
        StatusCode::METHOD_NOT_ALLOWED,
        "method_not_allowed",
        "method not allowed",
        json!({}),
    )
}
