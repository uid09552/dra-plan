use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use serde_with::{DisplayFromStr, serde_as, skip_serializing_none};
use uuid::Uuid;
use validator::Validate;

use super::application::Export;
use super::domain::{
    AssignmentEntry, MicroserviceBundle, PlanAggregate, PlanStatus, PlanVersion, RunbookWithSteps,
    SnapshotCodec,
};
use crate::app::AppState;
use crate::features::bia::api::BiaDto;
use crate::features::catalog::api::language_from;
use crate::features::data_protection::api::DataProtectionDto;
use crate::features::dependencies::api::DependencyDto;
use crate::features::directory::api::{PersonDto, RoleDto};
use crate::features::it_services::api::ItServiceDto;
use crate::features::microservices::api::MicroserviceDto;
use crate::features::objectives::api::ObjectiveDto;
use crate::features::roles_comm::api::{CommunicationRuleDto, RoleAssignmentDto};
use crate::features::runbooks::api::RunbookDto;
use crate::features::scenarios::api::ScenarioDto;
use crate::features::strategies::api::StrategyDto;
use crate::shared::kernel::{AppError, AppResult, TenantContext};
use crate::shared::web::{ApiPath, ApiQuery, ApiResult, OptionalJson, ValidJson, created};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/services/{service_id}/plan-versions",
            get(list).post(submit),
        )
        .route("/services/{service_id}/handbook", get(draft_handbook))
        .route("/plan-versions/{plan_version_id}", get(get_one))
        .route("/plan-versions/{plan_version_id}/approve", post(approve))
        .route("/plan-versions/{plan_version_id}/reject", post(reject))
        .route("/plan-versions/{plan_version_id}/export", get(export))
}

// ───────────────────────────── Snapshot format ─────────────────────────────

pub const SNAPSHOT_FORMAT_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MicroserviceBundleDto {
    #[serde(flatten)]
    pub microservice: MicroserviceDto,
    pub dependencies: Vec<DependencyDto>,
    pub recovery_objectives: Vec<ObjectiveDto>,
    pub recovery_strategies: Vec<StrategyDto>,
    pub data_protection: Vec<DataProtectionDto>,
    pub runbooks: Vec<RunbookDto>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssignmentEntryDto {
    #[serde(flatten)]
    pub assignment: RoleAssignmentDto,
    pub role_name: String,
    pub person: Option<PersonDto>,
}

/// The published `PlanSnapshot` schema.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanSnapshotDto {
    pub format_version: u32,
    pub service: ItServiceDto,
    pub bia: Option<BiaDto>,
    pub scenarios: Vec<ScenarioDto>,
    pub microservices: Vec<MicroserviceBundleDto>,
    pub role_assignments: Vec<AssignmentEntryDto>,
    pub communication_rules: Vec<CommunicationRuleDto>,
    pub roles: Vec<RoleDto>,
    pub persons: Vec<PersonDto>,
}

/// Adapter for the [`SnapshotCodec`] port, based on the API DTOs (the snapshot format is part of
/// the published contract).
pub struct JsonSnapshotCodec;

impl SnapshotCodec for JsonSnapshotCodec {
    fn encode(&self, a: &PlanAggregate) -> AppResult<String> {
        let a = a.clone();
        let dto = PlanSnapshotDto {
            format_version: SNAPSHOT_FORMAT_VERSION,
            service: ItServiceDto::new(a.service, None),
            bia: a.bia.map(|b| BiaDto::new(b, None)),
            scenarios: a.scenarios.into_iter().map(ScenarioDto::from).collect(),
            microservices: a
                .microservices
                .into_iter()
                .map(|b| MicroserviceBundleDto {
                    microservice: b.microservice.into(),
                    dependencies: b
                        .dependencies
                        .into_iter()
                        .map(|d| DependencyDto::new(d, None))
                        .collect(),
                    recovery_objectives: b.objectives.into_iter().map(ObjectiveDto::from).collect(),
                    recovery_strategies: b
                        .strategies
                        .into_iter()
                        .map(|s| StrategyDto::new(s, None))
                        .collect(),
                    data_protection: b
                        .data_protection
                        .into_iter()
                        .map(|d| DataProtectionDto::new(d, None))
                        .collect(),
                    runbooks: b
                        .runbooks
                        .into_iter()
                        .map(|r| RunbookDto::new(r.runbook, None, Some(r.steps)))
                        .collect(),
                })
                .collect(),
            role_assignments: a
                .role_assignments
                .into_iter()
                .map(|e| AssignmentEntryDto {
                    assignment: e.assignment.into(),
                    role_name: e.role_name,
                    person: e.person.map(PersonDto::from),
                })
                .collect(),
            communication_rules: a
                .communication_rules
                .into_iter()
                .map(CommunicationRuleDto::from)
                .collect(),
            roles: a.roles.into_iter().map(RoleDto::from).collect(),
            persons: a.persons.into_iter().map(PersonDto::from).collect(),
        };
        serde_json::to_string(&dto).map_err(AppError::internal)
    }

    fn decode(&self, snapshot: &str) -> AppResult<PlanAggregate> {
        let dto: PlanSnapshotDto = serde_json::from_str(snapshot)
            .map_err(|e| AppError::internal(format!("unreadable plan snapshot: {e}")))?;
        if dto.format_version != SNAPSHOT_FORMAT_VERSION {
            return Err(AppError::internal(format!(
                "unsupported snapshot format {}",
                dto.format_version
            )));
        }
        Ok(PlanAggregate {
            service: dto.service.into(),
            bia: dto.bia.map(TryInto::try_into).transpose()?,
            scenarios: dto
                .scenarios
                .into_iter()
                .map(TryInto::try_into)
                .collect::<AppResult<_>>()?,
            microservices: dto
                .microservices
                .into_iter()
                .map(|b| {
                    Ok(MicroserviceBundle {
                        microservice: b.microservice.into(),
                        dependencies: b
                            .dependencies
                            .into_iter()
                            .map(TryInto::try_into)
                            .collect::<AppResult<_>>()?,
                        objectives: b
                            .recovery_objectives
                            .into_iter()
                            .map(TryInto::try_into)
                            .collect::<AppResult<_>>()?,
                        strategies: b
                            .recovery_strategies
                            .into_iter()
                            .map(TryInto::try_into)
                            .collect::<AppResult<_>>()?,
                        data_protection: b
                            .data_protection
                            .into_iter()
                            .map(TryInto::try_into)
                            .collect::<AppResult<_>>()?,
                        runbooks: b
                            .runbooks
                            .into_iter()
                            .map(|r| {
                                r.into_domain()
                                    .map(|(runbook, steps)| RunbookWithSteps { runbook, steps })
                            })
                            .collect::<AppResult<_>>()?,
                    })
                })
                .collect::<AppResult<_>>()?,
            role_assignments: dto
                .role_assignments
                .into_iter()
                .map(|e| AssignmentEntry {
                    assignment: e.assignment.into(),
                    role_name: e.role_name,
                    person: e.person.map(Into::into),
                })
                .collect(),
            communication_rules: dto
                .communication_rules
                .into_iter()
                .map(TryInto::try_into)
                .collect::<AppResult<_>>()?,
            roles: dto.roles.into_iter().map(Into::into).collect(),
            persons: dto.persons.into_iter().map(Into::into).collect(),
        })
    }
}

// ───────────────────────────── DTOs ─────────────────────────────

#[serde_as]
#[skip_serializing_none]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanVersionDto {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub service_id: Option<Uuid>,
    pub version: u32,
    #[serde_as(as = "DisplayFromStr")]
    pub status: PlanStatus,
    pub submitted_by: String,
    pub submitted_at: DateTime<Utc>,
    pub approved_by: Option<String>,
    pub approved_at: Option<DateTime<Utc>>,
    pub next_review_due: Option<NaiveDate>,
    pub comment: Option<String>,
    pub review_comment: Option<String>,
    pub snapshot: Option<Box<RawValue>>,
}

impl PlanVersionDto {
    pub fn new(v: PlanVersion) -> AppResult<Self> {
        let snapshot = v
            .snapshot
            .map(|s| RawValue::from_string(s).map_err(AppError::internal))
            .transpose()?;
        Ok(PlanVersionDto {
            id: v.id,
            tenant_id: v.tenant_id.0,
            service_id: v.service_id,
            version: v.plan_number,
            status: v.status,
            submitted_by: v.submitted_by,
            submitted_at: v.submitted_at,
            approved_by: v.approved_by,
            approved_at: v.approved_at,
            next_review_due: v.next_review_due,
            comment: v.comment,
            review_comment: v.review_comment,
            snapshot,
        })
    }
}

#[derive(Debug, Default, Deserialize, Validate)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubmissionDto {
    #[validate(length(max = 5000))]
    pub comment: Option<String>,
}

#[derive(Debug, Default, Deserialize, Validate)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApprovalDto {
    #[validate(length(max = 5000))]
    pub comment: Option<String>,
    pub next_review_due: Option<NaiveDate>,
}

#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RejectionDto {
    #[validate(length(min = 1, max = 5000))]
    pub comment: String,
}

#[serde_as]
#[derive(Debug, Deserialize)]
pub struct ListQuery {
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[serde(default)]
    pub status: Option<PlanStatus>,
}

#[derive(Debug, Deserialize)]
pub struct ExportQuery {
    pub format: String,
}

// ───────────────────────────── Handlers ─────────────────────────────

async fn list(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(service_id): ApiPath<Uuid>,
    ApiQuery(q): ApiQuery<ListQuery>,
) -> ApiResult<Json<Vec<PlanVersionDto>>> {
    let versions = app.plans.list(&ctx, service_id, q.status).await?;
    Ok(Json(
        versions
            .into_iter()
            .map(PlanVersionDto::new)
            .collect::<AppResult<_>>()?,
    ))
}

async fn submit(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(service_id): ApiPath<Uuid>,
    OptionalJson(dto): OptionalJson<SubmissionDto>,
) -> ApiResult<Response> {
    let v = app.plans.submit(&ctx, service_id, dto.comment).await?;
    Ok(created(
        format!("/plan-versions/{}", v.id),
        None,
        PlanVersionDto::new(v)?,
    ))
}

async fn get_one(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(id): ApiPath<Uuid>,
) -> ApiResult<Json<PlanVersionDto>> {
    Ok(Json(PlanVersionDto::new(app.plans.get(&ctx, id).await?)?))
}

async fn approve(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(id): ApiPath<Uuid>,
    OptionalJson(dto): OptionalJson<ApprovalDto>,
) -> ApiResult<Json<PlanVersionDto>> {
    Ok(Json(PlanVersionDto::new(
        app.plans
            .approve(&ctx, id, dto.comment, dto.next_review_due)
            .await?,
    )?))
}

async fn reject(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(id): ApiPath<Uuid>,
    ValidJson(dto): ValidJson<RejectionDto>,
) -> ApiResult<Json<PlanVersionDto>> {
    Ok(Json(PlanVersionDto::new(
        app.plans.reject(&ctx, id, dto.comment).await?,
    )?))
}

async fn export(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(id): ApiPath<Uuid>,
    ApiQuery(q): ApiQuery<ExportQuery>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    let markdown = match q.format.as_str() {
        "markdown" => true,
        "json" => false,
        "pdf" => {
            return Err(AppError::NotImplemented(
                "PDF export is not implemented yet; use markdown".into(),
            )
            .into());
        }
        other => {
            return Err(AppError::invalid(
                "INVALID_VALUE",
                Some("format"),
                format!("unknown format `{other}`"),
            )
            .into());
        }
    };
    match app
        .plans
        .export(&ctx, id, markdown, language_from(&headers))
        .await?
    {
        Export::Json(v) => Ok(Json(PlanVersionDto::new(v)?).into_response()),
        Export::Markdown(text) => {
            let mut response = text.into_response();
            let headers = response.headers_mut();
            headers.insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/markdown; charset=utf-8"),
            );
            if let Ok(v) =
                HeaderValue::from_str(&format!("attachment; filename=\"dr-plan-{id}.md\""))
            {
                headers.insert(header::CONTENT_DISPOSITION, v);
            }
            Ok(response)
        }
    }
}

/// Live preview of the emergency handbook from the current working data (always Markdown).
async fn draft_handbook(
    State(app): State<AppState>,
    ctx: TenantContext,
    ApiPath(service_id): ApiPath<Uuid>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    let text = app
        .plans
        .draft_handbook(&ctx, service_id, language_from(&headers))
        .await?;
    let mut response = text.into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/markdown; charset=utf-8"),
    );
    Ok(response)
}
