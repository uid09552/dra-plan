use axum::extract::State;
use axum::http::HeaderMap;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;
use uuid::Uuid;
use validator::Validate;

use super::application::{StepState, WorkflowState};
use super::domain::WorkflowStepKey;
use crate::app::AppState;
use crate::features::catalog::api::language_from;
use crate::shared::kernel::{Language, Severity, TenantContext, parse_enum};
use crate::shared::web::{ApiPath, ApiResult, IssueDto, OptionalJson};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/services/{service_id}/workflow", get(get_workflow))
        .route(
            "/services/{service_id}/workflow/steps/{step_key}/complete",
            post(complete),
        )
        .route(
            "/services/{service_id}/workflow/steps/{step_key}/reopen",
            post(reopen),
        )
        .route("/services/{service_id}/validation", get(validation))
}

#[skip_serializing_none]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowStepDto {
    pub key: String,
    pub number: u8,
    pub phase: String,
    pub title: &'static str,
    pub status: String,
    pub gate_passed: bool,
    pub issues: Vec<IssueDto>,
    pub completed_by: Option<String>,
    pub completed_at: Option<DateTime<Utc>>,
}

impl WorkflowStepDto {
    pub fn new(s: StepState, lang: Language) -> Self {
        WorkflowStepDto {
            key: s.definition.key.to_string(),
            number: s.definition.number,
            phase: s.definition.phase.to_string(),
            title: s.definition.title.get(lang),
            status: s.status.to_string(),
            gate_passed: s.gate_passed,
            issues: s.issues.into_iter().map(IssueDto::from).collect(),
            completed_by: s.completed_by,
            completed_at: s.completed_at,
        }
    }
}

#[skip_serializing_none]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowStateDto {
    pub service_id: Uuid,
    pub completion_percent: u8,
    pub current_step_key: Option<String>,
    pub steps: Vec<WorkflowStepDto>,
}

impl WorkflowStateDto {
    pub fn new(s: WorkflowState, lang: Language) -> Self {
        WorkflowStateDto {
            service_id: s.service_id,
            completion_percent: s.completion_percent,
            current_step_key: s.current_step.map(|k| k.to_string()),
            steps: s
                .steps
                .into_iter()
                .map(|st| WorkflowStepDto::new(st, lang))
                .collect(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationReportDto {
    pub service_id: Uuid,
    pub generated_at: DateTime<Utc>,
    pub blocking_count: usize,
    pub warning_count: usize,
    pub issues: Vec<IssueDto>,
}

#[derive(Debug, Default, Deserialize, Validate)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompletionDto {
    #[validate(length(max = 5000))]
    pub comment: Option<String>,
    #[serde(default)]
    pub acknowledge_warnings: bool,
}

async fn get_workflow(
    State(app): State<AppState>,
    ctx: TenantContext,
    headers: HeaderMap,
    ApiPath(service_id): ApiPath<Uuid>,
) -> ApiResult<Json<WorkflowStateDto>> {
    Ok(Json(WorkflowStateDto::new(
        app.workflow.state(&ctx, service_id).await?,
        language_from(&headers),
    )))
}

async fn complete(
    State(app): State<AppState>,
    ctx: TenantContext,
    headers: HeaderMap,
    ApiPath((service_id, step_key)): ApiPath<(Uuid, String)>,
    OptionalJson(dto): OptionalJson<CompletionDto>,
) -> ApiResult<Json<WorkflowStepDto>> {
    let key: WorkflowStepKey = parse_enum(&step_key, "stepKey")?;
    let step = app
        .workflow
        .complete(&ctx, service_id, key, dto.comment, dto.acknowledge_warnings)
        .await?;
    Ok(Json(WorkflowStepDto::new(step, language_from(&headers))))
}

async fn reopen(
    State(app): State<AppState>,
    ctx: TenantContext,
    headers: HeaderMap,
    ApiPath((service_id, step_key)): ApiPath<(Uuid, String)>,
) -> ApiResult<Json<WorkflowStateDto>> {
    let key: WorkflowStepKey = parse_enum(&step_key, "stepKey")?;
    Ok(Json(WorkflowStateDto::new(
        app.workflow.reopen(&ctx, service_id, key).await?,
        language_from(&headers),
    )))
}

async fn validation(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(service_id): ApiPath<Uuid>,
) -> ApiResult<Json<ValidationReportDto>> {
    let issues = app.workflow.validate(&ctx, service_id).await?;
    Ok(Json(ValidationReportDto {
        service_id,
        generated_at: Utc::now(),
        blocking_count: issues
            .iter()
            .filter(|i| i.severity == Severity::Blocking)
            .count(),
        warning_count: issues
            .iter()
            .filter(|i| i.severity == Severity::Warning)
            .count(),
        issues: issues.into_iter().map(IssueDto::from).collect(),
    }))
}
