use std::fmt::Display;

use uuid::Uuid;

use super::enums::str_enum;

str_enum! {
    pub enum Severity { Blocking = "blocking", Warning = "warning", Info = "info" }
}

/// A validation error, integrity-rule violation or workflow-gate failure.
#[derive(Debug, Clone, PartialEq)]
pub struct Issue {
    pub rule_id: String,
    pub severity: Severity,
    pub message: String,
    pub field: Option<String>,
    pub entity_type: Option<String>,
    pub entity_id: Option<Uuid>,
    pub workflow_step: Option<String>,
    pub standard_ref: Option<String>,
}

impl Issue {
    pub fn new(severity: Severity, rule_id: &str, message: impl Into<String>) -> Self {
        Self {
            rule_id: rule_id.to_owned(),
            severity,
            message: message.into(),
            field: None,
            entity_type: None,
            entity_id: None,
            workflow_step: None,
            standard_ref: None,
        }
    }

    pub fn blocking(rule_id: &str, message: impl Into<String>) -> Self {
        Self::new(Severity::Blocking, rule_id, message)
    }

    pub fn warning(rule_id: &str, message: impl Into<String>) -> Self {
        Self::new(Severity::Warning, rule_id, message)
    }

    pub fn field(mut self, field: &str) -> Self {
        self.field = Some(field.to_owned());
        self
    }

    pub fn entity(mut self, entity_type: &str, id: Uuid) -> Self {
        self.entity_type = Some(entity_type.to_owned());
        self.entity_id = Some(id);
        self
    }

    pub fn standard(mut self, reference: &str) -> Self {
        self.standard_ref = Some(reference.to_owned());
        self
    }

    pub fn is_blocking(&self) -> bool {
        self.severity == Severity::Blocking
    }
}

/// The single error type of the domain and application layers. The web layer maps it to
/// RFC 9457 problem details; infrastructure adapters map their errors into it.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0} not found")]
    NotFound(&'static str),
    #[error("validation failed")]
    Validation(Vec<Issue>),
    #[error("{0}")]
    Conflict(String),
    #[error("the resource was modified by someone else (version mismatch)")]
    PreconditionFailed,
    #[error("{0}")]
    Forbidden(String),
    #[error("{0}")]
    Unauthorized(String),
    #[error("{0}")]
    Unavailable(String),
    #[error("{0}")]
    NotImplemented(String),
    #[error("internal error: {0}")]
    Internal(String),
}

impl AppError {
    /// A single blocking validation issue.
    pub fn invalid(rule_id: &str, field: Option<&str>, message: impl Into<String>) -> Self {
        let mut issue = Issue::blocking(rule_id, message);
        issue.field = field.map(str::to_owned);
        AppError::Validation(vec![issue])
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        AppError::Conflict(message.into())
    }

    pub fn internal(error: impl Display) -> Self {
        AppError::Internal(error.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;

/// Collects issues and fails with all of them at once.
#[derive(Debug, Default)]
pub struct Issues(Vec<Issue>);

impl Issues {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, issue: Issue) {
        self.0.push(issue);
    }

    pub fn check(&mut self, ok: bool, rule_id: &str, field: &str, message: impl Into<String>) {
        if !ok {
            self.push(Issue::blocking(rule_id, message).field(field));
        }
    }

    pub fn into_result(self) -> AppResult<()> {
        if self.0.is_empty() {
            Ok(())
        } else {
            Err(AppError::Validation(self.0))
        }
    }
}
