use axum::extract::rejection::{PathRejection, QueryRejection};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde_with::skip_serializing_none;
use uuid::Uuid;

use crate::shared::kernel::{AppError, Issue};

/// Error returned by handlers and extractors, rendered as RFC 9457 `application/problem+json`.
#[derive(Debug)]
pub enum ApiError {
    App(AppError),
    /// HTTP-level problems (malformed body, unsupported media type, payload too large).
    Http {
        status: StatusCode,
        detail: String,
    },
}

impl From<AppError> for ApiError {
    fn from(e: AppError) -> Self {
        ApiError::App(e)
    }
}

impl ApiError {
    pub fn bad_request(detail: impl Into<String>) -> Self {
        ApiError::Http {
            status: StatusCode::BAD_REQUEST,
            detail: detail.into(),
        }
    }
}

#[skip_serializing_none]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Problem {
    #[serde(rename = "type")]
    kind: &'static str,
    title: &'static str,
    status: u16,
    detail: Option<String>,
    issues: Option<Vec<IssueDto>>,
}

#[skip_serializing_none]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueDto {
    pub rule_id: String,
    pub severity: String,
    pub message: String,
    pub field: Option<String>,
    pub entity_type: Option<String>,
    pub entity_id: Option<Uuid>,
    pub workflow_step: Option<String>,
    pub standard_ref: Option<String>,
}

impl From<Issue> for IssueDto {
    fn from(i: Issue) -> Self {
        IssueDto {
            rule_id: i.rule_id,
            severity: i.severity.to_string(),
            message: i.message,
            field: i.field,
            entity_type: i.entity_type,
            entity_id: i.entity_id,
            workflow_step: i.workflow_step,
            standard_ref: i.standard_ref,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, title, detail, issues) = match self {
            ApiError::Http { status, detail } => (
                status,
                status.canonical_reason().unwrap_or("Error"),
                Some(detail),
                None,
            ),
            ApiError::App(e) => match e {
                AppError::NotFound(what) => (
                    StatusCode::NOT_FOUND,
                    "Not Found",
                    Some(format!("{what} not found")),
                    None,
                ),
                AppError::Validation(issues) => (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "Validation Failed",
                    Some("the request violates validation rules or workflow gates".to_owned()),
                    Some(issues.into_iter().map(IssueDto::from).collect()),
                ),
                AppError::Conflict(d) => (StatusCode::CONFLICT, "Conflict", Some(d), None),
                AppError::PreconditionFailed => (
                    StatusCode::PRECONDITION_FAILED,
                    "Precondition Failed",
                    Some("the resource was modified by someone else; reload and retry".to_owned()),
                    None,
                ),
                AppError::Forbidden(d) => (StatusCode::FORBIDDEN, "Forbidden", Some(d), None),
                AppError::Unauthorized(d) => {
                    (StatusCode::UNAUTHORIZED, "Unauthorized", Some(d), None)
                }
                AppError::Unavailable(d) => (
                    StatusCode::SERVICE_UNAVAILABLE,
                    "Service Unavailable",
                    Some(d),
                    None,
                ),
                AppError::NotImplemented(d) => (
                    StatusCode::NOT_IMPLEMENTED,
                    "Not Implemented",
                    Some(d),
                    None,
                ),
                AppError::Internal(d) => {
                    // Details are logged, never returned to the client.
                    tracing::error!(detail = %d, "internal error");
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "Internal Server Error",
                        None,
                        None,
                    )
                }
            },
        };
        let body = Problem {
            kind: "about:blank",
            title,
            status: status.as_u16(),
            detail,
            issues,
        };
        let mut response = (status, axum::Json(body)).into_response();
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/problem+json"),
        );
        if status == StatusCode::UNAUTHORIZED {
            response.headers_mut().insert(
                header::WWW_AUTHENTICATE,
                header::HeaderValue::from_static("Bearer"),
            );
        }
        response
    }
}

impl From<PathRejection> for ApiError {
    fn from(r: PathRejection) -> Self {
        ApiError::bad_request(format!("invalid path parameter: {}", r.body_text()))
    }
}

impl From<QueryRejection> for ApiError {
    fn from(r: QueryRejection) -> Self {
        ApiError::App(AppError::invalid("INVALID_QUERY", None, r.body_text()))
    }
}

pub type ApiResult<T> = Result<T, ApiError>;
